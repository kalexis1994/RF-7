//! One sounding note: six operators, their envelopes, and the routing.

use crate::{
    OPERATORS,
    algorithm::{ALGORITHMS, Algorithm},
    envelope::Envelope,
    sine::Sine,
    tables::{
        amp_mod_units, fixed_frequency, key_level_offset, level_gain, operator_ratio,
        pitch_eg_semitones, scale_output_level, velocity_offset,
    },
};
use rf7_voice::Voice;

/// Phase deviation, in cycles, produced by a modulator running at unity gain.
///
/// This is the single constant that decides how bright the whole instrument
/// is, and the one most worth measuring against a real DX7. Everything else in
/// an FM patch is a ratio; this is the scale.
pub const MODULATION_CYCLES: f32 = 1.0;

/// Concert pitch, and the MIDI note it sits on.
const REFERENCE_HERTZ: f64 = 440.0;
const REFERENCE_NOTE: f64 = 69.0;
/// A transpose byte of 24 means no transposition.
const TRANSPOSE_CENTRE: i32 = 24;

#[derive(Clone, Copy, Debug)]
pub struct NoteVoice {
    algorithm: &'static Algorithm,
    feedback: f32,
    /// Cycles, always wrapped into 0.0..1.0.
    phases: [f32; OPERATORS],
    /// Cycles per sample at the key's own pitch, before any modulation.
    increments: [f32; OPERATORS],
    /// A fixed operator ignores the key and every pitch modulation with it.
    fixed: [bool; OPERATORS],
    envelopes: [Envelope; OPERATORS],
    /// Level units this operator loses at full amplitude modulation.
    amp_mod: [f32; OPERATORS],
    outputs: [f32; OPERATORS],
    previous: [f32; OPERATORS],
    pitch_envelope: Envelope,
    channel: u8,
    note: u8,
    /// Rising while the key or the pedal holds it.
    held: bool,
    active: bool,
    /// Counts up for as long as this voice is sounding, so the allocator can
    /// tell which note has been going longest.
    age: u64,
}

impl Default for NoteVoice {
    fn default() -> Self {
        Self {
            algorithm: &ALGORITHMS[0],
            feedback: 0.0,
            phases: [0.0; OPERATORS],
            increments: [0.0; OPERATORS],
            fixed: [false; OPERATORS],
            envelopes: [Envelope::default(); OPERATORS],
            amp_mod: [0.0; OPERATORS],
            outputs: [0.0; OPERATORS],
            previous: [0.0; OPERATORS],
            pitch_envelope: Envelope::default(),
            channel: 0,
            note: 0,
            held: false,
            active: false,
            age: 0,
        }
    }
}

impl NoteVoice {
    pub fn start(&mut self, patch: &Voice, channel: u8, note: u8, velocity: u8, sample_rate: f32) {
        let transposed = i32::from(note) + i32::from(patch.transpose.min(48)) - TRANSPOSE_CENTRE;
        let hertz = REFERENCE_HERTZ * (((f64::from(transposed) - REFERENCE_NOTE) / 12.0).exp2());
        self.algorithm = &ALGORITHMS[usize::from(patch.algorithm.min(31))];
        self.feedback = feedback_amount(patch.feedback);
        for (index, operator) in patch.operators.iter().enumerate() {
            // Key scaling shifts the level on the same 0..=127 dial the
            // output level lives on, so it is clamped there before becoming
            // units; velocity is applied afterwards, in units.
            let scaled = (scale_output_level(operator.output_level)
                + key_level_offset(note, operator))
            .clamp(0, 127);
            let ceiling =
                scaled as f32 * 32.0 + velocity_offset(velocity, operator.velocity_sensitivity);
            self.envelopes[index] = Envelope::operator(operator, note, ceiling, sample_rate);
            self.fixed[index] = operator.fixed_frequency;
            let operator_hertz = if operator.fixed_frequency {
                fixed_frequency(operator)
            } else {
                hertz * operator_ratio(operator)
            };
            self.increments[index] = (operator_hertz / f64::from(sample_rate)) as f32;
            self.amp_mod[index] = amp_mod_units(operator.amp_mod_sensitivity);
            // Oscillator sync is what makes a struck patch repeat exactly; a
            // free-running voice keeps whatever phase it had.
            if patch.oscillator_sync {
                self.phases[index] = 0.0;
            }
            self.outputs[index] = 0.0;
            self.previous[index] = 0.0;
        }
        let targets = [
            pitch_eg_semitones(patch.pitch_eg_level[0]),
            pitch_eg_semitones(patch.pitch_eg_level[1]),
            pitch_eg_semitones(patch.pitch_eg_level[2]),
            pitch_eg_semitones(patch.pitch_eg_level[3]),
        ];
        self.pitch_envelope = Envelope::pitch(targets, patch.pitch_eg_rate, sample_rate);
        self.channel = channel;
        self.note = note;
        self.held = true;
        self.active = true;
        self.age = 0;
    }

