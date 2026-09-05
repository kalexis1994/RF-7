//! Panel numbers to engine numbers.
//!
//! A DX7 parameter is a small integer with no unit attached; what makes it
//! sound like a DX7 is the curve behind it. Everything that turns one into a
//! frequency, a gain or a slope lives here, one function per mapping, so
//! `docs/MODEL.md` can say for each of them whether it is a documented table
//! or an RF-7 approximation waiting for a measurement.
//!
//! Levels are held in *units*: a logarithmic amplitude scale with
//! [`LEVEL_UNITS_PER_OCTAVE`] units to a factor of two, so the full
//! [`LEVEL_FULL`] range spans a little over 96 dB.

use rf7_voice::{Curve, Operator};

/// Units of the logarithmic level scale per doubling of amplitude.
pub const LEVEL_UNITS_PER_OCTAVE: f32 = 256.0;
/// Unity gain. 127 scaled output-level steps of 32 units each.
pub const LEVEL_FULL: f32 = 4064.0;
/// Below this the operator is inaudible; holding it at zero keeps silence
/// exactly silent instead of leaving a denormal trickling through the mix.
pub const LEVEL_SILENT: f32 = 0.0;

/// The rising part of an envelope slows as it approaches the top. The ceiling
/// sits above [`LEVEL_FULL`] so the last of the attack still moves.
const ATTACK_CEILING: f32 = 4935.0;
/// Level units per second at the slowest quantised rate.
const RATE_BASE_UNITS: f32 = 64.0;
/// Level units lost per step of velocity sensitivity at zero velocity.
const VELOCITY_UNITS_PER_STEP: f32 = 176.0;
/// Where a break point of 0 sits on the MIDI scale.
const BREAK_POINT_ANCHOR: i32 = 17;
/// One detune step, in octaves. About 1.7 cents.
const DETUNE_OCTAVES: f64 = 0.001_417;
/// Pitch envelope excursion at the extreme levels, in semitones.
const PITCH_EG_RANGE: f32 = 48.0;
/// LFO pitch excursion at full depth and the highest sensitivity.
const PITCH_MOD_RANGE: f32 = 12.0;
/// Level units removed by amplitude modulation at full depth.
const AMP_MOD_UNITS: f32 = 1024.0;

/// Output level 0..=99 to the 0..=127 scale the level domain counts in.
///
/// The first twenty steps are a table because the DX7's own low end is not on
/// the straight line the rest of the range follows.
pub fn scale_output_level(level: u8) -> i32 {
    const LOW: [u8; 20] = [
        0, 5, 9, 13, 17, 20, 23, 25, 27, 29, 31, 33, 35, 37, 39, 41, 42, 43, 45, 46,
    ];
    let level = level.min(99);
    if (level as usize) < LOW.len() {
        i32::from(LOW[level as usize])
    } else {
        28 + i32::from(level)
    }
}

/// Keyboard level scaling for one operator at one key, on the 0..=127 scale.
///
/// Positive above the break point when the right curve is a positive one, and
/// so on for the other three combinations. The result is added to the scaled
/// output level before it is clamped.
pub fn key_level_offset(note: u8, operator: &Operator) -> i32 {
    let distance = i32::from(note) - i32::from(operator.break_point) - BREAK_POINT_ANCHOR;
    if distance >= 0 {
        signed_curve((distance + 1) / 3, operator.right_depth, operator.right())
    } else {
        signed_curve(-distance / 3, operator.left_depth, operator.left())
    }
}

fn signed_curve(group: i32, depth: u8, curve: Curve) -> i32 {
    const EXPONENTIAL: [u8; 33] = [
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 11, 14, 16, 19, 23, 27, 33, 39, 47, 56, 66, 80, 94, 110, 126,
        142, 158, 174, 190, 206, 222, 238, 250,
    ];
    let group = group.clamp(0, EXPONENTIAL.len() as i32 - 1);
    let depth = i32::from(depth.min(99));
    let magnitude = if curve.is_exponential() {
        (i32::from(EXPONENTIAL[group as usize]) * depth * 329) >> 15
    } else {
        (group * depth * 329) >> 12
    };
    if curve.is_positive() {
        magnitude
    } else {
        -magnitude
    }
}

/// How much faster this operator's envelope runs at this key.
///
/// Rate scaling is why a bright patch does not smear in the top octave: the
/// higher the key, the shorter every segment.
pub fn key_rate_offset(note: u8, sensitivity: u8) -> i32 {
    let position = (i32::from(note) / 3 - 7).clamp(0, 31);
    (i32::from(sensitivity.min(7)) * position) >> 3
}

/// Envelope rate 0..=99 to the hardware's 0..=63 quantised rate.
///
/// The DX7 has 64 distinct envelope speeds, not 100. Two neighbouring panel
/// rates therefore often produce exactly the same envelope, which is a real
/// property of the instrument rather than a shortcut.
pub fn quantised_rate(rate: u8, key_offset: i32) -> i32 {
    ((i32::from(rate.min(99)) * 41) / 64 + key_offset).clamp(0, 63)
}

