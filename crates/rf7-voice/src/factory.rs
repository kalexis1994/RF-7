//! The voices RF-7 ships with.
//!
//! These are written for RF-7. They are not transcriptions of any Yamaha
//! cartridge, and no ROM voice data appears in this repository: a DX7 sound
//! the user owns arrives at runtime as a cartridge they supply. They are
//! starting points chosen to exercise different algorithm shapes, not
//! calibrated recreations of anything.

use crate::{
    library::Library,
    parameters::{OPERATORS, Operator, Voice},
};

pub const FACTORY_VOICES: usize = 8;

/// One of the [`FACTORY_VOICES`] voices. Out-of-range indexes give INIT VOICE.
pub fn factory_voice(index: usize) -> Voice {
    match index {
        0 => tines(),
        1 => bells(),
        2 => bass(),
        3 => brass(),
        4 => marimba(),
        5 => glass(),
        6 => organ(),
        7 => wood(),
        _ => Voice::init(),
    }
}

/// The factory voices, and nothing else.
///
/// Exactly eight, not eight padded out to a cartridge's thirty-two: a slot
/// holding INIT VOICE is a sine wave with a name, and offering twenty-four of
/// them as programs would be worse than offering none.
pub fn factory_library() -> Library {
    let mut voices = [Voice::init(); FACTORY_VOICES];
    for (index, voice) in voices.iter_mut().enumerate() {
        *voice = factory_voice(index);
    }
    Library::from_voices(&voices)
}

/// `operators` is given in panel order: index 0 is OP1.
fn voice(name: &[u8; 10], algorithm: u8, feedback: u8, operators: [Operator; OPERATORS]) -> Voice {
    Voice {
        name: *name,
        algorithm,
        feedback,
        operators,
        ..Voice::init()
    }
}

fn op(
    eg_rate: [u8; 4],
    eg_level: [u8; 4],
    output_level: u8,
    coarse: u8,
    detune: u8,
    velocity_sensitivity: u8,
) -> Operator {
    Operator {
        eg_rate,
        eg_level,
        output_level,
        coarse,
        detune,
        velocity_sensitivity,
        ..Operator::init()
    }
}

/// Rate scaling shortens the upper register, which every struck voice wants.
fn struck(mut operator: Operator, rate_scaling: u8) -> Operator {
    operator.rate_scaling = rate_scaling;
    operator
}

/// Algorithm 5: three independent modulator/carrier pairs. The stacked pairs
/// let the bright inharmonic partial decay at its own rate.
fn tines() -> Voice {
    voice(
        b"RF TINES  ",
        4,
        5,
        [
            struck(op([96, 42, 28, 52], [99, 88, 52, 0], 99, 1, 7, 6), 3),
            struck(op([97, 55, 32, 60], [99, 58, 0, 0], 74, 14, 8, 7), 4),
            struck(op([95, 40, 26, 50], [99, 90, 55, 0], 88, 1, 6, 5), 3),
            struck(op([96, 45, 30, 55], [99, 72, 30, 0], 68, 1, 5, 6), 3),
            struck(op([93, 38, 24, 48], [99, 86, 48, 0], 62, 1, 9, 4), 2),
            struck(op([98, 60, 40, 62], [99, 45, 0, 0], 66, 3, 7, 4), 4),
        ],
    )
}

/// Algorithm 5 again, with modulator ratios far from any harmonic series.
fn bells() -> Voice {
    voice(
        b"RF BELLS  ",
        4,
        3,
        [
            op([92, 25, 20, 42], [99, 80, 40, 0], 99, 1, 7, 4),
            op([95, 30, 22, 45], [99, 70, 20, 0], 80, 7, 9, 5),
            op([90, 22, 18, 40], [99, 78, 36, 0], 84, 1, 5, 3),
            op([93, 28, 20, 44], [99, 66, 18, 0], 76, 11, 6, 4),
            op([88, 20, 16, 38], [99, 72, 30, 0], 70, 1, 10, 3),
            op([94, 26, 20, 42], [99, 60, 14, 0], 72, 17, 4, 4),
        ],
    )
}

/// Algorithm 16: everything converges on OP1, which is what makes the low
/// register dense without spreading energy across several carriers.
fn bass() -> Voice {
    voice(
        b"RF BASS   ",
        15,
        6,
        [
            struck(op([98, 60, 35, 62], [99, 80, 45, 0], 99, 1, 7, 5), 2),
            struck(op([99, 70, 40, 66], [99, 55, 0, 0], 76, 1, 8, 6), 2),
            struck(op([99, 72, 45, 68], [99, 40, 0, 0], 62, 2, 6, 5), 3),
            struck(op([99, 80, 50, 70], [99, 30, 0, 0], 58, 3, 7, 4), 3),
            struck(op([99, 85, 55, 72], [99, 25, 0, 0], 48, 5, 9, 4), 3),
            struck(op([99, 90, 60, 75], [99, 20, 0, 0], 52, 1, 7, 3), 3),
        ],
    )
}

