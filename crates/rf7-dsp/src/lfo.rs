//! The one low-frequency oscillator, shared by every note.
//!
//! The DX7 has a single LFO for the whole instrument, which is why a chord
//! played with vibrato moves as one thing rather than as six independent
//! wobbles. RF-7 keeps that: the LFO belongs to the engine, not to the voice.

use crate::tables::{lfo_delay_seconds, lfo_fade_seconds, lfo_hertz};
use rf7_voice::{Lfo as LfoParameters, LfoWaveform};

#[derive(Clone, Copy, Debug)]
pub struct Lfo {
    phase: f32,
    increment: f32,
    waveform: LfoWaveform,
    delay: f32,
    fade: f32,
    elapsed: f32,
    period: f32,
    sync: bool,
    held: f32,
    random: u32,
}

impl Lfo {
    pub fn new(parameters: &LfoParameters, sample_rate: f32) -> Self {
        let mut lfo = Self {
            phase: 0.0,
            increment: 0.0,
            waveform: parameters.shape(),
            delay: 0.0,
            fade: 0.0,
            elapsed: 0.0,
            period: 1.0 / sample_rate,
            sync: parameters.sync,
            held: 0.0,
            random: 0x2545_f491,
        };
        lfo.retune(parameters, sample_rate, 1.0, 0.0);
        lfo.held = lfo.next_random();
        lfo
    }

    /// Take the speed and the delay again, with the performance layer applied:
    /// the rate as a factor on the program's own speed, the delay as seconds
    /// added to it. The phase and whatever has already elapsed are kept, so a
    /// knob turned mid-phrase bends the modulation instead of restarting it.
    pub fn retune(
        &mut self,
        parameters: &LfoParameters,
        sample_rate: f32,
        rate_scale: f32,
        added_delay: f32,
    ) {
        let rate_scale = if rate_scale.is_finite() {
            rate_scale.max(0.0)
        } else {
            1.0
        };
        let added_delay = if added_delay.is_finite() {
            added_delay.max(0.0)
        } else {
            0.0
        };
        self.increment = lfo_hertz(parameters.speed) * rate_scale / sample_rate;
        self.delay = lfo_delay_seconds(parameters.delay) + added_delay;
        // The program's fade is the firmware's, about as long as its delay;
        // seconds added to the delay fade over their own length too.
        self.fade = lfo_fade_seconds(parameters.delay).max(added_delay);
    }

    /// Called when a key starts a phrase. A synced LFO restarts its cycle and
    /// its delay; an unsynced one keeps running, which is the whole difference.
    pub fn key_down(&mut self) {
        if self.sync {
            self.phase = 0.0;
            self.elapsed = 0.0;
            self.held = self.next_random();
        }
    }

    /// Advance one sample and return the modulation, in -1.0..=1.0, already
    /// scaled by however much of the delayed fade-in has arrived.
    pub fn advance(&mut self) -> f32 {
        self.phase += self.increment;
        if self.phase >= 1.0 {
            self.phase -= self.phase.floor();
            self.held = self.next_random();
        }
        self.elapsed += self.period;
        let amount = if self.fade > 0.0 {
            ((self.elapsed - self.delay) / self.fade).clamp(0.0, 1.0)
        } else {
            1.0
        };
        self.shape() * amount
    }

    /// Each wave from the phase word the way `LFO_GET_AMPLITUDE` reads it:
    /// the triangle starts at its bottom and peaks halfway, the saws start
    /// at their centre and wrap halfway, the square is high first, and the
    /// sine starts at zero.
    fn shape(&self) -> f32 {
        match self.waveform {
            LfoWaveform::Triangle => {
                if self.phase < 0.5 {
                    4.0 * self.phase - 1.0
                } else {
                    3.0 - 4.0 * self.phase
                }
            }
            LfoWaveform::SawUp => {
                if self.phase < 0.5 {
                    2.0 * self.phase
                } else {
                    2.0 * self.phase - 2.0
                }
            }
            LfoWaveform::SawDown => {
                if self.phase < 0.5 {
                    -2.0 * self.phase
                } else {
                    2.0 - 2.0 * self.phase
                }
            }
            LfoWaveform::Square => {
                if self.phase < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            LfoWaveform::Sine => (core::f32::consts::TAU * self.phase).sin(),
            LfoWaveform::SampleAndHold => self.held,
        }
    }

    fn next_random(&mut self) -> f32 {
        // xorshift: deterministic, so a render is reproducible, and cheap
        // enough to sit in the audio path.
        self.random ^= self.random << 13;
        self.random ^= self.random >> 17;
        self.random ^= self.random << 5;
        f32::from_bits((self.random >> 9) | 0x3f80_0000) * 2.0 - 3.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parameters(waveform: u8, speed: u8, delay: u8, sync: bool) -> LfoParameters {
        LfoParameters {
            speed,
            delay,
            pitch_mod_depth: 99,
            amp_mod_depth: 0,
            sync,
            waveform,
        }
    }

    fn extremes(lfo: &mut Lfo, samples: usize) -> (f32, f32) {
        let mut low = f32::MAX;
        let mut high = f32::MIN;
        for _ in 0..samples {
            let value = lfo.advance();
            low = low.min(value);
            high = high.max(value);
        }
        (low, high)
    }

    #[test]
    fn every_waveform_stays_inside_the_bipolar_range() {
        for waveform in 0..=5 {
            let mut lfo = Lfo::new(&parameters(waveform, 80, 0, true), 48_000.0);
            let (low, high) = extremes(&mut lfo, 48_000);
            assert!(low >= -1.0 && high <= 1.0, "waveform {waveform} escaped");
            assert!(high - low > 1.0, "waveform {waveform} barely moves");
        }
    }

    #[test]
    fn the_delay_holds_the_modulation_at_nothing_first() {
        let mut lfo = Lfo::new(&parameters(4, 80, 99, true), 48_000.0);
        let (low, high) = extremes(&mut lfo, 48_000);
        assert_eq!((low, high), (0.0, 0.0), "a four second delay is not over");
        let (low, high) = extremes(&mut lfo, 480_000);
        assert!(low < -0.9 && high > 0.9, "the delay never let it through");
    }

    #[test]
    fn a_synced_oscillator_restarts_and_an_unsynced_one_does_not() {
        let mut synced = Lfo::new(&parameters(0, 60, 0, true), 48_000.0);
        let mut free = Lfo::new(&parameters(0, 60, 0, false), 48_000.0);
        extremes(&mut synced, 1_000);
        extremes(&mut free, 1_000);
        let before = free.phase;
        synced.key_down();
        free.key_down();
        assert_eq!(synced.phase, 0.0, "a synced oscillator restarts its cycle");
        assert_eq!(synced.elapsed, 0.0, "and its delay");
        assert_eq!(free.phase, before, "a free oscillator runs on");
    }

    #[test]
    fn sample_and_hold_is_deterministic_and_changes() {
        let mut first = Lfo::new(&parameters(5, 90, 0, true), 48_000.0);
        let mut second = Lfo::new(&parameters(5, 90, 0, true), 48_000.0);
        let mut seen = 0;
        let mut previous = f32::NAN;
        for _ in 0..48_000 {
            let value = first.advance();
            assert_eq!(value, second.advance());
            if value != previous {
                seen += 1;
                previous = value;
            }
        }
        assert!(seen > 10, "sample and hold never picked a new value");
    }
}