/// Level units per second at a quantised rate. Four steps double the speed.
pub fn rate_units_per_second(quantised: i32) -> f32 {
    RATE_BASE_UNITS * ((quantised as f32) / 4.0).exp2()
}

/// The step a rising segment takes, which shortens as it nears the top.
pub fn rising_step(level: f32, units: f32) -> f32 {
    units * ((ATTACK_CEILING - level) / ATTACK_CEILING).max(0.0)
}

/// Level units to linear gain.
pub fn level_gain(units: f32) -> f32 {
    if units <= LEVEL_SILENT {
        return 0.0;
    }
    ((units.min(LEVEL_FULL) - LEVEL_FULL) / LEVEL_UNITS_PER_OCTAVE).exp2()
}

/// Level units removed by a key struck below full force.
///
/// Zero at velocity 127 for every sensitivity: velocity takes level away, it
/// never adds any, which is why a sensitive patch still reaches its programmed
/// output level when played hard.
pub fn velocity_offset(velocity: u8, sensitivity: u8) -> f32 {
    let normalised = f32::from(velocity.min(127)) / 127.0;
    let response = normalised * (2.0 - normalised);
    -(1.0 - response) * f32::from(sensitivity.min(7)) * VELOCITY_UNITS_PER_STEP
}

/// The frequency ratio of a key-tracking operator.
///
/// Coarse 0 is the half ratio, not silence; fine adds hundredths of the coarse
/// ratio; detune is a fixed small offset in the logarithmic domain.
pub fn operator_ratio(operator: &Operator) -> f64 {
    let coarse = if operator.coarse == 0 {
        0.5
    } else {
        f64::from(operator.coarse.min(31))
    };
    let fine = 1.0 + f64::from(operator.fine.min(99)) / 100.0;
    coarse * fine * detune_factor(operator.detune)
}

/// The frequency of a fixed operator, in hertz. Four decades from 1 Hz.
pub fn fixed_frequency(operator: &Operator) -> f64 {
    let decade = f64::from(operator.coarse & 3);
    let fraction = f64::from(operator.fine.min(99)) / 100.0;
    10f64.powf(decade + fraction) * detune_factor(operator.detune)
}

fn detune_factor(detune: u8) -> f64 {
    ((f64::from(detune.min(14)) - 7.0) * DETUNE_OCTAVES).exp2()
}

/// LFO speed 0..=99 to hertz.
pub fn lfo_hertz(speed: u8) -> f32 {
    const SLOWEST: f32 = 0.062;
    const FASTEST: f32 = 47.0;
    let position = f32::from(speed.min(99)) / 99.0;
    SLOWEST * (FASTEST / SLOWEST).powf(position)
}

/// LFO delay 0..=99 to the seconds before modulation is fully in.
pub fn lfo_delay_seconds(delay: u8) -> f32 {
    let position = f32::from(delay.min(99)) / 99.0;
    position * position * 4.0
}

/// Pitch modulation sensitivity 0..=7 to semitones at full depth.
pub fn pitch_mod_semitones(sensitivity: u8) -> f32 {
    const STEPS: [f32; 8] = [0.0, 10.0, 20.0, 33.0, 55.0, 92.0, 153.0, 255.0];
    STEPS[usize::from(sensitivity.min(7))] / 255.0 * PITCH_MOD_RANGE
}

/// Amplitude modulation sensitivity 0..=3 to level units at full depth.
pub fn amp_mod_units(sensitivity: u8) -> f32 {
    const STEPS: [f32; 4] = [0.0, 0.25, 0.5, 1.0];
    STEPS[usize::from(sensitivity.min(3))] * AMP_MOD_UNITS
}

/// Pitch envelope level 0..=99 to semitones, with 50 as no change.
///
/// The curve is gentle either side of centre and steep at the extremes, so the
/// small departures most patches use stay usable on the same 0..=99 dial as a
/// four-octave sweep.
pub fn pitch_eg_semitones(level: u8) -> f32 {
    let position = ((f32::from(level.min(99)) - 50.0) / 49.0).clamp(-1.0, 1.0);
    position * position.abs() * PITCH_EG_RANGE
}

#[cfg(test)]
mod tests {
    use super::*;
    use rf7_voice::Voice;

    #[test]
    fn the_level_scale_spans_ninety_six_decibels() {
        assert_eq!(scale_output_level(0), 0);
        assert_eq!(scale_output_level(99), 127);
        assert_eq!(scale_output_level(200), 127);
        assert_eq!(level_gain(LEVEL_FULL), 1.0);
        assert_eq!(level_gain(0.0), 0.0);
        assert!(level_gain(LEVEL_FULL + 1000.0) <= 1.0);
        let half = level_gain(LEVEL_FULL - LEVEL_UNITS_PER_OCTAVE);
        assert!((half - 0.5).abs() < 1e-6);
    }

