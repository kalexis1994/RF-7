//! RackForge adapter. Device access and persistence remain host responsibilities.
//!
//! Beyond wiring, this adapter does two things. It writes the program catalog
//! at run time, because the programs RackForge offers are the voices of
//! whatever cartridge the user installed plus whatever they saved. And it
//! speaks the host's program-editing contract, so a voice can be edited from
//! any RackForge surface — the desktop window, the web shell, a controller's
//! display — without a command line anywhere.

mod catalog;
pub mod document;
pub mod editor;
pub mod parameters;
pub mod programs;

use document::{
    EditorValue, PROGRAM_EDIT_SCHEMA_VERSION, PROGRAM_SCHEMA_VERSION, PreparedProgram,
    ProgramDocument, ProgramEditRequest, ProgramFieldEditRequest, pad_name,
};
use programs::{CustomProgram, CustomPrograms};
use rackforge_plugin_sdk::{
    BlockContext, MIDI_FAMILY_BEND, MIDI_FAMILY_CONTROL, MIDI_FAMILY_NOTE, MIDI_FAMILY_PRESSURE,
    MIDI_FAMILY_PROGRAM, MIDI2_FLAG_ORIGIN_7BIT, MIDI2_KIND_CHANNEL_PRESSURE,
    MIDI2_KIND_CONTROL_CHANGE, MIDI2_KIND_NOTE_OFF, MIDI2_KIND_NOTE_ON, MIDI2_KIND_PITCH_BEND,
    MIDI2_KIND_PROGRAM_CHANGE, MidiEvent, MidiEvent2, PROGRAM_EDIT_BASIC, PROGRAM_EDIT_DECLARATIVE,
    PROGRAM_EDIT_PREVIEW, ParallelProcessor, PlanWriter, UnitContext, UnitMix,
    export_parallel_processor,
};
use rf7_dsp::{DISPATCH_STRIDE, Engine, MAX_BLOCK_FRAMES, POLYPHONY, SHARED_CAPACITY, Unit};
use rf7_voice::{Library, MAX_VOICES, SCRATCH_BYTES, Voice, decode_cartridge, factory_library};
use serde::Serialize;

pub const MAX_FRAMES: u32 = 4096;
pub const MAX_EVENTS: usize = 256;
/// Sized for the editor: a hundred and forty fields with their labels and
/// hints, and the thirty-two algorithm options, come to about twenty
/// kilobytes.
pub const TRANSFER_BYTES: usize = 262_144;
/// The largest library file RF-7 will take from the host. Four cartridges of
/// bulk dumps is already past [`MAX_VOICES`]; the rest is slack so an oversized
/// collection is read and capped rather than refused for its size alone.
pub const MAX_RESOURCE_BYTES: usize = 8 * 1_048_576;
/// Version 3 adds the saved program the user had selected, and is written only
/// when one is. Version 2 carries every parameter; version 1, which carried
/// only the gain and the program, is still accepted so a session saved by
/// 0.1.0 opens.
pub const STATE_VERSION: u32 = 2;
const STATE_VERSION_FIRST: u32 = 1;
const STATE_VERSION_SELECTED_CUSTOM: u32 = 3;
const STATE_HEADER: usize = 16;
const STATE_FIRST_BYTES: usize = 20;
const STATE_ID_MAX: usize = 255;
pub const PARAMETER_GAIN: u32 = parameters::GAIN;
pub const PARAMETER_COUNT: usize = parameters::COUNT;
/// Cartridges RF-7 holds at once. The instrument had one slot; this is a
/// rack of bays, and what plays is all of them in order.
pub const CARTRIDGE_BAYS: usize = 8;
/// The resource each bay is declared as. Bay one keeps the name RF-7 has
/// always used, so a cartridge installed before there were bays stays put.
pub const CARTRIDGE_RESOURCES: [&str; CARTRIDGE_BAYS] = [
    "cartridge",
    "cartridge-2",
    "cartridge-3",
    "cartridge-4",
    "cartridge-5",
    "cartridge-6",
    "cartridge-7",
    "cartridge-8",
];
pub const RESOURCE_CARTRIDGE: &str = CARTRIDGE_RESOURCES[0];

/// Which bay a resource id names.
pub fn cartridge_bay(id: &str) -> Option<usize> {
    CARTRIDGE_RESOURCES.iter().position(|name| *name == id)
}
/// What the editor opens when asked for a program that does not exist yet.
const NEW_PROGRAM_NAME: &str = "RF NEW";

