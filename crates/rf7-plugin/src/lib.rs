//! RackForge adapter. Device access and persistence remain host responsibilities.
//!
//! The one thing this adapter does beyond wiring is the program catalog: the
//! thirty-two programs RackForge offers are the thirty-two voices of whatever
//! cartridge the user installed, so the catalog is written at run time rather
//! than shipped in the package.

mod catalog;

use rackforge_plugin_sdk::{
    MIDI_FAMILY_BEND, MIDI_FAMILY_CONTROL, MIDI_FAMILY_NOTE, MIDI_FAMILY_PROGRAM,
    MIDI2_FLAG_ORIGIN_7BIT, MIDI2_KIND_CONTROL_CHANGE, MIDI2_KIND_NOTE_OFF, MIDI2_KIND_NOTE_ON,
    MIDI2_KIND_PITCH_BEND, MIDI2_KIND_PROGRAM_CHANGE, MidiEvent, MidiEvent2, ParameterEvent,
    Processor, export_processor,
};
use rf7_dsp::{Engine, GAIN_MAX};
use rf7_voice::{
    BULK_DUMP_LENGTH, Cartridge, decode_bulk_dump, decode_voice_dump, factory_cartridge,
};

pub const MAX_FRAMES: u32 = 4096;
pub const MAX_EVENTS: usize = 256;
pub const TRANSFER_BYTES: usize = 16_384;
pub const STATE_VERSION: u32 = 1;
pub const PARAMETER_GAIN: u32 = 0;
pub const RESOURCE_CARTRIDGE: &str = "cartridge";

const DEFAULT_GAIN: f64 = 0.2;

pub struct Rf7Processor {
    engine: Option<Box<Engine>>,
    cartridge: Cartridge,
    program: usize,
    gain: f64,
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
            cartridge: factory_cartridge(),
            program: 0,
            gain: DEFAULT_GAIN,
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
        catalog::write(&self.cartridge, destination)
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
            _ => {}
        }
    }

    fn adopt(&mut self, cartridge: Cartridge) {
        self.cartridge = cartridge;
        if let Some(engine) = &mut self.engine {
            engine.load_cartridge(self.cartridge);
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
        engine.load_cartridge(self.cartridge);
        engine.select_program(self.program);
        engine.set_gain(self.gain);
        self.engine = Some(Box::new(engine));
        self.maximum_frames = frames;
        self.channels = outputs;
        true
    }

    fn set_parameter(&mut self, index: u32, value: f64) -> bool {
        if index != PARAMETER_GAIN || !value.is_finite() || !(0.0..=GAIN_MAX).contains(&value) {
            return false;
        }
        self.gain = value;
        if let Some(engine) = &mut self.engine {
            engine.set_gain(value);
        }
        true
    }

    fn get_parameter(&self, index: u32) -> Option<f64> {
        (index == PARAMETER_GAIN).then_some(self.gain)
    }

    fn reset(&mut self) {
        if let Some(engine) = &mut self.engine {
            engine.reset();
        }
    }

    fn begin_resource(&mut self, id: &str, total_bytes: u64) -> bool {
        if id != RESOURCE_CARTRIDGE || total_bytes > BULK_DUMP_LENGTH as u64 {
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
            || self.incoming.len() + bytes.len() > BULK_DUMP_LENGTH
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
        // A single-voice dump is accepted too, and fills every slot, so a
        // patch a user exported on its own is playable without a cartridge.
        let cartridge = match decode_bulk_dump(&self.incoming) {
            Ok(cartridge) => cartridge,
            Err(_) => match decode_voice_dump(&self.incoming) {
                Ok(decoded) => Cartridge::from_voices([decoded.voice; 32]),
                Err(_) => return false,
            },
        };
        self.incoming = Vec::new();
        self.adopt(cartridge);
        true
    }

    fn write_program_catalog(&mut self, destination: &mut [u8]) -> Option<usize> {
        catalog::write(&self.cartridge, destination)
    }

    fn load_preset(&mut self, id: &str) -> bool {
        let Some(program) = catalog::program_index(id) else {
            return false;
        };
        self.program = program;
        match &mut self.engine {
            Some(engine) => engine.select_program(program),
            None => true,
        }
    }

    fn save_state(&self, destination: &mut [u8]) -> Option<usize> {
        let bytes = destination.get_mut(..20)?;
        bytes[..4].copy_from_slice(b"RF7A");
        bytes[4..8].copy_from_slice(&STATE_VERSION.to_le_bytes());
        bytes[8..16].copy_from_slice(&self.gain.to_le_bytes());
        bytes[16..20].copy_from_slice(&(self.program as u32).to_le_bytes());
        Some(20)
    }

    fn load_state(&mut self, state: &[u8]) -> bool {
        if state.len() != 20 || &state[..4] != b"RF7A" {
            return false;
        }
        let version = u32::from_le_bytes(state[4..8].try_into().expect("validated state length"));
        if version != STATE_VERSION {
            return false;
        }
        let gain = f64::from_le_bytes(state[8..16].try_into().expect("validated state length"));
        let program =
            u32::from_le_bytes(state[16..20].try_into().expect("validated state length")) as usize;
        // Both halves are checked before either is applied, so a rejected
        // state leaves the instrument exactly as it was.
        if !gain.is_finite() || !(0.0..=GAIN_MAX).contains(&gain) || program >= 32 {
            return false;
        }
        self.gain = gain;
        self.program = program;
        if let Some(engine) = &mut self.engine {
            engine.set_gain(gain);
            engine.select_program(program);
        }
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
            || parameters.iter().any(|event| {
                event.index != PARAMETER_GAIN
                    || !event.value.is_finite()
                    || !(0.0..=GAIN_MAX).contains(&event.value)
            })
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
        0xc0 => event.length == 2,
        _ => true,
    }
}

export_processor!(Rf7Processor,
    max_frames = 4096, max_input_channels = 0, max_output_channels = 2,
    max_midi_events = 256, max_parameter_events = 256, max_transfer_bytes = 16_384,
    midi2 = {
        max_events = 256,
        families = MIDI_FAMILY_NOTE | MIDI_FAMILY_CONTROL | MIDI_FAMILY_PROGRAM | MIDI_FAMILY_BEND
    }
);
