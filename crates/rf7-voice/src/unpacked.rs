//! The 155-byte unpacked voice, as it appears in a single-voice dump and in
//! the DX7's edit buffer.
//!
//! One parameter per byte, six 21-byte operator blocks stored OP6 first, then
//! 19 voice bytes and a 10-byte name. Nothing shares a byte here, which is why
//! this layout — not the packed one — is the readable statement of the model.

use crate::{
    Corrections, Decoded,
    parameters::{NAME_LENGTH, OPERATORS, Operator, Voice},
};

pub const UNPACKED_VOICE_LENGTH: usize = 155;

const OPERATOR_BLOCK: usize = 21;
const VOICE_BLOCK: usize = OPERATORS * OPERATOR_BLOCK;

pub fn decode_unpacked(bytes: &[u8; UNPACKED_VOICE_LENGTH]) -> Decoded {
    let mut voice = Voice::init();
    for (block, operator) in voice.operators.iter_mut().rev().enumerate() {
        let source = &bytes[block * OPERATOR_BLOCK..][..OPERATOR_BLOCK];
        *operator = Operator {
            eg_rate: [source[0], source[1], source[2], source[3]],
            eg_level: [source[4], source[5], source[6], source[7]],
            break_point: source[8],
            left_depth: source[9],
            right_depth: source[10],
            left_curve: source[11],
            right_curve: source[12],
            rate_scaling: source[13],
            amp_mod_sensitivity: source[14],
            velocity_sensitivity: source[15],
            output_level: source[16],
            fixed_frequency: source[17] & 1 != 0,
            coarse: source[18],
            fine: source[19],
            detune: source[20],
        };
    }
    let tail = &bytes[VOICE_BLOCK..];
    voice.pitch_eg_rate = [tail[0], tail[1], tail[2], tail[3]];
    voice.pitch_eg_level = [tail[4], tail[5], tail[6], tail[7]];
    voice.algorithm = tail[8];
    voice.feedback = tail[9];
    voice.oscillator_sync = tail[10] & 1 != 0;
    voice.lfo.speed = tail[11];
    voice.lfo.delay = tail[12];
    voice.lfo.pitch_mod_depth = tail[13];
    voice.lfo.amp_mod_depth = tail[14];
    voice.lfo.sync = tail[15] & 1 != 0;
    voice.lfo.waveform = tail[16];
    voice.pitch_mod_sensitivity = tail[17];
    voice.transpose = tail[18];
    voice.name.copy_from_slice(&tail[19..19 + NAME_LENGTH]);
    let corrections = voice.clamp();
    Decoded {
        voice,
        corrections: Corrections(corrections),
    }
}

pub fn encode_unpacked(voice: &Voice) -> [u8; UNPACKED_VOICE_LENGTH] {
    let mut voice = *voice;
    voice.clamp();
    let mut bytes = [0u8; UNPACKED_VOICE_LENGTH];
    for (block, operator) in voice.operators.iter().rev().enumerate() {
        let target = &mut bytes[block * OPERATOR_BLOCK..][..OPERATOR_BLOCK];
        target[..4].copy_from_slice(&operator.eg_rate);
        target[4..8].copy_from_slice(&operator.eg_level);
        target[8] = operator.break_point;
        target[9] = operator.left_depth;
        target[10] = operator.right_depth;
        target[11] = operator.left_curve;
        target[12] = operator.right_curve;
        target[13] = operator.rate_scaling;
        target[14] = operator.amp_mod_sensitivity;
        target[15] = operator.velocity_sensitivity;
        target[16] = operator.output_level;
        target[17] = u8::from(operator.fixed_frequency);
        target[18] = operator.coarse;
        target[19] = operator.fine;
        target[20] = operator.detune;
    }
    let tail = &mut bytes[VOICE_BLOCK..];
    tail[..4].copy_from_slice(&voice.pitch_eg_rate);
    tail[4..8].copy_from_slice(&voice.pitch_eg_level);
    tail[8] = voice.algorithm;
    tail[9] = voice.feedback;
    tail[10] = u8::from(voice.oscillator_sync);
    tail[11] = voice.lfo.speed;
    tail[12] = voice.lfo.delay;
    tail[13] = voice.lfo.pitch_mod_depth;
    tail[14] = voice.lfo.amp_mod_depth;
    tail[15] = u8::from(voice.lfo.sync);
    tail[16] = voice.lfo.waveform;
    tail[17] = voice.pitch_mod_sensitivity;
    tail[18] = voice.transpose;
    tail[19..19 + NAME_LENGTH].copy_from_slice(&voice.name);
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_layouts_describe_the_same_voice() {
        for index in 0..crate::FACTORY_VOICES {
            let voice = crate::factory_voice(index);
            let through_packed = crate::decode_packed(&crate::encode_packed(&voice)).voice;
            let through_unpacked = decode_unpacked(&encode_unpacked(&voice)).voice;
            assert_eq!(through_packed, through_unpacked);
            assert_eq!(through_unpacked, voice);
        }
    }

    #[test]
    fn every_byte_is_read() {
        // A byte the decoder ignores would let two different dumps produce the
        // same voice; a full sweep of single-byte changes catches that. The
        // name carries no space, so flipping a name byte to 0 does not clamp
        // straight back to the byte it replaced.
        let mut source = Voice::init();
        source.name = *b"REACHALLOF";
        let reference = encode_unpacked(&source);
        let decoded = decode_unpacked(&reference).voice;
        for index in 0..UNPACKED_VOICE_LENGTH {
            let mut altered = reference;
            altered[index] = if reference[index] == 0 { 1 } else { 0 };
            assert_ne!(
                decode_unpacked(&altered).voice,
                decoded,
                "byte {index} does not reach the voice"
            );
        }
    }
}
