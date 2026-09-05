//! The 128-byte packed voice, as it appears 32 times inside a cartridge dump.
//!
//! Six 17-byte operator blocks are stored OP6 first, followed by 16 voice
//! bytes and a 10-byte name. Six fields share bytes with a neighbour; the bit
//! positions below are the whole of that layout.

use crate::{
    Corrections, Decoded,
    parameters::{NAME_LENGTH, OPERATORS, Operator, Voice},
};

pub const PACKED_VOICE_LENGTH: usize = 128;

const OPERATOR_BLOCK: usize = 17;
const VOICE_BLOCK: usize = OPERATORS * OPERATOR_BLOCK;

/// Read one packed voice. Out-of-range bytes are clamped and counted rather
/// than rejected: cartridges in circulation carry values the DX7 ignored, and
/// refusing to open them would lose the other 31 voices with them.
pub fn decode_packed(bytes: &[u8; PACKED_VOICE_LENGTH]) -> Decoded {
    let mut voice = Voice::init();
    for (block, operator) in voice.operators.iter_mut().rev().enumerate() {
        let source = &bytes[block * OPERATOR_BLOCK..][..OPERATOR_BLOCK];
        *operator = Operator {
            eg_rate: [source[0], source[1], source[2], source[3]],
            eg_level: [source[4], source[5], source[6], source[7]],
            break_point: source[8],
            left_depth: source[9],
            right_depth: source[10],
            left_curve: source[11] & 3,
            right_curve: (source[11] >> 2) & 3,
            rate_scaling: source[12] & 7,
            amp_mod_sensitivity: source[13] & 3,
            velocity_sensitivity: (source[13] >> 2) & 7,
            output_level: source[14],
            fixed_frequency: source[15] & 1 != 0,
            coarse: (source[15] >> 1) & 31,
            fine: source[16],
            detune: (source[12] >> 3) & 15,
        };
    }
    let tail = &bytes[VOICE_BLOCK..];
    voice.pitch_eg_rate = [tail[0], tail[1], tail[2], tail[3]];
    voice.pitch_eg_level = [tail[4], tail[5], tail[6], tail[7]];
    voice.algorithm = tail[8] & 31;
    voice.feedback = tail[9] & 7;
    voice.oscillator_sync = (tail[9] >> 3) & 1 != 0;
    voice.lfo.speed = tail[10];
    voice.lfo.delay = tail[11];
    voice.lfo.pitch_mod_depth = tail[12];
    voice.lfo.amp_mod_depth = tail[13];
    voice.lfo.sync = tail[14] & 1 != 0;
    voice.lfo.waveform = (tail[14] >> 1) & 7;
    voice.pitch_mod_sensitivity = (tail[14] >> 4) & 7;
    voice.transpose = tail[15];
    voice.name.copy_from_slice(&tail[16..16 + NAME_LENGTH]);
    let corrections = voice.clamp();
    Decoded {
        voice,
        corrections: Corrections(corrections),
    }
}

/// Write one packed voice. Fields already in range round-trip exactly.
pub fn encode_packed(voice: &Voice) -> [u8; PACKED_VOICE_LENGTH] {
    let mut voice = *voice;
    voice.clamp();
    let mut bytes = [0u8; PACKED_VOICE_LENGTH];
    for (block, operator) in voice.operators.iter().rev().enumerate() {
        let target = &mut bytes[block * OPERATOR_BLOCK..][..OPERATOR_BLOCK];
        target[..4].copy_from_slice(&operator.eg_rate);
        target[4..8].copy_from_slice(&operator.eg_level);
        target[8] = operator.break_point;
        target[9] = operator.left_depth;
        target[10] = operator.right_depth;
        target[11] = operator.left_curve | (operator.right_curve << 2);
        target[12] = operator.rate_scaling | (operator.detune << 3);
        target[13] = operator.amp_mod_sensitivity | (operator.velocity_sensitivity << 2);
        target[14] = operator.output_level;
        target[15] = u8::from(operator.fixed_frequency) | (operator.coarse << 1);
        target[16] = operator.fine;
    }
    let tail = &mut bytes[VOICE_BLOCK..];
    tail[..4].copy_from_slice(&voice.pitch_eg_rate);
    tail[4..8].copy_from_slice(&voice.pitch_eg_level);
    tail[8] = voice.algorithm;
    tail[9] = voice.feedback | (u8::from(voice.oscillator_sync) << 3);
    tail[10] = voice.lfo.speed;
    tail[11] = voice.lfo.delay;
    tail[12] = voice.lfo.pitch_mod_depth;
    tail[13] = voice.lfo.amp_mod_depth;
    tail[14] =
        u8::from(voice.lfo.sync) | (voice.lfo.waveform << 1) | (voice.pitch_mod_sensitivity << 4);
    tail[15] = voice.transpose;
    tail[16..16 + NAME_LENGTH].copy_from_slice(&voice.name);
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packing_is_lossless_for_in_range_voices() {
        let voice = crate::factory_voice(0);
        let round_trip = decode_packed(&encode_packed(&voice));
        assert_eq!(round_trip.voice, voice);
        assert!(round_trip.corrections.is_clean());
    }

    #[test]
    fn shared_bytes_do_not_bleed_between_neighbouring_fields() {
        let mut voice = Voice::init();
        voice.operators[0].detune = 14;
        voice.operators[0].rate_scaling = 7;
        voice.operators[0].velocity_sensitivity = 7;
        voice.operators[0].amp_mod_sensitivity = 3;
        voice.operators[0].coarse = 31;
        voice.operators[0].fixed_frequency = true;
        voice.pitch_mod_sensitivity = 7;
        voice.lfo.waveform = 5;
        voice.lfo.sync = true;
        let decoded = decode_packed(&encode_packed(&voice)).voice;
        assert_eq!(decoded, voice);
    }

    #[test]
    fn out_of_range_bytes_are_clamped_and_counted() {
        // Every byte set: two operator bit fields already mask to a legal
        // value, so the count is below the number of fields.
        let decoded = decode_packed(&[0xff; PACKED_VOICE_LENGTH]);
        assert!(!decoded.corrections.is_clean());
        assert_eq!(decoded.voice.operators[0].output_level, 99);
        assert_eq!(decoded.voice.transpose, 48);
        assert_eq!(decoded.voice.algorithm, 31);
        assert_eq!(crate::printable_name(&decoded.voice.name), "");
    }
}
