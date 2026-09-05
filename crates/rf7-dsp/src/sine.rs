//! The one waveform an FM operator has.
//!
//! A table rather than a call to `sin` per operator per sample: six operators
//! across sixteen voices is ninety-six evaluations a sample, and the engine
//! has to hold that on a Raspberry Pi as well as on a desktop. The table is
//! owned by the engine rather than held in a static, so a WebAssembly build
//! needs no lazy initialisation and no atomics.

use core::f32::consts::TAU;

/// Points per cycle. The interpolation error at this size is below the level
/// quantisation the envelopes already impose.
const POINTS: usize = 4096;

#[derive(Clone, Debug)]
pub struct Sine {
    /// One extra point repeats the start, so interpolation never wraps.
    table: [f32; POINTS + 1],
}

impl Default for Sine {
    fn default() -> Self {
        Self::new()
    }
}

impl Sine {
    pub fn new() -> Self {
        let mut table = [0.0; POINTS + 1];
        for (index, point) in table.iter_mut().enumerate() {
            *point = (TAU * index as f32 / POINTS as f32).sin();
        }
        Self { table }
    }

    /// The sine of a phase given in cycles. Any real input is accepted:
    /// modulation routinely pushes the argument several cycles either way.
    pub fn lookup(&self, cycles: f32) -> f32 {
        if !cycles.is_finite() {
            return 0.0;
        }
        let wrapped = cycles - cycles.floor();
        let position = wrapped * POINTS as f32;
        let index = (position as usize).min(POINTS - 1);
        let fraction = position - index as f32;
        let low = self.table[index];
        low + (self.table[index + 1] - low) * fraction
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_agrees_with_the_real_sine() {
        let sine = Sine::new();
        let mut worst = 0.0f32;
        for step in 0..100_000 {
            let cycles = step as f32 / 100_000.0;
            let error = (sine.lookup(cycles) - (TAU * cycles).sin()).abs();
            worst = worst.max(error);
        }
        assert!(worst < 1e-5, "table error {worst} is too large");
    }

    #[test]
    fn phases_outside_one_cycle_wrap_the_same_way() {
        let sine = Sine::new();
        for step in 0..1_000 {
            let cycles = step as f32 / 1_000.0;
            for turns in [-8.0, -1.0, 0.0, 1.0, 7.0] {
                let shifted = sine.lookup(cycles + turns);
                assert!((shifted - sine.lookup(cycles)).abs() < 1e-4);
            }
        }
    }

    #[test]
    fn a_broken_phase_is_silence_rather_than_a_panic() {
        let sine = Sine::new();
        assert_eq!(sine.lookup(f32::NAN), 0.0);
        assert_eq!(sine.lookup(f32::INFINITY), 0.0);
        assert_eq!(sine.lookup(f32::NEG_INFINITY), 0.0);
    }

    #[test]
    fn the_quarter_points_are_where_they_should_be() {
        let sine = Sine::new();
        assert!(sine.lookup(0.0).abs() < 1e-6);
        assert!((sine.lookup(0.25) - 1.0).abs() < 1e-6);
        assert!(sine.lookup(0.5).abs() < 1e-6);
        assert!((sine.lookup(0.75) + 1.0).abs() < 1e-6);
    }
}
