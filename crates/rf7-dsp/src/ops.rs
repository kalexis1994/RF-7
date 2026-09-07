//! The operator kernel, as the OPS chip computes it.
//!
//! The DX7's operator chip (the YM21280, Shirriff's die analysis) never
//! multiplies a sine by a gain. Its sine is stored as a logarithm — a
//! quarter wave of 1024 entries holding `round(−log2(sin ω) × 1024)`, a
//! 14-bit attenuation in 1/1024 octave — the envelope's own attenuation is
//! *added* to it, and the sum goes through an exponential built from a
//! 1024-entry table of 12-bit mantissas and a shifter driven by the integer
//! part. Every stage is a small integer, and what that costs — a phase of
//! 4096 steps, a mantissa of twelve bits that loses one bit for every six
//! decibels of attenuation, a floor sixteen octaves down — is part of the
//! instrument's sound: the grain of a quiet operator, the edge on the tail
//! of a note.
//!
//! The kernel keeps the interfaces the engine already speaks — a phase in
//! cycles, a level in [`crate::tables::LEVEL_UNITS_PER_OCTAVE`] units, an
//! output around unity — and does the chip's arithmetic in between. The
//! tables are built by the engine rather than held in a static, so a
//! WebAssembly build needs no lazy initialisation and no atomics.

use crate::tables::{LEVEL_FULL, LEVEL_HEADROOM, LEVEL_UNITS_PER_OCTAVE};

/// Entries in the quarter-wave log-sine table: the ten low bits of the
/// twelve-bit phase.
const QUARTER: usize = 1024;
/// Steps in one cycle of the sine index, the chip's phase resolution.
const CYCLE: usize = 4 * QUARTER;
/// Log-domain units per octave of attenuation, the sine table's own scale.
const LOG_UNITS_PER_OCTAVE: u32 = 1024;
/// Where the fourteen-bit log domain ends: sixteen octaves down, the
/// exponential's shifter has nothing left and the operator is silent.
const LOG_FLOOR: u32 = 16 * LOG_UNITS_PER_OCTAVE;
/// Entries in the exponential table: the fraction bits of the log value.
const EXP_ENTRIES: usize = 1024;
/// The mantissa's scale — twelve bits with the leading one at `2^0`.
const MANTISSA_ONE: u32 = 4096;
/// The signal exponential's output at attenuation 0: fourteen bits.
const OUTPUT_FULL: f32 = 16_384.0;
/// The level, in units, that the hardware's attenuation counts from: the
/// firmware's reference level sits fifteen sixteenths of an octave under it
/// (see [`LEVEL_FULL`]), and that reference is unity in the engine.
const HARDWARE_TOP: f32 = LEVEL_FULL + 15.0 * LEVEL_UNITS_PER_OCTAVE / 16.0;
/// The highest level the engine lets an operator reach, in units.
const LEVEL_CEILING: f32 = LEVEL_FULL + LEVEL_HEADROOM;
/// Log-domain units per level unit: the envelope counts 256 to the octave,
/// the sine table 1024.
const LOG_UNITS_PER_LEVEL_UNIT: u32 = LOG_UNITS_PER_OCTAVE / LEVEL_UNITS_PER_OCTAVE as u32;
/// What the chip's full-scale output is in the engine's scale: `2^(15/16)`,
/// since the reference level the engine calls unity is that far under it.
const FULL_SCALE_GAIN: f32 = 1.914_213_5;

#[derive(Clone, Debug)]
pub struct Ops {
    /// `round(−log2(sin((n + ½) / 1024 × π/2)) × 1024)` for the quarter wave.
    log_sine: [u16; QUARTER],
    /// `round(2^(−f / 1024) × 4096)` for the fraction of the log value.
    exp: [u16; EXP_ENTRIES],
    /// Engine output per unit of the fourteen-bit magnitude.
    scale: f32,
}

impl Default for Ops {
    fn default() -> Self {
        Self::new()
    }
}

