//! RackForge adapter. Device access and persistence remain host responsibilities.
//!
//! The one thing this adapter does beyond wiring is the program catalog: the
//! thirty-two programs RackForge offers are the thirty-two voices of whatever
//! cartridge the user installed, so the catalog is written at run time rather
//! than shipped in the package.

mod catalog;
pub mod parameters;

use rackforge_plugin_sdk::{
    MIDI_FAMILY_BEND, MIDI_FAMILY_CONTROL, MIDI_FAMILY_NOTE, MIDI_FAMILY_PRESSURE,
    MIDI_FAMILY_PROGRAM, MIDI2_FLAG_ORIGIN_7BIT, MIDI2_KIND_CHANNEL_PRESSURE,
    MIDI2_KIND_CONTROL_CHANGE, MIDI2_KIND_NOTE_OFF, MIDI2_KIND_NOTE_ON, MIDI2_KIND_PITCH_BEND,
    MIDI2_KIND_PROGRAM_CHANGE, MidiEvent, MidiEvent2, ParameterEvent, Processor, export_processor,
};
use rf7_dsp::Engine;
use rf7_voice::{Library, MAX_VOICES, decode_library, factory_library};

pub const MAX_FRAMES: u32 = 4096;
pub const MAX_EVENTS: usize = 256;
pub const TRANSFER_BYTES: usize = 32_768;
/// The largest library file RF-7 will take from the host. Four cartridges of
/// bulk dumps is already past [`MAX_VOICES`]; the rest is slack so an oversized
/// collection is read and capped rather than refused for its size alone.
pub const MAX_RESOURCE_BYTES: usize = 65_536;
/// Version 2 carries every parameter. Version 1, which carried only the gain
/// and the program, is still accepted so a session saved by 0.1.0 opens.
pub const STATE_VERSION: u32 = 2;
const STATE_VERSION_FIRST: u32 = 1;
const STATE_HEADER: usize = 16;
const STATE_FIRST_BYTES: usize = 20;
pub const PARAMETER_GAIN: u32 = parameters::GAIN;
pub const PARAMETER_COUNT: usize = parameters::COUNT;
pub const RESOURCE_CARTRIDGE: &str = "cartridge";

pub struct Rf7Processor {
    engine: Option<Box<Engine>>,
    library: Library,
    program: usize,
    /// One entry per declared parameter, always in its declared range.
    values: [f64; PARAMETER_COUNT],
    /// A cartridge arriving in pieces on the control thread.
    incoming: Vec<u8>,
    receiving: bool,
    maximum_frames: u32,
    channels: u32,
}

impl Default for Rf7Processor {
    fn default() -> Self {
        Self {
            engine: None,
            library: factory_library(),
            program: 0,
            values: parameters::defaults(),
            incoming: Vec::new(),
            receiving: false,
            maximum_frames: 0,
            channels: 0,
        }
    }
}

impl Rf7Processor {
    /// The programs RackForge should offer, as Preset Catalog JSON.
    pub fn catalog(&self, destination: &mut [u8]) -> Option<usize> {
        catalog::write(&self.library, destination)
    }

    /// How many programs this instance currently offers.
    pub fn program_count(&self) -> usize {
        self.library.len()
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
                if engine.select_program(usize::from(index)) {
                    self.program = usize::from(index);
                }
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
                let program = usize::from(event.index);
                if engine.select_program(program) {
                    self.program = program;
                }
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

    fn adopt(&mut self, library: Library) {
        self.library = library;
        self.program = self.program.min(self.library.len().saturating_sub(1));
        if let Some(engine) = &mut self.engine {
            engine.load_library(self.library.clone());
            engine.select_program(self.program);
        }
    }
}

impl Processor for Rf7Processor {
    fn prepare(&mut self, rate: f64, frames: u32, inputs: u32, outputs: u32) -> bool {
        if frames == 0 || frames > MAX_FRAMES || inputs != 0 || !(1..=2).contains(&outputs) {
            return false;
        }
        let Ok(mut engine) = Engine::new(rate as f32) else {
            return false;
        };
        engine.load_library(self.library.clone());
        engine.select_program(self.program);
        self.engine = Some(Box::new(engine));
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
        if id != RESOURCE_CARTRIDGE || total_bytes > MAX_RESOURCE_BYTES as u64 {
            return false;
        }
        self.incoming.clear();
        self.incoming.reserve(total_bytes as usize);
        self.receiving = true;
        true
    }

    fn write_resource(&mut self, offset: u64, bytes: &[u8]) -> bool {
        // Only sequential delivery is accepted: a gap would leave the buffer
        // holding whatever happened to be at that offset before.
        if !self.receiving
            || offset != self.incoming.len() as u64
            || self.incoming.len() + bytes.len() > MAX_RESOURCE_BYTES
        {
            self.receiving = false;
            return false;
        }
        self.incoming.extend_from_slice(bytes);
        true
    }

    fn end_resource(&mut self) -> bool {
        if !self.receiving {
            return false;
        }
        self.receiving = false;
        // Bulk dumps, a single voice, or a headerless chip image: whichever
        // shape the user installed, the programs come out the same way.
        let Ok(library) = decode_library(&self.incoming) else {
            self.incoming = Vec::new();
            return false;
        };
        self.incoming = Vec::new();
        self.adopt(library);
        true
    }

