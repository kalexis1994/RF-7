//! The parameter contract.
//!
//! Two things have to agree about RF-7's parameters: this table, and the
//! package's `metadata/parameters.json` that RackForge reads. They are
//! compared against each other in `tests/schema.rs`, so a parameter added to
//! one and forgotten in the other fails the build rather than reaching a
//! surface as a control that does nothing.
//!
//! Nothing here duplicates a cartridge. Every parameter is either the DX7's
//! own function-parameter layer — which never lived in a voice — or an RF-7
//! control that sits on top of the loaded program, neutral at its default.

use rf7_dsp::{
    BEND_SEMITONES_DEFAULT, BEND_SEMITONES_MAX, BRIGHTNESS_MAX, Controls, DEFAULT_GAIN,
    ENVELOPE_TIME_MAX, ENVELOPE_TIME_MIN, GAIN_MAX, LFO_DELAY_MAX, LFO_DEPTH_MAX, LFO_RATE_MAX,
    LFO_RATE_MIN, OPERATORS, TRANSPOSE_MAX, TUNE_CENTS_MAX, Target, VELOCITY_DEPTH_MAX,
};

pub const GAIN: u32 = 0;
pub const BEND_RANGE: u32 = 1;
pub const MASTER_TUNE: u32 = 2;
pub const TRANSPOSE: u32 = 3;
pub const WHEEL_RANGE: u32 = 4;
pub const WHEEL_TARGET: u32 = 5;
pub const AFTERTOUCH_RANGE: u32 = 6;
pub const AFTERTOUCH_TARGET: u32 = 7;
pub const BRIGHTNESS: u32 = 8;
pub const ENVELOPE_TIME: u32 = 9;
pub const VELOCITY_DEPTH: u32 = 10;
/// The first of the six operator switches; OP(n) is `OPERATOR_FIRST + n - 1`.
pub const OPERATOR_FIRST: u32 = 11;
/// The performance layer over the program's LFO. These sit after the operator
/// switches because an index, once published, is what a saved session and a
/// MIDI link point at: new controls are appended, never inserted.
pub const LFO_RATE: u32 = OPERATOR_FIRST + OPERATORS as u32;
pub const LFO_DEPTH: u32 = LFO_RATE + 1;
pub const LFO_DELAY: u32 = LFO_RATE + 2;

pub const COUNT: usize = LFO_DELAY as usize + 1;

/// Which shape the schema must declare. The host draws from this; RF-7 only
/// needs it to check that the two descriptions of a parameter match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Float,
    Integer,
    Boolean,
    Enum,
}

#[derive(Clone, Copy, Debug)]
pub struct Parameter {
    pub id: &'static str,
    pub kind: Kind,
    pub minimum: f64,
    pub maximum: f64,
    pub default: f64,
}

pub const PARAMETERS: [Parameter; COUNT] = [
    float("gain", 0.0, GAIN_MAX, DEFAULT_GAIN),
    integer(
        "bend_range",
        0.0,
        BEND_SEMITONES_MAX as f64,
        BEND_SEMITONES_DEFAULT as f64,
    ),
    float(
        "master_tune",
        -(TUNE_CENTS_MAX as f64),
        TUNE_CENTS_MAX as f64,
        0.0,
    ),
    integer(
        "transpose",
        -(TRANSPOSE_MAX as f64),
        TRANSPOSE_MAX as f64,
        0.0,
    ),
    float("wheel_range", 0.0, 1.0, 1.0),
    choice("wheel_target", 0.0),
    float("aftertouch_range", 0.0, 1.0, 0.0),
    choice("aftertouch_target", 0.0),
    float("brightness", 0.0, BRIGHTNESS_MAX as f64, 1.0),
    float(
        "envelope_time",
        ENVELOPE_TIME_MIN as f64,
        ENVELOPE_TIME_MAX as f64,
        1.0,
    ),
    float("velocity_depth", 0.0, VELOCITY_DEPTH_MAX as f64, 1.0),
    switch("operator_1"),
    switch("operator_2"),
    switch("operator_3"),
    switch("operator_4"),
    switch("operator_5"),
    switch("operator_6"),
    float("lfo_rate", LFO_RATE_MIN as f64, LFO_RATE_MAX as f64, 1.0),
    float("lfo_depth", 0.0, LFO_DEPTH_MAX as f64, 0.0),
    float("lfo_delay", 0.0, LFO_DELAY_MAX as f64, 0.0),
];

const fn float(id: &'static str, minimum: f64, maximum: f64, default: f64) -> Parameter {
    Parameter {
        id,
        kind: Kind::Float,
        minimum,
        maximum,
        default,
    }
}

const fn integer(id: &'static str, minimum: f64, maximum: f64, default: f64) -> Parameter {
    Parameter {
        id,
        kind: Kind::Integer,
        minimum,
        maximum,
        default,
    }
}

/// Pitch, amplitude or both.
const fn choice(id: &'static str, default: f64) -> Parameter {
    Parameter {
        id,
        kind: Kind::Enum,
        minimum: 0.0,
        maximum: 2.0,
        default,
    }
}