pub struct Rf7Processor {
    engine: Option<Box<Engine>>,
    library: Library,
    program: usize,
    /// Programs saved from the editor, installed by the host.
    custom: CustomPrograms,
    /// The saved program currently playing, if the selection is one rather
    /// than a library slot. `program` keeps the last library selection so a
    /// program change lands where it did before.
    selected_custom: Option<String>,
    /// One entry per declared parameter, always in its declared range.
    values: [f64; PARAMETER_COUNT],
    /// A cartridge arriving in pieces on the control thread, and the bay it
    /// is going into.
    incoming: Vec<u8>,
    receiving: Option<usize>,
    /// How many voices each bay put into the library, and how many the file
    /// in it held — which can be more than the library had room for.
    bay_length: [usize; CARTRIDGE_BAYS],
    bay_found: [usize; CARTRIDGE_BAYS],
    maximum_frames: u32,
    channels: u32,
    /// Whether the block being rendered passed the pre-stage's checks; a
    /// block that did not is silence, whatever the units did.
    block_valid: bool,
}

impl Default for Rf7Processor {
    fn default() -> Self {
        Self {
            engine: None,
            library: factory_library(),
            program: 0,
            custom: CustomPrograms::default(),
            selected_custom: None,
            values: parameters::defaults(),
            incoming: Vec::new(),
            receiving: None,
            bay_length: [0; CARTRIDGE_BAYS],
            bay_found: [0; CARTRIDGE_BAYS],
            maximum_frames: 0,
            channels: 0,
            block_valid: false,
        }
    }
}

impl Rf7Processor {
    /// The programs RackForge should offer, as Preset Catalog JSON.
    pub fn catalog(&self, destination: &mut [u8]) -> Option<usize> {
        catalog::write(&self.library, &self.bay_length, &self.custom, destination)
    }

    /// How many voices each bay contributed, in bay order.
    pub fn bays(&self) -> &[usize; CARTRIDGE_BAYS] {
        &self.bay_length
    }

    /// Put a cartridge in a bay, or take the one there out.
    ///
    /// A bay's voices are one stretch of the library, so this replaces that
    /// stretch and leaves every other bay where it was — which is what keeps
    /// a program number pointing at the same program when another bay
    /// changes. With every bay empty the factory bank plays again.
    fn install_bay(&mut self, bay: usize, cartridge: &Library) {
        if bay >= CARTRIDGE_BAYS {
            return;
        }
        if self.bay_length.iter().all(|length| *length == 0) {
            // The factory bank is standing in; the first cartridge is not
            // spliced into it, it replaces it.
            self.library = Library::from_voices(&[]);
        }
        let at: usize = self.bay_length[..bay].iter().sum();
        let remove = self.bay_length[bay];
        self.bay_length[bay] = self.library.splice(at, remove, cartridge);
        self.bay_found[bay] = cartridge.found();
        if self.bay_length.iter().all(|length| *length == 0) {
            self.library = factory_library();
            self.bay_found = [0; CARTRIDGE_BAYS];
        } else {
            self.library.set_found(self.bay_found.iter().sum());
        }
        self.adopt();
    }

    /// How many library programs this instance currently offers.
    pub fn program_count(&self) -> usize {
        self.library.len()
    }

    /// The programs saved from the editor, in catalog order.
    pub fn custom_programs(&self) -> &[CustomProgram] {
        self.custom.entries()
    }

    /// The catalog id of the sound now playing.
    pub fn selected_sound_id(&self) -> String {
        match &self.selected_custom {
            Some(id) => format!("{}{id}", programs::PREFIX),
            None => format!("program-{:03}", self.program + 1),
        }
    }

    fn midi1(&mut self, event: &MidiEvent) {
        let Some(engine) = &mut self.engine else {
            return;
        };
        let [status, index, value] = event.data;
        let channel = status & 15;
        match status & 0xf0 {
            0x90 => engine.note_on(channel, index, value),
            0x80 => engine.note_off(channel, index),
            0xb0 => engine.control_change(channel, index, f64::from(value) / 127.0),
            0xc0 => {
                self.program_change(usize::from(index));
            }
            0xd0 => engine.channel_pressure(channel, f64::from(index) / 127.0),
            0xe0 => {
                let raw = i32::from(index) | (i32::from(value) << 7);
                engine.pitch_bend(channel, f64::from(raw - 8192) / 8192.0);
            }
            _ => {}
        }
    }