    pub fn channel(&self) -> u8 {
        self.channel
    }

    pub fn note(&self) -> u8 {
        self.note
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    pub fn is_held(&self) -> bool {
        self.held
    }

    pub fn age(&self) -> u64 {
        self.age
    }

    pub fn release(&mut self) {
        if !self.held {
            return;
        }
        self.held = false;
        for envelope in &mut self.envelopes {
            envelope.release();
        }
        self.pitch_envelope.release();
    }

    /// Stop without a release segment. Used only when a voice is taken.
    pub fn silence(&mut self) {
        *self = Self::default();
    }

    /// One sample. `pitch` is the whole voice's detuning in semitones, and
    /// `amplitude` is how much of the amplitude modulation is currently in,
    /// from 0.0 to 1.0.
    pub fn next_sample(&mut self, sine: &Sine, pitch: f32, amplitude: f32) -> f32 {
        if !self.active {
            return 0.0;
        }
        self.age += 1;
        let semitones = pitch + self.pitch_envelope.advance();
        let factor = (semitones / 12.0).exp2();
        let mut sum = 0.0;
        let mut audible = false;
        for index in (0..OPERATORS).rev() {
            let units = self.envelopes[index].advance() - self.amp_mod[index] * amplitude;
            let gain = level_gain(units);
            let mut modulation = 0.0;
            let sources = self.algorithm.modulators[index];
            for source in index + 1..OPERATORS {
                if sources & (1 << source) != 0 {
                    modulation += self.outputs[source];
                }
            }
            let (from, to) = self.algorithm.feedback;
            if usize::from(to) == index && self.feedback > 0.0 {
                let source = usize::from(from);
                // Averaging the last two samples is what keeps a feedback
                // operator from oscillating at the Nyquist frequency.
                let history = (self.previous[source] + self.outputs[source]) * 0.5;
                modulation += history * self.feedback;
            }
            let increment = if self.fixed[index] {
                self.increments[index]
            } else {
                self.increments[index] * factor
            };
            let mut phase = self.phases[index] + increment;
            phase -= phase.floor();
            self.phases[index] = phase;
            let output = sine.lookup(phase + modulation * MODULATION_CYCLES) * gain;
            self.previous[index] = self.outputs[index];
            self.outputs[index] = output;
            if self.algorithm.is_carrier(index) {
                sum += output;
                audible |= !self.envelopes[index].is_silent();
            }
        }
        if !audible {
            self.active = false;
        }
        sum
    }
}

fn feedback_amount(level: u8) -> f32 {
    if level == 0 {
        0.0
    } else {
        (f32::from(level.min(7)) - 7.0).exp2()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rf7_voice::factory_voice;

    fn render(patch: &Voice, note: u8, velocity: u8, samples: usize) -> Vec<f32> {
        let sine = Sine::new();
        let mut voice = NoteVoice::default();
        voice.start(patch, 0, note, velocity, 48_000.0);
        (0..samples)
            .map(|_| voice.next_sample(&sine, 0.0, 0.0))
            .collect()
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0f32, |peak, s| peak.max(s.abs()))
    }

    #[test]
    fn a_struck_note_makes_sound_and_every_sample_is_finite() {
        for index in 0..8 {
            let patch = factory_voice(index);
            let rendered = render(&patch, 60, 100, 24_000);
            assert!(peak(&rendered) > 0.001, "factory voice {index} is silent");
            assert!(
                rendered.iter().all(|s| s.is_finite()),
                "factory voice {index} produced a non-finite sample"
            );
        }
    }

    #[test]
    fn a_released_note_frees_its_voice() {
        let patch = factory_voice(4); // RF MARIMBA, no sustain segment
        let sine = Sine::new();
        let mut voice = NoteVoice::default();
        voice.start(&patch, 0, 60, 100, 48_000.0);
        voice.release();
        for _ in 0..48_000 * 4 {
            voice.next_sample(&sine, 0.0, 0.0);
        }
        assert!(!voice.is_active(), "the voice never finished");
        assert_eq!(voice.next_sample(&sine, 0.0, 0.0), 0.0);
    }

    #[test]
    fn playing_harder_is_louder_when_the_patch_says_so() {
        let patch = factory_voice(0); // RF TINES, velocity sensitive
        let soft = peak(&render(&patch, 60, 20, 12_000));
        let hard = peak(&render(&patch, 60, 127, 12_000));
        assert!(
            hard > soft * 1.5,
            "velocity did almost nothing: {soft} {hard}"
        );
    }

    #[test]
    fn concert_a_is_concert_a() {
        // INIT VOICE is one carrier at ratio 1.0 with no modulation, so its
        // output is a plain sine at the key's own frequency. If this drifts,
        // every other pitch in the instrument is wrong with it.
        // Two seconds, so the one crossing the window boundary can cost at
        // most half a hertz.
        let patch = Voice::init();
        let hertz = zero_crossings(&render(&patch, 69, 100, 96_000)) as f32 / 2.0;
        assert!((hertz - 440.0).abs() < 1.0, "A4 came out at {hertz} Hz");
        let mut transposed = patch;
        transposed.transpose = 12; // An octave below the centred 24.
        let low = zero_crossings(&render(&transposed, 69, 100, 96_000)) as f32 / 2.0;
        assert!(
            (low - 220.0).abs() < 1.0,
            "an octave down came out at {low} Hz"
        );
    }

    #[test]
    fn an_octave_up_doubles_the_period() {
        // Zero crossings are a cheap frequency estimate and need no analysis
        // crate. RF ORGAN is six carriers, so its fundamental is unambiguous.
        let patch = factory_voice(6);
        let low = zero_crossings(&render(&patch, 48, 100, 48_000));
        let high = zero_crossings(&render(&patch, 60, 100, 48_000));
        let ratio = high as f32 / low as f32;
        assert!(
            (ratio - 2.0).abs() < 0.1,
            "an octave gave a ratio of {ratio}"
        );
    }

    fn zero_crossings(samples: &[f32]) -> usize {
        samples
            .windows(2)
            .filter(|pair| pair[0] <= 0.0 && pair[1] > 0.0)
            .count()
    }

    #[test]
    fn silencing_a_voice_leaves_nothing_behind() {
        let sine = Sine::new();
        let mut voice = NoteVoice::default();
        voice.start(&factory_voice(0), 3, 72, 100, 48_000.0);
        for _ in 0..1_000 {
            voice.next_sample(&sine, 0.0, 0.0);
        }
        voice.silence();
        assert!(!voice.is_active());
        assert_eq!(voice.next_sample(&sine, 0.0, 0.0), 0.0);
    }

    #[test]
    fn feedback_scales_by_powers_of_two_from_nothing() {
        assert_eq!(feedback_amount(0), 0.0);
        assert_eq!(feedback_amount(7), 1.0);
        assert_eq!(feedback_amount(6), 0.5);
        assert!(feedback_amount(1) < 0.02);
    }
}
