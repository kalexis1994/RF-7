//! The plugin driven the way a host drives it: the coordinator's pre-stage,
//! one render per planned unit, the post-stage — over buffers this harness
//! owns, so every test has its own instrument and they can run side by
//! side. The SDK's generated `process` runs the same three stages over the
//! component's statics; that composition is the SDK's to prove.

use rackforge_plugin_sdk::{
    BlockContext, MidiEvent, MidiEvent2, ParallelProcessor, ParameterEvent, PlanWriter,
    UnitContext, UnitMix,
};
use rf7_dsp::{DISPATCH_STRIDE, MAX_BLOCK_FRAMES, POLYPHONY, SHARED_CAPACITY, Unit};
use rf7_plugin::Rf7Processor;
use std::ops::{Deref, DerefMut};

const MAX_CHANNELS: usize = 2;
const SLOT_SAMPLES: usize = MAX_BLOCK_FRAMES * MAX_CHANNELS;

pub struct Host {
    processor: Rf7Processor,
    units: Box<[Unit; POLYPHONY]>,
    plan: Vec<u32>,
    dispatch: Vec<u8>,
    shared: Vec<u8>,
    mix: Vec<f32>,
}

impl Default for Host {
    fn default() -> Self {
        Self {
            processor: Rf7Processor::default(),
            units: Box::new(std::array::from_fn(|_| Unit::default())),
            plan: vec![0; POLYPHONY * 2],
            dispatch: vec![0; POLYPHONY * DISPATCH_STRIDE],
            shared: vec![0; SHARED_CAPACITY],
            mix: vec![0.0; POLYPHONY * SLOT_SAMPLES],
        }
    }
}

impl Deref for Host {
    type Target = Rf7Processor;

    fn deref(&self) -> &Rf7Processor {
        &self.processor
    }
}

impl DerefMut for Host {
    fn deref_mut(&mut self) -> &mut Rf7Processor {
        &mut self.processor
    }
}

impl Host {
    /// Stops every voice, as the host does on a reset: the coordinator and
    /// each unit instance.
    pub fn reset(&mut self) {
        ParallelProcessor::reset(&mut self.processor);
        for unit in self.units.iter_mut() {
            Rf7Processor::reset_unit(unit);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn process(
        &mut self,
        input: &[f32],
        output: &mut [f32],
        midi: &[MidiEvent],
        parameters: &[ParameterEvent],
        frames: u32,
        input_channels: u32,
        output_channels: u32,
    ) {
        self.process_wide(
            input,
            output,
            midi,
            &[],
            parameters,
            frames,
            input_channels,
            output_channels,
        );
    }

    /// One block: the pre-stage, every planned unit in ascending order into
    /// its mix slot, the post-stage.
    #[allow(clippy::too_many_arguments)]
    pub fn process_wide(
        &mut self,
        input: &[f32],
        output: &mut [f32],
        midi: &[MidiEvent],
        midi2: &[MidiEvent2],
        parameters: &[ParameterEvent],
        frames: u32,
        input_channels: u32,
        output_channels: u32,
    ) {
        let samples = frames as usize * output_channels as usize;
        let (count, shared_len) = {
            let mut writer = PlanWriter::new(
                &mut self.plan,
                &mut self.dispatch,
                &mut self.shared,
                DISPATCH_STRIDE,
                POLYPHONY,
            );
            let context = BlockContext {
                input,
                midi,
                midi2,
                parameters,
                frames,
                input_channels,
                output_channels,
            };
            self.processor.begin_block(&context, &mut writer);
            (writer.activated(), writer.shared_len())
        };
        for index in 0..count {
            let unit = self.plan[index * 2] as usize;
            let length = self.plan[index * 2 + 1] as usize;
            let payload = &self.dispatch[unit * DISPATCH_STRIDE..][..length];
            let context = UnitContext {
                input,
                shared: &self.shared[..shared_len],
                frames,
                output_channels,
            };
            Rf7Processor::render_unit(
                unit as u32,
                &mut self.units[unit],
                payload,
                &context,
                &mut self.mix[unit * SLOT_SAMPLES..][..samples],
            );
        }
        let mix = UnitMix::new(&self.mix, SLOT_SAMPLES, &self.plan, count, samples);
        self.processor
            .end_block(&mix, output, frames, output_channels);
    }
}