    fn midi2(&mut self, event: &MidiEvent2) {
        let Some(engine) = &mut self.engine else {
            return;
        };
        match event.kind {
            MIDI2_KIND_NOTE_ON => {
                let velocity = if event.flags & MIDI2_FLAG_ORIGIN_7BIT != 0 {
                    (event.value >> 9) as u8
                } else {
                    // A genuine MIDI 2.0 Note On with zero velocity is not
                    // Note Off, so the quietest wide note still sounds.
                    ((event.value.max(1) >> 9) as u8).max(1)
                };
                engine.note_on(event.channel, event.index, velocity);
            }
            MIDI2_KIND_NOTE_OFF => engine.note_off(event.channel, event.index),
            MIDI2_KIND_CONTROL_CHANGE => {
                let value = if event.flags & MIDI2_FLAG_ORIGIN_7BIT != 0 {
                    f64::from(event.value >> 25) / 127.0
                } else {
                    f64::from(event.value) / f64::from(u32::MAX)
                };
                engine.control_change(event.channel, event.index, value);
            }
            MIDI2_KIND_PROGRAM_CHANGE => {
                self.program_change(usize::from(event.index));
            }
            MIDI2_KIND_PITCH_BEND => {
                let centred = f64::from(event.value) - f64::from(u32::MAX) / 2.0;
                engine.pitch_bend(event.channel, centred / (f64::from(u32::MAX) / 2.0));
            }
            MIDI2_KIND_CHANNEL_PRESSURE => {
                let value = if event.flags & MIDI2_FLAG_ORIGIN_7BIT != 0 {
                    f64::from(event.value >> 25) / 127.0
                } else {
                    f64::from(event.value) / f64::from(u32::MAX)
                };
                engine.channel_pressure(event.channel, value);
            }
            _ => {}
        }
    }

    /// A MIDI program change. The numbers up to the library's length are its
    /// slots; the numbers past it reach the saved programs in catalog order,
    /// so a controller can call up a program written in the editor as well as
    /// one from the cartridge. A number nothing answers to changes nothing.
    fn program_change(&mut self, number: usize) -> bool {
        if number < self.library.len() {
            return self.select_library_program(number);
        }
        let Some(program) = self.custom.entries().get(number - self.library.len()) else {
            return false;
        };
        let id = program.id.clone();
        self.select_custom_program(&id)
    }

    /// A program change from the catalog: a library slot.
    fn select_library_program(&mut self, program: usize) -> bool {
        if program >= self.library.len() {
            return false;
        }
        self.program = program;
        self.selected_custom = None;
        if let Some(engine) = &mut self.engine {
            engine.select_program(program);
        }
        true
    }

    /// A saved program from the catalog.
    fn select_custom_program(&mut self, id: &str) -> bool {
        let Some(program) = self.custom.find(id) else {
            return false;
        };
        let voice = program.voice;
        self.selected_custom = Some(id.to_owned());
        if let Some(engine) = &mut self.engine {
            engine.load_patch(voice);
        }
        true
    }

    /// Push every parameter into the engine at once. Cheap enough to run on
    /// any change, which keeps one path instead of one per parameter.
    fn apply(&mut self) {
        let controls = parameters::controls(&self.values);
        let gain = self.values[parameters::GAIN as usize];
        if let Some(engine) = &mut self.engine {
            engine.set_gain(gain);
            engine.set_controls(controls);
        }
    }

    /// Take the library as it now stands into the engine.
    fn adopt(&mut self) {
        self.program = self.program.min(self.library.len().saturating_sub(1));
        if let Some(engine) = &mut self.engine {
            engine.load_library(self.library.clone());
            engine.select_program(self.program);
        }
        // A saved program is not part of the cartridge, so it keeps playing.
        if let Some(id) = self.selected_custom.clone() {
            self.select_custom_program(&id);
        }
    }

    /// Restore whatever is selected into a freshly built engine.
    fn restore_selection(&mut self) {
        match self.selected_custom.clone() {
            Some(id) if self.custom.find(&id).is_some() => {
                self.select_custom_program(&id);
            }
            _ => {
                self.selected_custom = None;
                if let Some(engine) = &mut self.engine {
                    engine.select_program(self.program);
                }
            }
        }
    }