    fn write_program_catalog(&mut self, destination: &mut [u8]) -> Option<usize> {
        catalog::write(&self.library, destination)
    }

    fn load_preset(&mut self, id: &str) -> bool {
        let Some(program) = catalog::program_index(id).filter(|slot| *slot < self.library.len())
        else {
            return false;
        };
        self.program = program;
        match &mut self.engine {
            Some(engine) => engine.select_program(program),
            None => true,
        }
    }

    fn save_state(&self, destination: &mut [u8]) -> Option<usize> {
        let length = STATE_HEADER + PARAMETER_COUNT * 8;
        let bytes = destination.get_mut(..length)?;
        bytes[..4].copy_from_slice(b"RF7A");
        bytes[4..8].copy_from_slice(&STATE_VERSION.to_le_bytes());
        bytes[8..12].copy_from_slice(&(self.program as u32).to_le_bytes());
        bytes[12..16].copy_from_slice(&(PARAMETER_COUNT as u32).to_le_bytes());
        for (index, value) in self.values.iter().enumerate() {
            bytes[STATE_HEADER + index * 8..][..8].copy_from_slice(&value.to_le_bytes());
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
        let (program, values) = match version {
            STATE_VERSION_FIRST => match read_first_state(state) {
                Some(pair) => pair,
                None => return false,
            },
            STATE_VERSION => match read_state(state) {
                Some(pair) => pair,
                None => return false,
            },
            _ => return false,
        };
        // A state can name a program a shorter library does not have; the
        // level and the controls still restore, and the selection falls back.
        self.program = program.min(self.library.len().saturating_sub(1));
        self.values = values;
        if let Some(engine) = &mut self.engine {
            engine.select_program(self.program);
        }
        self.apply();
        true
    }

    fn process(
        &mut self,
        input: &[f32],
        output: &mut [f32],
        midi: &[MidiEvent],
        parameters: &[ParameterEvent],
        frames: u32,
        inputs: u32,
        outputs: u32,
    ) {
        self.process_wide(
            input,
            output,
            midi,
            &[],
            parameters,
            frames,
            inputs,
            outputs,
        );
    }

    fn process_wide(
        &mut self,
        _input: &[f32],
        output: &mut [f32],
        midi: &[MidiEvent],
        midi2: &[MidiEvent2],
        parameters: &[ParameterEvent],
        frames: u32,
        inputs: u32,
        outputs: u32,
    ) {
        output.fill(0.0);
        let samples = (frames as usize).checked_mul(outputs as usize);
        if self.engine.is_none()
            || frames > self.maximum_frames
            || inputs != 0
            || outputs != self.channels
            || samples.is_none_or(|count| count > output.len())
            || !ordered(midi.iter().map(|event| event.frame), frames)
            || !ordered(midi2.iter().map(|event| event.frame), frames)
            || !ordered(parameters.iter().map(|event| event.frame), frames)
            || midi.iter().any(|event| !valid_midi1(event))
            || midi2
                .iter()
                .any(|event| event.channel >= 16 || event.index >= 128)
            || parameters
                .iter()
                .any(|event| !parameters::is_valid(event.index, event.value))
        {
            return;
        }
        let (mut p, mut m, mut w) = (0, 0, 0);
        for frame in 0..frames {
            // Explicit stable tie order: parameters, MIDI 1.0, then MIDI 2.0.
            while p < parameters.len() && parameters[p].frame == frame {
                self.set_parameter(parameters[p].index, parameters[p].value);
                p += 1;
            }
            while m < midi.len() && midi[m].frame == frame {
                self.midi1(&midi[m]);
                m += 1;
            }
            while w < midi2.len() && midi2[w].frame == frame {
                self.midi2(&midi2[w]);
                w += 1;
            }
            let sample = self.engine.as_mut().expect("prepared engine").next_sample();
            let offset = frame as usize * outputs as usize;
            for channel in 0..outputs as usize {
                output[offset + channel] = sample;
            }
        }
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

/// A version 2 state: program, count, then that many values.
///
/// A state written by a future RF-7 with more parameters is refused rather
/// than half-read; one written by an earlier RF-7 with fewer is accepted, and
/// the parameters it never heard of keep their defaults.
fn read_state(state: &[u8]) -> Option<(usize, [f64; PARAMETER_COUNT])> {
    if state.len() < STATE_HEADER {
        return None;
    }
    let program = u32::from_le_bytes(state[8..12].try_into().ok()?) as usize;
    let count = u32::from_le_bytes(state[12..16].try_into().ok()?) as usize;
    if program >= MAX_VOICES || count > PARAMETER_COUNT || state.len() != STATE_HEADER + count * 8 {
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

export_processor!(Rf7Processor,
    max_frames = 4096, max_input_channels = 0, max_output_channels = 2,
    max_midi_events = 256, max_parameter_events = 256, max_transfer_bytes = 32_768,
    midi2 = {
        max_events = 256,
        families = MIDI_FAMILY_NOTE
            | MIDI_FAMILY_CONTROL
            | MIDI_FAMILY_PROGRAM
            | MIDI_FAMILY_BEND
            | MIDI_FAMILY_PRESSURE
    }
);