impl Ops {
    pub fn new() -> Self {
        let mut log_sine = [0; QUARTER];
        for (index, entry) in log_sine.iter_mut().enumerate() {
            // The half step keeps the table off sin 0, which has no logarithm;
            // the chip's quarter wave is read the same way.
            let angle = (index as f64 + 0.5) / QUARTER as f64 * core::f64::consts::FRAC_PI_2;
            *entry = (-angle.sin().log2() * f64::from(LOG_UNITS_PER_OCTAVE)).round() as u16;
        }
        let mut exp = [0; EXP_ENTRIES];
        for (index, entry) in exp.iter_mut().enumerate() {
            let fraction = index as f64 / EXP_ENTRIES as f64;
            *entry = ((-fraction).exp2() * f64::from(MANTISSA_ONE)).round() as u16;
        }
        Self {
            log_sine,
            exp,
            scale: FULL_SCALE_GAIN / OUTPUT_FULL,
        }
    }

    /// The operator's output for a phase in cycles at a level in units.
    ///
    /// Any real phase is accepted: modulation routinely pushes it several
    /// cycles either way, and the chip's twelve-bit index wraps the same
    /// way. A level at or under the sixteen-octave floor is exactly zero.
    pub fn sample(&self, cycles: f32, level: f32) -> f32 {
        if !cycles.is_finite() || !level.is_finite() {
            return 0.0;
        }
        // The envelope's attenuation, in its own 1/256-octave steps, from
        // the hardware's top; the engine's headroom clamp is the same one
        // `level_gain` applies.
        let attenuation = (HARDWARE_TOP + 0.5 - level.min(LEVEL_CEILING)) as i32;
        if attenuation >= (LOG_FLOOR / LOG_UNITS_PER_LEVEL_UNIT) as i32 {
            return 0.0;
        }
        let attenuation = attenuation.max(0) as u32 * LOG_UNITS_PER_LEVEL_UNIT;
        // The cast saturates, so a phase far outside one cycle is wrapped by
        // the mask rather than by a floor of its own.
        let index = (cycles * CYCLE as f32).floor() as i32 as usize & (CYCLE - 1);
        let quadrant = index / QUARTER;
        let position = if quadrant & 1 == 0 {
            index & (QUARTER - 1)
        } else {
            QUARTER - 1 - (index & (QUARTER - 1))
        };
        let log = u32::from(self.log_sine[position]) + attenuation;
        if log >= LOG_FLOOR {
            return 0.0;
        }
        let mantissa = u32::from(self.exp[(log & (EXP_ENTRIES as u32 - 1)) as usize]);
        let magnitude = (mantissa << 2) >> (log / LOG_UNITS_PER_OCTAVE);
        let output = magnitude as f32 * self.scale;
        if quadrant >= 2 { -output } else { output }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tables::level_gain;
    use core::f32::consts::TAU;

    #[test]
    fn the_log_sine_table_is_the_chips() {
        let ops = Ops::new();
        // The last entry is the top of the quarter wave, a hair under 1.0.
        assert_eq!(ops.log_sine[QUARTER - 1], 0);
        // The first is the smallest angle the table reaches: about 10.35
        // octaves down, well inside fourteen bits.
        let angle = 0.5 / QUARTER as f64 * core::f64::consts::FRAC_PI_2;
        let expected = (-angle.sin().log2() * 1024.0).round() as u16;
        assert_eq!(ops.log_sine[0], expected);
        assert!(ops.log_sine[0] < 16_384);
        // The exponential's mantissa runs from one down to a hair over half.
        assert_eq!(ops.exp[0], 4096);
        assert_eq!(ops.exp[EXP_ENTRIES - 1], 2049);
    }

    #[test]
    fn at_the_reference_level_the_kernel_is_a_unit_sine() {
        let ops = Ops::new();
        let mut worst = 0.0f32;
        for step in 0..100_000 {
            let cycles = step as f32 / 100_000.0;
            let error = (ops.sample(cycles, LEVEL_FULL) - (TAU * cycles).sin()).abs();
            worst = worst.max(error);
        }
        // The twelve-bit phase alone moves a full-scale sine by up to
        // 2π/4096 ≈ 1.5e-3; the mantissa adds a part in four thousand.
        assert!(worst < 2.5e-3, "kernel error {worst} is too large");
    }

    #[test]
    fn the_level_scale_is_level_gain_to_the_mantissas_precision() {
        let ops = Ops::new();
        // Each octave of attenuation costs the mantissa a bit, so the
        // tolerance widens with the distance from the top: a part in a
        // thousand at the reference, a part in eighty eight octaves down.
        for level in [4240.0, 4064.0, 3808.0, 3072.0, 2048.0, 1024.0] {
            let peak = ops.sample(0.25, level);
            let expected = level_gain(level);
            let relative = ((peak - expected) / expected).abs();
            let octaves_down = (HARDWARE_TOP - level) / LEVEL_UNITS_PER_OCTAVE;
            let tolerance = 1e-3 + octaves_down.exp2() / 2048.0;
            assert!(
                relative < tolerance,
                "level {level}: {peak} against {expected}, off by {relative}"
            );
        }
    }

    #[test]
    fn a_quiet_operator_is_coarser_than_a_loud_one() {
        let ops = Ops::new();
        let distinct = |level: f32| {
            let mut values: Vec<u32> = (0..CYCLE)
                .map(|index| {
                    (ops.sample(index as f32 / CYCLE as f32, level).abs() / ops.scale).round()
                        as u32
                })
                .collect();
            values.sort_unstable();
            values.dedup();
            values.len()
        };
        // Ten octaves down the shifter leaves four bits of mantissa: the
        // sixteen magnitudes those bits can hold, and zero.
        let quiet = distinct(HARDWARE_TOP - 10.0 * LEVEL_UNITS_PER_OCTAVE);
        assert!(quiet <= 17, "{quiet} distinct magnitudes ten octaves down");
        assert!(distinct(HARDWARE_TOP) > 256);
    }

    #[test]
    fn sixteen_octaves_down_is_exactly_silence() {
        let ops = Ops::new();
        let floor = HARDWARE_TOP - 16.0 * LEVEL_UNITS_PER_OCTAVE;
        assert_eq!(ops.sample(0.25, floor), 0.0);
        assert_eq!(ops.sample(0.25, floor - 5000.0), 0.0);
        assert!(ops.sample(0.25, floor + 2.0 * LEVEL_UNITS_PER_OCTAVE) > 0.0);
    }

    #[test]
    fn phases_outside_one_cycle_wrap_the_same_way() {
        let ops = Ops::new();
        for step in 0..1_000 {
            let cycles = step as f32 / 1_000.0;
            for turns in [-8.0, -1.0, 0.0, 1.0, 7.0] {
                let shifted = ops.sample(cycles + turns, LEVEL_FULL);
                assert!((shifted - ops.sample(cycles, LEVEL_FULL)).abs() < 2e-3);
            }
        }
    }

    #[test]
    fn a_broken_phase_is_silence_rather_than_a_panic() {
        let ops = Ops::new();
        assert_eq!(ops.sample(f32::NAN, LEVEL_FULL), 0.0);
        assert_eq!(ops.sample(f32::INFINITY, LEVEL_FULL), 0.0);
        assert_eq!(ops.sample(0.25, f32::NAN), 0.0);
    }

    #[test]
    fn the_quarter_points_are_where_they_should_be() {
        let ops = Ops::new();
        assert!(ops.sample(0.0, LEVEL_FULL).abs() < 2e-3);
        assert!((ops.sample(0.25, LEVEL_FULL) - 1.0).abs() < 2e-3);
        assert!(ops.sample(0.5, LEVEL_FULL).abs() < 2e-3);
        assert!((ops.sample(0.75, LEVEL_FULL) + 1.0).abs() < 2e-3);
    }
}