    /// The voice behind a catalog id: a library slot or a saved program.
    fn voice_for_catalog_id(&self, id: &str) -> Option<(Voice, Option<String>)> {
        if let Some(document_id) = CustomPrograms::document_id(id) {
            let program = self.custom.find(document_id)?;
            let mut voice = program.voice;
            voice.name = pad_name(&program.name);
            return Some((voice, Some(document_id.to_owned())));
        }
        let slot = catalog::program_index(id)?;
        Some((*self.library.voice(slot)?, None))
    }

    /// A prepared program for a document, or `None` if the document is not
    /// one of ours. The document's name wins over the voice's ten bytes, so
    /// renaming in the host renames the exported dump too.
    fn prepare_document(&self, mut document: ProgramDocument) -> Option<PreparedProgram> {
        if document.schema_version != PROGRAM_SCHEMA_VERSION {
            return None;
        }
        let mut voice = document::voice_of(&document)?;
        if document.name.trim().is_empty() {
            document.name = NEW_PROGRAM_NAME.to_owned();
        }
        voice.name = pad_name(document.name.trim());
        let preview = format!("{}{}", programs::PREFIX, document.id);
        let mut prepared = document::prepared(
            document::document_with_id_and_name(&document.id, &document.name, &voice),
            &preview,
            &voice,
        );
        prepared
            .artifacts
            .extend(self.export_artifacts(&document.id, &voice));
        Some(prepared)
    }

    /// The saved programs as they will stand once `id` is saved as `voice`,
    /// and the factory library while it is the library, as DX7 bulk dumps
    /// for the host to write beside the save: the way a program made here,
    /// or the bank itself, gets to a cartridge or to another instrument.
    fn export_artifacts(&self, id: &str, voice: &Voice) -> Vec<document::ProgramArtifact> {
        let mut saved: Vec<Voice> = self
            .custom
            .entries()
            .iter()
            .map(|program| {
                let mut voice = program.voice;
                voice.name = pad_name(&program.name);
                voice
            })
            .collect();
        match self.custom.entries().iter().position(|p| p.id == id) {
            Some(slot) => saved[slot] = *voice,
            None => saved.push(*voice),
        }
        let mut artifacts = document::export_artifacts("rf7-programs", &saved);
        let factory = self.library.len() == rf7_voice::FACTORY_VOICES
            && (0..rf7_voice::FACTORY_VOICES)
                .all(|i| self.library.voice(i) == Some(&rf7_voice::factory_voice(i)));
        if factory {
            let voices: Vec<Voice> = (0..rf7_voice::FACTORY_VOICES)
                .map(rf7_voice::factory_voice)
                .collect();
            artifacts.extend(document::export_artifacts("rf7-factory", &voices));
        }
        artifacts
    }
}

/// Serialise into the host's buffer, or write nothing.
fn emit<T: Serialize>(value: &T, destination: &mut [u8]) -> Option<usize> {
    let bytes = serde_json::to_vec(value).ok()?;
    let slot = destination.get_mut(..bytes.len())?;
    slot.copy_from_slice(&bytes);
    Some(bytes.len())
}

impl ParallelProcessor for Rf7Processor {
    type Unit = Unit;

    fn prepare(&mut self, rate: f64, frames: u32, inputs: u32, outputs: u32) -> bool {
        if frames == 0 || frames > MAX_FRAMES || inputs != 0 || !(1..=2).contains(&outputs) {
            return false;
        }
        let Ok(mut engine) = Engine::new(rate as f32) else {
            return false;
        };
        engine.load_library(self.library.clone());
        self.engine = Some(Box::new(engine));
        self.restore_selection();
        self.apply();
        self.maximum_frames = frames;
        self.channels = outputs;
        true
    }

    fn set_parameter(&mut self, index: u32, value: f64) -> bool {
        if !parameters::is_valid(index, value) {
            return false;
        }
        self.values[index as usize] = value;
        self.apply();
        true
    }

    fn get_parameter(&self, index: u32) -> Option<f64> {
        self.values.get(index as usize).copied()
    }

    fn reset(&mut self) {
        if let Some(engine) = &mut self.engine {
            engine.reset();
        }
    }