const fn switch(id: &'static str) -> Parameter {
    Parameter {
        id,
        kind: Kind::Boolean,
        minimum: 0.0,
        maximum: 1.0,
        default: 1.0,
    }
}

pub fn defaults() -> [f64; COUNT] {
    let mut values = [0.0; COUNT];
    for (value, parameter) in values.iter_mut().zip(PARAMETERS) {
        *value = parameter.default;
    }
    values
}

/// Whether the host may set this parameter to this value.
///
/// A stepped parameter is not required to arrive whole: a fader that lands on
/// 2.4 means the third choice, and refusing it would silence a whole block
/// over a rounding difference. [`controls`] does the rounding.
pub fn is_valid(index: u32, value: f64) -> bool {
    let Some(parameter) = PARAMETERS.get(index as usize) else {
        return false;
    };
    value.is_finite() && value >= parameter.minimum && value <= parameter.maximum
}

/// The engine controls these values describe. Index [`GAIN`] is not among
/// them: output gain belongs to the engine's own level, not to its controls.
pub fn controls(values: &[f64; COUNT]) -> Controls {
    let mut operators = 0;
    for index in 0..OPERATORS {
        if values[OPERATOR_FIRST as usize + index] >= 0.5 {
            operators |= 1 << index;
        }
    }
    Controls {
        bend_semitones: values[BEND_RANGE as usize] as f32,
        master_tune_cents: values[MASTER_TUNE as usize] as f32,
        transpose: values[TRANSPOSE as usize].round() as i32,
        wheel_range: values[WHEEL_RANGE as usize] as f32,
        wheel_target: Target::from_index(values[WHEEL_TARGET as usize].round().max(0.0) as u32),
        aftertouch_range: values[AFTERTOUCH_RANGE as usize] as f32,
        aftertouch_target: Target::from_index(
            values[AFTERTOUCH_TARGET as usize].round().max(0.0) as u32
        ),
        brightness: values[BRIGHTNESS as usize] as f32,
        envelope_time: values[ENVELOPE_TIME as usize] as f32,
        velocity_depth: values[VELOCITY_DEPTH as usize] as f32,
        lfo_rate: values[LFO_RATE as usize] as f32,
        lfo_depth: values[LFO_DEPTH as usize] as f32,
        lfo_delay: values[LFO_DELAY as usize] as f32,
        operators,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rf7_dsp::ALL_OPERATORS;

    #[test]
    fn every_parameter_has_a_distinct_identifier_and_a_default_in_range() {
        for (index, parameter) in PARAMETERS.iter().enumerate() {
            assert!(parameter.minimum < parameter.maximum, "{}", parameter.id);
            assert!(
                (parameter.minimum..=parameter.maximum).contains(&parameter.default),
                "{} defaults outside its own range",
                parameter.id
            );
            assert!(
                !PARAMETERS[..index]
                    .iter()
                    .any(|earlier| earlier.id == parameter.id),
                "{} appears twice",
                parameter.id
            );
        }
    }

    #[test]
    fn the_declared_gain_default_is_the_engines_own() {
        // Two places could disagree about where the level starts, and the
        // schema test only compares this table against the package.
        let engine = rf7_dsp::Engine::new(48_000.0).expect("a supported rate");
        assert_eq!(defaults()[GAIN as usize], engine.gain());
    }

    #[test]
    fn the_defaults_describe_the_engines_own_defaults() {
        // If these ever drift apart, a freshly loaded plugin would sound
        // different from a freshly constructed engine for no visible reason.
        assert_eq!(controls(&defaults()), Controls::default());
        assert_eq!(controls(&defaults()).operators, ALL_OPERATORS);
    }

    #[test]
    fn a_value_outside_the_declared_range_is_refused() {
        for (index, parameter) in PARAMETERS.iter().enumerate() {
            let index = index as u32;
            assert!(is_valid(index, parameter.default));
            assert!(is_valid(index, parameter.minimum));
            assert!(is_valid(index, parameter.maximum));
            assert!(!is_valid(index, parameter.minimum - 0.001));
            assert!(!is_valid(index, parameter.maximum + 0.001));
            assert!(!is_valid(index, f64::NAN));
            assert!(!is_valid(index, f64::INFINITY));
        }
        assert!(!is_valid(COUNT as u32, 0.0));
        assert!(!is_valid(u32::MAX, 0.0));
    }

    #[test]
    fn a_stepped_parameter_rounds_rather_than_refusing() {
        let mut values = defaults();
        values[WHEEL_TARGET as usize] = 1.4;
        assert_eq!(controls(&values).wheel_target, Target::Amplitude);
        values[WHEEL_TARGET as usize] = 1.6;
        assert_eq!(controls(&values).wheel_target, Target::Both);
        values[TRANSPOSE as usize] = -11.5;
        assert_eq!(controls(&values).transpose, -12);
        values[OPERATOR_FIRST as usize] = 0.0;
        values[OPERATOR_FIRST as usize + 3] = 0.0;
        assert_eq!(controls(&values).operators, 0b110110);
    }
}