/// Algorithm 21: four carriers, two modulators, slow envelopes. Detuning the
/// carriers against one another is what widens it.
fn brass() -> Voice {
    voice(
        b"RF BRASS  ",
        20,
        4,
        [
            op([62, 40, 45, 55], [99, 90, 88, 0], 96, 1, 7, 5),
            op([58, 38, 44, 54], [99, 88, 86, 0], 92, 1, 9, 5),
            op([55, 42, 40, 52], [99, 78, 70, 0], 78, 1, 6, 6),
            op([60, 40, 44, 54], [99, 86, 84, 0], 88, 2, 5, 5),
            op([56, 36, 42, 52], [99, 84, 80, 0], 74, 1, 10, 4),
            op([52, 40, 38, 50], [99, 70, 62, 0], 70, 1, 8, 6),
        ],
    )
}

/// Algorithm 5, with no sustain segment at all: level 3 is silence.
fn marimba() -> Voice {
    voice(
        b"RF MARIMBA",
        4,
        2,
        [
            struck(op([99, 60, 45, 70], [99, 60, 0, 0], 99, 1, 7, 5), 5),
            struck(op([99, 70, 50, 72], [99, 40, 0, 0], 70, 4, 8, 6), 5),
            struck(op([99, 66, 48, 70], [99, 52, 0, 0], 78, 1, 5, 4), 5),
            struck(op([99, 74, 52, 74], [99, 30, 0, 0], 64, 9, 6, 5), 5),
            struck(op([99, 72, 50, 72], [99, 44, 0, 0], 60, 1, 10, 4), 4),
            struck(op([99, 80, 56, 76], [99, 22, 0, 0], 58, 13, 7, 5), 5),
        ],
    )
}

/// Algorithm 25: five carriers on a harmonic ladder, one modulator across the
/// top two. The ladder is the body; OP6 is the shimmer on the attack.
fn glass() -> Voice {
    voice(
        b"RF GLASS  ",
        24,
        5,
        [
            op([80, 30, 25, 45], [99, 85, 55, 0], 92, 1, 7, 3),
            op([78, 28, 24, 44], [99, 82, 52, 0], 82, 2, 9, 3),
            op([76, 26, 22, 42], [99, 80, 48, 0], 70, 3, 5, 4),
            op([74, 24, 20, 40], [99, 76, 44, 0], 62, 4, 10, 4),
            op([72, 22, 18, 38], [99, 72, 40, 0], 54, 6, 4, 4),
            op([85, 35, 28, 48], [99, 65, 30, 0], 74, 8, 8, 5),
        ],
    )
}

/// Algorithm 32: six carriers, no modulation. The ratios are drawbar
/// intervals, so this is additive synthesis with a DX7's envelopes.
fn organ() -> Voice {
    voice(
        b"RF ORGAN  ",
        31,
        0,
        [
            op([88, 50, 45, 70], [99, 95, 95, 0], 99, 1, 7, 2),
            op([88, 50, 45, 70], [99, 95, 95, 0], 88, 0, 7, 2),
            op([88, 50, 45, 70], [99, 95, 95, 0], 80, 2, 7, 2),
            op([88, 50, 45, 70], [99, 95, 95, 0], 70, 3, 7, 2),
            op([88, 50, 45, 70], [99, 95, 95, 0], 62, 4, 7, 2),
            op([88, 50, 45, 70], [99, 95, 95, 0], 55, 6, 7, 2),
        ],
    )
}

/// Algorithm 3: two three-deep stacks. A deep stack with fast envelopes is
/// how a wooden attack gets its noise without a noise source.
fn wood() -> Voice {
    voice(
        b"RF WOOD   ",
        2,
        7,
        [
            struck(op([99, 55, 40, 68], [99, 70, 20, 0], 99, 1, 7, 6), 4),
            struck(op([99, 65, 45, 70], [99, 45, 0, 0], 72, 3, 8, 6), 4),
            struck(op([99, 75, 50, 72], [99, 25, 0, 0], 60, 7, 6, 5), 5),
            struck(op([99, 58, 42, 68], [99, 66, 16, 0], 82, 1, 5, 5), 4),
            struck(op([99, 68, 48, 70], [99, 38, 0, 0], 66, 5, 9, 5), 4),
            struck(op([99, 78, 52, 74], [99, 20, 0, 0], 56, 11, 7, 5), 5),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_factory_voice_is_already_in_range() {
        for index in 0..FACTORY_VOICES {
            let mut voice = factory_voice(index);
            assert_eq!(voice.clamp(), 0, "factory voice {index} needed clamping");
            assert!(!crate::printable_name(&voice.name).is_empty());
        }
    }

    #[test]
    fn the_factory_library_offers_the_eight_voices_and_no_padding() {
        let library = factory_library();
        assert_eq!(library.len(), FACTORY_VOICES);
        assert_eq!(library.found(), FACTORY_VOICES);
        assert!(library.corrections().is_clean());
        assert_eq!(library.voice(0), Some(&factory_voice(0)));
        assert_eq!(library.voice(FACTORY_VOICES - 1), Some(&factory_voice(7)));
        assert_eq!(library.voice(FACTORY_VOICES), None);
    }
}