    fn begin_resource(&mut self, id: &str, total_bytes: u64) -> bool {
        let Some(bay) = cartridge_bay(id).filter(|_| total_bytes <= MAX_RESOURCE_BYTES as u64)
        else {
            return false;
        };
        self.incoming.clear();
        self.incoming.reserve(total_bytes as usize);
        self.receiving = Some(bay);
        true
    }

    fn write_resource(&mut self, offset: u64, bytes: &[u8]) -> bool {
        // Only sequential delivery is accepted: a gap would leave the buffer
        // holding whatever happened to be at that offset before.
        if self.receiving.is_none()
            || offset != self.incoming.len() as u64
            || self.incoming.len() + bytes.len() > MAX_RESOURCE_BYTES
        {
            self.receiving = None;
            return false;
        }
        self.incoming.extend_from_slice(bytes);
        true
    }

    fn end_resource(&mut self) -> bool {
        let Some(bay) = self.receiving.take() else {
            return false;
        };
        // Bulk dumps, a single voice, a headerless chip image, or a ZIP of
        // any of those: whichever shape the user installed, the programs
        // come out the same way. The scratch an archive is unpacked through
        // is taken here, on the control thread, and given back at once.
        let mut scratch = vec![0; SCRATCH_BYTES];
        let Ok(cartridge) = decode_cartridge(&self.incoming, &mut scratch) else {
            self.incoming = Vec::new();
            return false;
        };
        self.incoming = Vec::new();
        self.install_bay(bay, &cartridge);
        true
    }

    fn write_program_catalog(&mut self, destination: &mut [u8]) -> Option<usize> {
        catalog::write(&self.library, &self.bay_length, &self.custom, destination)
    }

    fn load_preset(&mut self, id: &str) -> bool {
        if let Some(document_id) = CustomPrograms::document_id(id) {
            return self.select_custom_program(document_id);
        }
        match catalog::program_index(id) {
            Some(program) => self.select_library_program(program),
            None => false,
        }
    }

    fn save_state(&self, destination: &mut [u8]) -> Option<usize> {
        let custom = self
            .selected_custom
            .as_deref()
            .filter(|id| id.len() <= STATE_ID_MAX);
        let length = STATE_HEADER + PARAMETER_COUNT * 8 + custom.map_or(0, |id| 1 + id.len());
        let bytes = destination.get_mut(..length)?;
        bytes[..4].copy_from_slice(b"RF7A");
        let version = if custom.is_some() {
            STATE_VERSION_SELECTED_CUSTOM
        } else {
            STATE_VERSION
        };
        bytes[4..8].copy_from_slice(&version.to_le_bytes());
        bytes[8..12].copy_from_slice(&(self.program as u32).to_le_bytes());
        bytes[12..16].copy_from_slice(&(PARAMETER_COUNT as u32).to_le_bytes());
        for (index, value) in self.values.iter().enumerate() {
            bytes[STATE_HEADER + index * 8..][..8].copy_from_slice(&value.to_le_bytes());
        }
        if let Some(id) = custom {
            let start = STATE_HEADER + PARAMETER_COUNT * 8;
            bytes[start] = id.len() as u8;
            bytes[start + 1..].copy_from_slice(id.as_bytes());
        }
        Some(length)
    }

    fn load_state(&mut self, state: &[u8]) -> bool {
        if state.len() < 8 || &state[..4] != b"RF7A" {
            return false;
        }
        let version = u32::from_le_bytes(state[4..8].try_into().expect("checked length"));
        // Everything is validated before anything is applied, so a rejected
        // state leaves the instrument exactly as it was.
        let (program, values, custom) = match version {
            STATE_VERSION_FIRST => match read_first_state(state) {
                Some((program, values)) => (program, values, None),
                None => return false,
            },
            STATE_VERSION => match read_state(state) {
                Some((program, values)) => (program, values, None),
                None => return false,
            },
            STATE_VERSION_SELECTED_CUSTOM => match read_state_with_custom(state) {
                Some(triple) => triple,
                None => return false,
            },
            _ => return false,
        };
        // A state can name a program a shorter library does not have; the
        // level and the controls still restore, and the selection falls back.
        self.program = program.min(self.library.len().saturating_sub(1));
        self.values = values;
        // The saved program may not have been installed yet, or may have been
        // deleted in the host; then the library slot plays instead.
        self.selected_custom = custom.filter(|id| self.custom.find(id).is_some());
        self.restore_selection();
        self.apply();
        true
    }

