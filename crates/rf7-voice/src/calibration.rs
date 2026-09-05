//! The calibration voice: two operators and nothing that could confuse a
//! measurement.
//!
//! OP2 modulates OP1 at four times its frequency, so every sideband lands on
//! its own odd harmonic and none folds onto another. Envelopes are flat,
//! velocity and keyboard scaling are off, the LFO does nothing, and the other
//! four operators are silent. Played on a real DX7 and recorded dry, the
//! spectrum of a held note gives that instrument's modulation index at the
//! modulator's output level — the one number RF-7 most needs and cannot
//! derive from a cartridge.

use crate::{
    library::Library,
    parameters::{Operator, Voice},
    sysex::{Cartridge, VOICES_PER_CARTRIDGE},
};

/// The carrier is OP1 at 1:1 and full level; OP2 is the modulator at 4:1 and
/// the given output level. Algorithm 1 routes OP2 into OP1 and leaves OP3 —
/// the other carrier — silent at level 0.
pub fn calibration_voice(modulator_level: u8) -> Voice {
    let flat = Operator {
        eg_rate: [99, 99, 99, 99],
        eg_level: [99, 99, 99, 0],
        velocity_sensitivity: 0,
        rate_scaling: 0,
        detune: 7,
        ..Operator::init()
    };
    let mut operators = [Operator {
        output_level: 0,
        ..flat
    }; 6];
    operators[0] = Operator {
        output_level: 99,
        coarse: 1,
        ..flat
    };
    operators[1] = Operator {
        output_level: modulator_level.min(99),
        coarse: 4,
        ..flat
    };
    let mut name = *b"RF CAL    ";
    name[7] = b'0' + modulator_level.min(99) / 10;
    name[8] = b'0' + modulator_level.min(99) % 10;
    let mut voice = Voice {
        operators,
        algorithm: 0,
        feedback: 0,
        oscillator_sync: true,
        pitch_mod_sensitivity: 0,
        name,
        ..Voice::init()
    };
    voice.lfo.pitch_mod_depth = 0;
    voice.lfo.amp_mod_depth = 0;
    voice
}

/// Modulator levels for one full calibration cartridge: 99 down to 6 in
/// steps of three, so a single recording session covers the level curve.
pub fn calibration_levels() -> [u8; VOICES_PER_CARTRIDGE] {
    let mut levels = [0u8; VOICES_PER_CARTRIDGE];
    for (slot, level) in levels.iter_mut().enumerate() {
        *level = 99 - 3 * slot as u8;
    }
    levels
}

/// The cartridge to send to a DX7 for a calibration session.
pub fn calibration_cartridge() -> Cartridge {
    let mut voices = [Voice::init(); VOICES_PER_CARTRIDGE];
    for (voice, level) in voices.iter_mut().zip(calibration_levels()) {
        *voice = calibration_voice(level);
    }
    Cartridge::from_voices(voices)
}

/// The same thirty-two voices as a library, for the laboratory to render.
pub fn calibration_library() -> Library {
    Library::from_voices(calibration_cartridge().voices())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::printable_name;

    #[test]
    fn the_calibration_voice_has_nothing_that_could_move_the_measurement() {
        let voice = calibration_voice(80);
        let mut checked = voice;
        assert_eq!(checked.clamp(), 0, "in range by construction");
        assert_eq!(voice.algorithm, 0);
        assert_eq!(voice.feedback, 0);
        assert_eq!(voice.lfo.pitch_mod_depth, 0);
        assert_eq!(voice.lfo.amp_mod_depth, 0);
        assert_eq!(voice.pitch_eg_level, [50, 50, 50, 50]);
        assert_eq!(voice.operators[0].output_level, 99);
        assert_eq!(voice.operators[0].coarse, 1);
        assert_eq!(voice.operators[1].output_level, 80);
        assert_eq!(voice.operators[1].coarse, 4);
        for operator in &voice.operators {
            assert_eq!(operator.velocity_sensitivity, 0);
            assert_eq!(operator.left_depth, 0);
            assert_eq!(operator.right_depth, 0);
            assert_eq!(operator.eg_rate, [99, 99, 99, 99]);
        }
        for operator in &voice.operators[2..] {
            assert_eq!(operator.output_level, 0);
        }
        assert_eq!(printable_name(&voice.name), "RF CAL 80");
        assert_eq!(printable_name(&calibration_voice(6).name), "RF CAL 06");
        assert_eq!(printable_name(&calibration_voice(200).name), "RF CAL 99");
    }

    #[test]
    fn the_cartridge_walks_the_level_curve_from_the_top() {
        let levels = calibration_levels();
        assert_eq!(levels[0], 99);
        assert_eq!(levels[31], 6);
        let cartridge = calibration_cartridge();
        assert!(cartridge.corrections().is_clean());
        assert_eq!(cartridge.voice(0).unwrap().operators[1].output_level, 99);
        assert_eq!(cartridge.voice(31).unwrap().operators[1].output_level, 6);
        assert_eq!(calibration_library().len(), VOICES_PER_CARTRIDGE);
    }
}