    #[test]
    fn velocity_only_ever_takes_level_away() {
        for sensitivity in 0..=7 {
            assert_eq!(velocity_offset(127, sensitivity), 0.0);
            for velocity in 0..=127 {
                assert!(velocity_offset(velocity, sensitivity) <= 0.0);
            }
            assert!(velocity_offset(1, sensitivity) <= velocity_offset(100, sensitivity));
        }
        assert_eq!(velocity_offset(0, 0), 0.0);
        assert!(velocity_offset(0, 7) < velocity_offset(0, 1));
    }

    #[test]
    fn rates_quantise_to_sixty_four_speeds_and_stay_ordered() {
        assert_eq!(quantised_rate(0, 0), 0);
        assert_eq!(quantised_rate(99, 0), 63);
        assert_eq!(quantised_rate(99, 20), 63);
        let mut previous = -1;
        let mut distinct = 0;
        for rate in 0..=99u8 {
            let quantised = quantised_rate(rate, 0);
            assert!(quantised >= previous);
            if quantised != previous {
                distinct += 1;
            }
            previous = quantised;
        }
        assert_eq!(distinct, 64);
        assert!(rate_units_per_second(63) > rate_units_per_second(0) * 50_000.0);
    }

    #[test]
    fn key_scaling_runs_the_right_way_on_each_side() {
        let mut operator = Voice::init().operators[0];
        operator.break_point = 39;
        operator.left_depth = 99;
        operator.right_depth = 99;
        operator.left_curve = 0; // -LIN
        operator.right_curve = 3; // +LIN
        assert_eq!(
            key_level_offset(39 + BREAK_POINT_ANCHOR as u8, &operator),
            0
        );
        assert!(key_level_offset(24, &operator) < 0);
        assert!(key_level_offset(96, &operator) > 0);
        // Swapping both curves mirrors the sign on both sides.
        operator.left_curve = 3;
        operator.right_curve = 0;
        assert!(key_level_offset(24, &operator) > 0);
        assert!(key_level_offset(96, &operator) < 0);
        // Zero depth is flat whatever the curve says.
        operator.left_depth = 0;
        operator.right_depth = 0;
        assert_eq!(key_level_offset(24, &operator), 0);
        assert_eq!(key_level_offset(96, &operator), 0);
    }

    #[test]
    fn rate_scaling_never_slows_an_envelope_down() {
        for sensitivity in 0..=7 {
            assert_eq!(key_rate_offset(0, sensitivity), 0);
            assert!(key_rate_offset(108, sensitivity) >= key_rate_offset(36, sensitivity));
        }
        assert_eq!(key_rate_offset(108, 0), 0);
        assert!(key_rate_offset(108, 7) > 0);
    }

    #[test]
    fn operator_frequencies_follow_the_panel() {
        let mut operator = Voice::init().operators[0];
        operator.detune = 7;
        operator.coarse = 0;
        assert!((operator_ratio(&operator) - 0.5).abs() < 1e-9);
        operator.coarse = 1;
        assert!((operator_ratio(&operator) - 1.0).abs() < 1e-9);
        operator.coarse = 14;
        assert!((operator_ratio(&operator) - 14.0).abs() < 1e-9);
        operator.fine = 50;
        assert!((operator_ratio(&operator) - 21.0).abs() < 1e-9);
        // Detune moves either side of centre and by the same factor.
        operator.fine = 0;
        operator.detune = 8;
        let up = operator_ratio(&operator);
        operator.detune = 6;
        let down = operator_ratio(&operator);
        assert!(up > 14.0 && down < 14.0);
        assert!((up * down - 196.0).abs() < 1e-6);
        // A fixed operator ignores the coarse decades above three.
        operator.detune = 7;
        operator.fixed_frequency = true;
        operator.coarse = 0;
        assert!((fixed_frequency(&operator) - 1.0).abs() < 1e-9);
        operator.coarse = 3;
        assert!((fixed_frequency(&operator) - 1000.0).abs() < 1e-6);
        operator.coarse = 7;
        assert!((fixed_frequency(&operator) - 1000.0).abs() < 1e-6);
    }

    #[test]
    fn the_modulation_dials_are_centred_and_bounded() {
        assert_eq!(pitch_eg_semitones(50), 0.0);
        assert!(pitch_eg_semitones(99) > 40.0);
        assert!(pitch_eg_semitones(0) < -40.0);
        assert!(pitch_eg_semitones(55).abs() < 1.0);
        assert_eq!(pitch_mod_semitones(0), 0.0);
        assert!(pitch_mod_semitones(7) > pitch_mod_semitones(3));
        assert_eq!(amp_mod_units(0), 0.0);
        assert!(amp_mod_units(3) > amp_mod_units(1));
        assert!(lfo_hertz(0) < 0.1 && lfo_hertz(99) > 40.0);
        assert!(lfo_hertz(50) > lfo_hertz(49));
        assert_eq!(lfo_delay_seconds(0), 0.0);
        assert!(lfo_delay_seconds(99) > 3.0);
    }
}