    fn program_editing_capabilities(&self) -> u32 {
        PROGRAM_EDIT_BASIC | PROGRAM_EDIT_PREVIEW | PROGRAM_EDIT_DECLARATIVE
    }

    /// Open the editor: on a saved program under its own id, on a library
    /// voice as a copy with a new id, or on the initial voice when no program
    /// is named.
    fn begin_program_edit(&mut self, request: &[u8], destination: &mut [u8]) -> Option<usize> {
        let request: ProgramEditRequest = serde_json::from_slice(request).ok()?;
        if request.schema_version != PROGRAM_EDIT_SCHEMA_VERSION {
            return None;
        }
        let (voice, id, name) = match request.program_id.as_deref() {
            None => {
                let mut voice = Voice::init();
                voice.name = pad_name(NEW_PROGRAM_NAME);
                (voice, self.custom.next_id(), NEW_PROGRAM_NAME.to_owned())
            }
            Some(catalog_id) => {
                let (voice, existing) = self.voice_for_catalog_id(catalog_id)?;
                match existing {
                    Some(id) => {
                        let name = self.custom.find(&id)?.name.clone();
                        (voice, id, name)
                    }
                    None => {
                        let name = rf7_voice::printable_name(&voice.name).to_owned();
                        (voice, self.custom.next_id(), name)
                    }
                }
            }
        };
        let document = document::document_with_id_and_name(&id, &name, &voice);
        let prepared = self.prepare_document(document)?;
        emit(&prepared, destination)
    }

    fn prepare_program_save(&mut self, document: &[u8], destination: &mut [u8]) -> Option<usize> {
        let document: ProgramDocument = serde_json::from_slice(document).ok()?;
        let prepared = self.prepare_document(document)?;
        emit(&prepared, destination)
    }

    fn program_editor_view(&mut self, document: &[u8], destination: &mut [u8]) -> Option<usize> {
        let document: ProgramDocument = serde_json::from_slice(document).ok()?;
        let mut voice = document::voice_of(&document)?;
        voice.name = pad_name(document.name.trim());
        emit(&editor::view(&voice), destination)
    }

    fn apply_program_edit(&mut self, request: &[u8], destination: &mut [u8]) -> Option<usize> {
        let request: ProgramFieldEditRequest = serde_json::from_slice(request).ok()?;
        if request.schema_version != PROGRAM_EDIT_SCHEMA_VERSION {
            return None;
        }
        let voice = document::voice_of(&request.document)?;
        let value = match &request.value {
            EditorValue::Inherited => return None,
            other => other,
        };
        let edited = editor::apply(&voice, &request.field_id, value)?;
        let document = document::document_with_id_and_name(
            &request.document.id,
            &request.document.name,
            &edited,
        );
        let prepared = self.prepare_document(document)?;
        emit(&prepared, destination)
    }

    /// The host has saved the program; it joins the catalog under
    /// `custom.<id>`. If it is the one playing, the engine takes the new
    /// version at once.
    fn install_program(&mut self, prepared: &[u8]) -> bool {
        let prepared: PreparedProgram = match serde_json::from_slice(prepared) {
            Ok(prepared) => prepared,
            Err(_) => return false,
        };
        let Some(prepared) = self.prepare_document(prepared.document) else {
            return false;
        };
        let Some(voice) = document::voice_of(&prepared.document) else {
            return false;
        };
        let id = prepared.document.id.clone();
        if !self.custom.install(CustomProgram {
            id: id.clone(),
            name: prepared.document.name.clone(),
            voice,
        }) {
            return false;
        }
        if self.selected_custom.as_deref() == Some(id.as_str()) {
            self.select_custom_program(&id);
        }
        true
    }

    /// Play the draft. The selection is untouched: the host restores the
    /// previous sound when the edit ends, and a program change meanwhile
    /// lands where it would have.
    fn preview_program(&mut self, prepared: &[u8]) -> bool {
        let prepared: PreparedProgram = match serde_json::from_slice(prepared) {
            Ok(prepared) => prepared,
            Err(_) => return false,
        };
        let Some(mut voice) = document::voice_of(&prepared.document) else {
            return false;
        };
        voice.name = pad_name(prepared.document.name.trim());
        match &mut self.engine {
            Some(engine) => {
                engine.load_patch(voice);
                true
            }
            None => false,
        }
    }

    /// The pre-stage: every event at its frame, the performance of every
    /// frame into the shared payload, and one dispatch payload per voice
    /// that has something to do. A block the plugin cannot accept plans
    /// nothing and is rendered as silence.
    fn begin_block(&mut self, context: &BlockContext<'_>, plan: &mut PlanWriter<'_>) {
        let frames = context.frames;
        self.block_valid = false;
        let Some(mut engine) = self.engine.take() else {
            return;
        };
        let accepted = frames > 0
            && frames <= self.maximum_frames
            && context.input_channels == 0
            && context.output_channels == self.channels
            && ordered(context.midi.iter().map(|event| event.frame), frames)
            && ordered(context.midi2.iter().map(|event| event.frame), frames)
            && ordered(context.parameters.iter().map(|event| event.frame), frames)
            && context.midi.iter().all(valid_midi1)
            && context
                .midi2
                .iter()
                .all(|event| event.channel < 16 && event.index < 128)
            && context
                .parameters
                .iter()
                .all(|event| parameters::is_valid(event.index, event.value))
            && engine.begin_block(frames);
        self.engine = Some(engine);
        if !accepted {
            return;
        }
        let (mut p, mut m, mut w) = (0, 0, 0);
        for frame in 0..frames {
            self.engine
                .as_mut()
                .expect("prepared engine")
                .set_frame(frame);
            // Explicit stable tie order: parameters, MIDI 1.0, then MIDI 2.0.
            while p < context.parameters.len() && context.parameters[p].frame == frame {
                self.set_parameter(context.parameters[p].index, context.parameters[p].value);
                p += 1;
            }
            while m < context.midi.len() && context.midi[m].frame == frame {
                self.midi1(&context.midi[m]);
                m += 1;
            }
            while w < context.midi2.len() && context.midi2[w].frame == frame {
                self.midi2(&context.midi2[w]);
                w += 1;
            }
            self.engine
                .as_mut()
                .expect("prepared engine")
                .advance_frame();
        }
        let engine = self.engine.as_ref().expect("prepared engine");
        let shared = engine.shared();
        let buffer = plan.shared_buffer();
        if buffer.len() < shared.len() {
            return;
        }
        buffer[..shared.len()].copy_from_slice(shared);
        if !plan.commit_shared(shared.len()) {
            return;
        }
        let mut payload = [0_u8; DISPATCH_STRIDE];
        for slot in 0..POLYPHONY {
            if let Some(length) = engine.dispatch(slot, &mut payload) {
                plan.activate(slot as u32, &payload[..length]);
            }
        }
        self.block_valid = true;
    }

    /// One voice, from its payload and the shared payload alone. The voice
    /// is mono; every output channel gets the same samples.
    fn render_unit(
        _unit_index: u32,
        unit: &mut Unit,
        payload: &[u8],
        context: &UnitContext<'_>,
        output: &mut [f32],
    ) {
        let frames = (context.frames as usize).min(MAX_BLOCK_FRAMES);
        let channels = context.output_channels as usize;
        let samples = frames * channels;
        if channels == 0 || output.len() < samples {
            output.fill(0.0);
            return;
        }
        let mut mono = [0.0_f32; MAX_BLOCK_FRAMES];
        if !unit.render(payload, context.shared, &mut mono[..frames]) {
            output[..samples].fill(0.0);
            return;
        }
        for (frame, sample) in mono[..frames].iter().enumerate() {
            output[frame * channels..][..channels].fill(*sample);
        }
    }

    /// The post-stage: the voices summed in slot order, the output scale,
    /// and what the engine learns from the audio about which notes ended.
    fn end_block(
        &mut self,
        mix: &UnitMix<'_>,
        output: &mut [f32],
        frames: u32,
        output_channels: u32,
    ) {
        let samples = frames as usize * output_channels as usize;
        let valid = self.block_valid;
        let Some(engine) = self.engine.as_mut().filter(|_| valid) else {
            let length = samples.min(output.len());
            output[..length].fill(0.0);
            return;
        };
        engine.end_block(
            output_channels as usize,
            mix.active_units()
                .map(|unit| (unit as usize, mix.slot(unit))),
            &mut output[..samples],
        );
    }
}

fn ordered(frames: impl Iterator<Item = u32>, block_frames: u32) -> bool {
    let mut previous = 0;
    let mut count = 0;
    for frame in frames {
        count += 1;
        if count > MAX_EVENTS || frame >= block_frames || frame < previous {
            return false;
        }
        previous = frame;
    }
    true
}

fn valid_midi1(event: &MidiEvent) -> bool {
    if !(1..=3).contains(&event.length) || event.data[0] < 128 {
        return false;
    }
    if event.data[1..event.length as usize]
        .iter()
        .any(|byte| *byte >= 128)
    {
        return false;
    }
    match event.data[0] & 0xf0 {
        0x80 | 0x90 | 0xb0 | 0xe0 => event.length == 3,
        0xc0 | 0xd0 => event.length == 2,
        _ => true,
    }
}

/// The program, count and values of a version 2 or 3 state, given how many
/// bytes follow the values.
///
/// A state written by a future RF-7 with more parameters is refused rather
/// than half-read; one written by an earlier RF-7 with fewer is accepted, and
/// the parameters it never heard of keep their defaults.
fn read_values(state: &[u8], trailing: usize) -> Option<(usize, [f64; PARAMETER_COUNT])> {
    if state.len() < STATE_HEADER {
        return None;
    }
    let program = u32::from_le_bytes(state[8..12].try_into().ok()?) as usize;
    let count = u32::from_le_bytes(state[12..16].try_into().ok()?) as usize;
    if program >= MAX_VOICES
        || count > PARAMETER_COUNT
        || state.len() != STATE_HEADER + count * 8 + trailing
    {
        return None;
    }
    let mut values = parameters::defaults();
    for index in 0..count {
        let bytes = state[STATE_HEADER + index * 8..][..8].try_into().ok()?;
        let value = f64::from_le_bytes(bytes);
        if !parameters::is_valid(index as u32, value) {
            return None;
        }
        values[index] = value;
    }
    Some((program, values))
}

/// A version 2 state: program, count, then that many values.
fn read_state(state: &[u8]) -> Option<(usize, [f64; PARAMETER_COUNT])> {
    read_values(state, 0)
}

/// A version 3 state: version 2, then the selected saved program's id as a
/// length byte and that many bytes.
#[allow(clippy::type_complexity)]
fn read_state_with_custom(state: &[u8]) -> Option<(usize, [f64; PARAMETER_COUNT], Option<String>)> {
    if state.len() < STATE_HEADER + 2 {
        return None;
    }
    let count = u32::from_le_bytes(state[12..16].try_into().ok()?) as usize;
    let start = STATE_HEADER + count.min(PARAMETER_COUNT) * 8;
    let id_length = usize::from(*state.get(start)?);
    if id_length == 0 {
        return None;
    }
    let (program, values) = read_values(state, 1 + id_length)?;
    let id = core::str::from_utf8(&state[start + 1..]).ok()?;
    if !id
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b".-_".contains(&byte))
    {
        return None;
    }
    Some((program, values, Some(id.to_owned())))
}

/// The 0.1.0 state: a gain and a program, and nothing else.
fn read_first_state(state: &[u8]) -> Option<(usize, [f64; PARAMETER_COUNT])> {
    if state.len() != STATE_FIRST_BYTES {
        return None;
    }
    let gain = f64::from_le_bytes(state[8..16].try_into().ok()?);
    let program = u32::from_le_bytes(state[16..20].try_into().ok()?) as usize;
    if program >= MAX_VOICES || !parameters::is_valid(parameters::GAIN, gain) {
        return None;
    }
    let mut values = parameters::defaults();
    values[parameters::GAIN as usize] = gain;
    Some((program, values))
}

export_parallel_processor!(
    Rf7Processor,
    max_units = 16,
    dispatch_stride = 1024,
    shared_capacity = 49_176,
    max_frames = 4096,
    max_input_channels = 0,
    max_output_channels = 2,
    max_midi_events = 256,
    max_parameter_events = 256,
    max_transfer_bytes = 262_144,
    midi2 = {
        max_events = 256,
        families = MIDI_FAMILY_NOTE
            | MIDI_FAMILY_CONTROL
            | MIDI_FAMILY_PROGRAM
            | MIDI_FAMILY_BEND
            | MIDI_FAMILY_PRESSURE
    }
);

// The export's numbers are the engine's, restated where the macro can read them.
const _: () = assert!(
    DISPATCH_STRIDE == 1024
        && SHARED_CAPACITY == 49_176
        && MAX_BLOCK_FRAMES == 4096
        && POLYPHONY == 16
);
