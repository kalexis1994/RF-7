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

pub const FACTORY_VOICES: usize = 32;

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
        8 => ep_soft(),
        9 => ep_hard(),
        10 => piano(),
        11 => clav(),
        12 => harpsichord(),
        13 => guitar(),
        14 => koto(),
        15 => fretless(),
        16 => slap(),
        17 => sub_bass(),
        18 => horns(),
        19 => trumpet(),
        20 => sax(),
        21 => strings(),
        22 => pad(),
        23 => choir(),
        24 => drawbar(),
        25 => pipe(),
        26 => lead(),
        27 => square(),
        28 => vibes(),
        29 => tubular(),
        30 => steel(),
        31 => timpani(),
        _ => Voice::init(),
    }
}

/// The factory voices: one full cartridge of thirty-two, every slot a voice
/// written for RF-7. None is padding, and none is a transcription.
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

/// Hundredths of the coarse ratio. `fine(op, 50)` on coarse 3 is 3.5:1, which
/// is how the inharmonic partials of a bell or a drum are reached.
fn fine(mut operator: Operator, fine: u8) -> Operator {
    operator.fine = fine;
    operator
}

/// Keyboard level scaling. The usual use is a negative right side on a
/// modulator, so a patch stays bright in the middle and does not shriek at
/// the top of the keyboard.
fn scaled(
    mut operator: Operator,
    break_point: u8,
    left_depth: u8,
    right_depth: u8,
    left_curve: u8,
    right_curve: u8,
) -> Operator {
    operator.break_point = break_point;
    operator.left_depth = left_depth;
    operator.right_depth = right_depth;
    operator.left_curve = left_curve;
    operator.right_curve = right_curve;
    operator
}

/// How much this operator answers the LFO's amplitude modulation.
fn tremolo(mut operator: Operator, sensitivity: u8) -> Operator {
    operator.amp_mod_sensitivity = sensitivity;
    operator
}

/// The voice's LFO and its pitch modulation sensitivity.
fn lfo(
    mut voice: Voice,
    speed: u8,
    delay: u8,
    pitch_depth: u8,
    amp_depth: u8,
    waveform: u8,
    sensitivity: u8,
) -> Voice {
    voice.lfo.speed = speed;
    voice.lfo.delay = delay;
    voice.lfo.pitch_mod_depth = pitch_depth;
    voice.lfo.amp_mod_depth = amp_depth;
    voice.lfo.waveform = waveform;
    voice.pitch_mod_sensitivity = sensitivity;
    voice
}

/// The pitch envelope: level 50 is no change.
fn pitch_envelope(mut voice: Voice, rates: [u8; 4], levels: [u8; 4]) -> Voice {
    voice.pitch_eg_rate = rates;
    voice.pitch_eg_level = levels;
    voice
}

fn octave_down(mut voice: Voice) -> Voice {
    voice.transpose = 12;
    voice
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

/// Algorithm 5 again, gentler than RF TINES: the 14:1 tine sits low and only
/// opens with velocity, so a soft touch is mostly the warm 1:1 pair.
fn ep_soft() -> Voice {
    voice(
        b"RF EP SOFT",
        4,
        4,
        [
            struck(op([95, 35, 25, 50], [99, 90, 60, 0], 99, 1, 7, 3), 3),
            struck(op([96, 45, 30, 55], [99, 55, 0, 0], 70, 1, 8, 6), 4),
            struck(op([94, 33, 24, 48], [99, 88, 58, 0], 85, 1, 5, 3), 3),
            struck(
                scaled(
                    op([97, 50, 32, 58], [99, 50, 0, 0], 76, 14, 7, 7),
                    55,
                    0,
                    30,
                    0,
                    0,
                ),
                5,
            ),
            struck(op([92, 30, 22, 46], [99, 85, 50, 0], 70, 1, 9, 2), 2),
            struck(op([98, 55, 35, 60], [99, 40, 0, 0], 62, 2, 7, 4), 4),
        ],
    )
}

/// Algorithm 7: two carriers, one of them fed by a pair and a stack. The
/// bark is OP4 at 14:1, fully velocity-dependent.
fn ep_hard() -> Voice {
    voice(
        b"RF EP HARD",
        6,
        5,
        [
            struck(op([97, 40, 28, 52], [99, 85, 45, 0], 99, 1, 7, 4), 3),
            struck(op([98, 52, 34, 58], [99, 60, 0, 0], 82, 1, 8, 7), 4),
            struck(op([96, 38, 26, 50], [99, 84, 44, 0], 90, 1, 6, 4), 3),
            struck(
                scaled(
                    op([99, 60, 40, 62], [99, 45, 0, 0], 78, 14, 7, 7),
                    55,
                    0,
                    35,
                    0,
                    0,
                ),
                5,
            ),
            struck(op([97, 48, 32, 56], [99, 58, 0, 0], 74, 1, 9, 6), 4),
            struck(op([99, 65, 42, 64], [99, 30, 0, 0], 70, 3, 7, 5), 5),
        ],
    )
}

/// Algorithm 18: one carrier fed three ways. The stack behind OP4 is the
/// hammer, gone in a tenth of a second; OP2 at 1:1 is the body that stays.
fn piano() -> Voice {
    voice(
        b"RF PIANO  ",
        17,
        4,
        [
            struck(op([96, 32, 24, 45], [99, 88, 60, 0], 99, 1, 7, 3), 3),
            struck(op([97, 40, 28, 50], [99, 70, 20, 0], 80, 1, 9, 6), 4),
            struck(
                scaled(
                    op([98, 50, 32, 55], [99, 55, 0, 0], 74, 2, 6, 7),
                    50,
                    0,
                    40,
                    0,
                    0,
                ),
                5,
            ),
            struck(op([99, 58, 36, 60], [99, 40, 0, 0], 68, 3, 7, 6), 5),
            struck(op([99, 66, 40, 64], [99, 30, 0, 0], 60, 1, 8, 5), 5),
            struck(op([99, 75, 48, 70], [99, 20, 0, 0], 55, 5, 7, 5), 6),
        ],
    )
}

/// Algorithm 3: two three-deep stacks with almost no sustain. Deep stacks
/// with fast envelopes are what give a clavinet its snap.
fn clav() -> Voice {
    voice(
        b"RF CLAV   ",
        2,
        6,
        [
            struck(op([99, 62, 45, 72], [99, 55, 10, 0], 99, 1, 7, 5), 4),
            struck(op([99, 68, 48, 74], [99, 45, 0, 0], 84, 1, 8, 7), 5),
            struck(op([99, 75, 52, 76], [99, 35, 0, 0], 72, 3, 6, 7), 6),
            struck(op([99, 60, 44, 72], [99, 52, 8, 0], 88, 2, 6, 5), 4),
            struck(op([99, 70, 50, 75], [99, 40, 0, 0], 80, 1, 9, 7), 5),
            struck(op([99, 80, 56, 78], [99, 28, 0, 0], 74, 7, 7, 6), 6),
        ],
    )
}

/// Algorithm 5 with a buzzing feedback modulator and no sustain at all: a
/// plucked string that never had a soundboard to ring.
fn harpsichord() -> Voice {
    voice(
        b"RF HARPSI ",
        4,
        7,
        [
            struck(op([99, 55, 40, 68], [99, 60, 0, 0], 99, 1, 7, 2), 5),
            struck(op([99, 60, 44, 70], [99, 52, 0, 0], 84, 4, 8, 4), 6),
            struck(op([99, 54, 40, 68], [99, 58, 0, 0], 84, 2, 5, 2), 5),
            struck(op([99, 62, 45, 70], [99, 48, 0, 0], 78, 5, 7, 4), 6),
            struck(op([99, 56, 42, 68], [99, 55, 0, 0], 70, 1, 10, 2), 5),
            struck(op([99, 66, 48, 72], [99, 40, 0, 0], 80, 1, 7, 3), 6),
        ],
    )
}

/// Algorithm 8: the feedback sits on OP4, a modulator, so the pluck has a
/// nail in it and the ringing does not.
fn guitar() -> Voice {
    voice(
        b"RF GUITAR ",
        7,
        5,
        [
            struck(op([99, 50, 36, 66], [99, 70, 25, 0], 99, 1, 7, 4), 4),
            struck(op([99, 60, 42, 70], [99, 45, 0, 0], 78, 1, 9, 7), 5),
            struck(op([99, 48, 35, 65], [99, 68, 22, 0], 86, 1, 5, 4), 4),
            struck(op([99, 66, 46, 72], [99, 36, 0, 0], 76, 1, 7, 7), 5),
            struck(op([99, 58, 40, 68], [99, 48, 0, 0], 70, 2, 8, 6), 5),
            struck(op([99, 72, 50, 74], [99, 30, 0, 0], 60, 3, 7, 5), 6),
        ],
    )
}

/// Algorithm 5 with odd ratios and a short upward pitch blip at the pluck.
fn koto() -> Voice {
    pitch_envelope(
        voice(
            b"RF KOTO   ",
            4,
            3,
            [
                struck(op([99, 58, 44, 70], [99, 55, 0, 0], 99, 1, 7, 3), 5),
                struck(op([99, 64, 48, 72], [99, 42, 0, 0], 82, 3, 8, 6), 6),
                struck(op([99, 56, 42, 70], [99, 52, 0, 0], 80, 2, 5, 3), 5),
                struck(op([99, 70, 52, 74], [99, 35, 0, 0], 74, 5, 7, 6), 6),
                struck(op([99, 60, 46, 70], [99, 50, 0, 0], 62, 1, 10, 3), 5),
                struck(op([99, 74, 54, 76], [99, 30, 0, 0], 66, 7, 7, 5), 6),
            ],
        ),
        [99, 80, 99, 99],
        [58, 50, 50, 50],
    )
}

/// Algorithm 16, everything into OP1, with a slow swell and a delayed vibrato.
fn fretless() -> Voice {
    octave_down(lfo(
        voice(
            b"RF FRET BS",
            15,
            5,
            [
                struck(op([75, 45, 30, 60], [99, 85, 60, 0], 99, 1, 7, 3), 2),
                struck(op([80, 50, 32, 62], [99, 65, 20, 0], 76, 1, 8, 5), 2),
                struck(op([78, 55, 36, 64], [99, 50, 0, 0], 62, 2, 6, 6), 3),
                struck(op([85, 60, 40, 66], [99, 40, 0, 0], 55, 1, 7, 5), 3),
                struck(op([82, 58, 38, 64], [99, 45, 0, 0], 58, 3, 9, 6), 3),
                struck(op([88, 66, 44, 70], [99, 30, 0, 0], 52, 1, 7, 4), 3),
            ],
        ),
        32,
        45,
        18,
        0,
        4,
        2,
    ))
}

/// Algorithm 1: a four-deep stack into one carrier for the click, a pair for
/// the note. Feedback at the top of the stack is the thumb.
fn slap() -> Voice {
    octave_down(voice(
        b"RF SLAP   ",
        0,
        7,
        [
            struck(op([99, 62, 45, 70], [99, 75, 45, 0], 99, 1, 7, 5), 3),
            struck(op([99, 72, 50, 72], [99, 40, 0, 0], 84, 1, 9, 7), 4),
            struck(op([99, 60, 44, 70], [99, 70, 40, 0], 84, 1, 5, 5), 3),
            struck(op([99, 78, 54, 74], [99, 32, 0, 0], 78, 2, 7, 7), 5),
            struck(op([99, 84, 58, 76], [99, 25, 0, 0], 70, 3, 8, 7), 5),
            struck(op([99, 90, 62, 78], [99, 18, 0, 0], 76, 4, 7, 6), 6),
        ],
    ))
}

/// Algorithm 32: two carriers an octave apart and nothing modulating them,
/// plus a touch of second harmonic that fades. Additive, on purpose.
fn sub_bass() -> Voice {
    octave_down(voice(
        b"RF SUB    ",
        31,
        0,
        [
            struck(op([99, 50, 40, 68], [99, 90, 80, 0], 99, 1, 7, 2), 1),
            struck(op([99, 50, 40, 68], [99, 90, 80, 0], 92, 0, 7, 2), 1),
            struck(op([99, 60, 45, 70], [99, 60, 30, 0], 60, 2, 7, 4), 3),
            op([99, 99, 99, 99], [99, 99, 99, 0], 0, 1, 7, 0),
            op([99, 99, 99, 99], [99, 99, 99, 0], 0, 1, 7, 0),
            op([99, 99, 99, 99], [99, 99, 99, 0], 0, 1, 7, 0),
        ],
    ))
}

/// Algorithm 22: four carriers under one modulator, so the section moves as
/// one thing. Starts slightly flat and settles, the way a lip does.
fn horns() -> Voice {
    pitch_envelope(
        lfo(
            voice(
                b"RF HORNS  ",
                21,
                5,
                [
                    op([60, 40, 45, 55], [99, 88, 85, 0], 96, 1, 7, 4),
                    op([58, 42, 40, 52], [99, 75, 68, 0], 78, 1, 9, 6),
                    op([58, 38, 44, 54], [99, 86, 84, 0], 88, 1, 5, 4),
                    op([62, 40, 44, 54], [99, 86, 84, 0], 84, 1, 10, 4),
                    op([56, 36, 42, 52], [99, 84, 80, 0], 74, 2, 6, 3),
                    op([54, 44, 40, 50], [99, 72, 64, 0], 74, 1, 8, 6),
                ],
            ),
            34,
            30,
            20,
            0,
            4,
            2,
        ),
        [85, 70, 99, 99],
        [46, 50, 50, 50],
    )
}

/// Algorithm 1 with the stack carrier turned down: a single-voiced brass with
/// a sharper blip than the section, and feedback for the edge.
fn trumpet() -> Voice {
    pitch_envelope(
        lfo(
            voice(
                b"RF TRUMPET",
                0,
                6,
                [
                    op([70, 45, 50, 58], [99, 90, 88, 0], 99, 1, 7, 5),
                    op([66, 48, 42, 55], [99, 78, 70, 0], 82, 1, 8, 7),
                    op([68, 44, 48, 56], [99, 85, 82, 0], 60, 1, 6, 4),
                    op([64, 50, 44, 54], [99, 70, 60, 0], 70, 1, 7, 6),
                    op([62, 52, 44, 54], [99, 65, 55, 0], 60, 2, 9, 6),
                    op([60, 55, 45, 54], [99, 60, 50, 0], 64, 1, 7, 6),
                ],
            ),
            36,
            35,
            16,
            0,
            4,
            2,
        ),
        [90, 60, 99, 99],
        [42, 50, 50, 50],
    )
}

/// Algorithm 7 with a feedback modulator at 3:1 for the reed's grit, and a
/// slower vibrato than the brass.
fn sax() -> Voice {
    lfo(
        voice(
            b"RF SAX    ",
            6,
            7,
            [
                op([72, 40, 45, 58], [99, 88, 85, 0], 96, 1, 7, 5),
                op([68, 45, 40, 55], [99, 74, 66, 0], 80, 1, 9, 7),
                op([70, 38, 44, 56], [99, 86, 82, 0], 84, 1, 5, 5),
                op([75, 50, 42, 56], [99, 60, 50, 0], 66, 2, 7, 7),
                op([66, 46, 40, 54], [99, 70, 62, 0], 72, 1, 8, 6),
                op([80, 55, 45, 58], [99, 55, 40, 0], 70, 3, 7, 6),
            ],
        ),
        38,
        40,
        22,
        0,
        4,
        3,
    )
}

/// Algorithm 2: the feedback is on OP2, the modulator of the first pair,
/// which is what turns a sine into something bowed. Detuned carriers, slow
/// attack, delayed vibrato.
fn strings() -> Voice {
    lfo(
        voice(
            b"RF STRINGS",
            1,
            4,
            [
                op([50, 30, 40, 48], [99, 90, 88, 0], 96, 1, 5, 3),
                op([52, 35, 38, 46], [99, 70, 64, 0], 74, 1, 9, 5),
                op([48, 28, 40, 48], [99, 88, 86, 0], 92, 1, 10, 3),
                op([50, 34, 38, 46], [99, 66, 60, 0], 70, 1, 6, 5),
                op([52, 36, 38, 46], [99, 60, 50, 0], 62, 2, 8, 4),
                op([54, 38, 38, 46], [99, 50, 40, 0], 56, 3, 7, 4),
            ],
        ),
        34,
        45,
        24,
        0,
        4,
        3,
    )
}

/// Algorithm 20: one modulator shared by two detuned carriers, two more
/// modulators on a third. Slow everything.
fn pad() -> Voice {
    lfo(
        voice(
            b"RF PAD    ",
            19,
            3,
            [
                op([40, 25, 35, 42], [99, 92, 90, 0], 94, 1, 4, 2),
                op([38, 24, 35, 42], [99, 92, 90, 0], 90, 1, 11, 2),
                op([42, 30, 32, 40], [99, 70, 62, 0], 72, 2, 7, 4),
                op([36, 22, 34, 40], [99, 90, 88, 0], 86, 1, 7, 2),
                op([44, 32, 32, 40], [99, 64, 56, 0], 66, 3, 6, 4),
                op([46, 34, 32, 40], [99, 58, 48, 0], 60, 1, 9, 4),
            ],
        ),
        28,
        50,
        20,
        0,
        4,
        3,
    )
}

/// Algorithm 25: five carriers on the first harmonics with the top two
/// lightly modulated, and a little tremolo on the lowest for breath.
fn choir() -> Voice {
    lfo(
        voice(
            b"RF CHOIR  ",
            24,
            4,
            [
                tremolo(op([45, 30, 40, 44], [99, 90, 88, 0], 90, 1, 7, 2), 1),
                tremolo(op([44, 30, 40, 44], [99, 88, 86, 0], 84, 2, 9, 2), 1),
                op([46, 30, 40, 44], [99, 84, 80, 0], 76, 3, 5, 2),
                op([42, 28, 40, 44], [99, 86, 84, 0], 80, 1, 10, 2),
                op([43, 28, 40, 44], [99, 82, 78, 0], 70, 4, 6, 2),
                op([50, 35, 36, 42], [99, 60, 52, 0], 60, 1, 8, 4),
            ],
        ),
        30,
        40,
        18,
        15,
        4,
        3,
    )
}

/// Algorithm 32 on drawbar footages: 16', 8', 5⅓', 4', 2⅔', 2'. Fast release
/// and a little tremolo, as an organ has.
fn drawbar() -> Voice {
    let bar = |output: u8, coarse: u8| {
        tremolo(
            op([99, 50, 50, 80], [99, 96, 96, 0], output, coarse, 7, 1),
            1,
        )
    };
    lfo(
        voice(
            b"RF DRAWBAR",
            31,
            0,
            [
                bar(99, 1),
                bar(92, 0),
                fine(bar(74, 1), 50),
                bar(82, 2),
                bar(64, 3),
                bar(58, 4),
            ],
        ),
        40,
        0,
        0,
        25,
        4,
        0,
    )
}

/// Algorithm 31: four steady carriers, and a fifth whose modulator is gone in
/// a moment. That moment is the chiff, and it is all a pipe's attack is.
fn pipe() -> Voice {
    voice(
        b"RF PIPE   ",
        30,
        5,
        [
            op([70, 40, 45, 60], [99, 92, 90, 0], 96, 1, 7, 1),
            op([68, 40, 45, 60], [99, 88, 86, 0], 82, 2, 7, 1),
            op([66, 40, 45, 60], [99, 80, 78, 0], 66, 4, 7, 1),
            op([72, 40, 45, 60], [99, 76, 74, 0], 56, 0, 7, 1),
            op([95, 60, 45, 60], [99, 60, 55, 0], 70, 3, 7, 2),
            struck(op([99, 70, 50, 65], [99, 30, 0, 0], 78, 5, 7, 3), 4),
        ],
    )
}

/// Algorithm 22 with every carrier up and feedback at full: the modulator
/// on top of four carriers is what makes a lead cut.
fn lead() -> Voice {
    lfo(
        voice(
            b"RF LEAD   ",
            21,
            7,
            [
                op([99, 50, 45, 60], [99, 90, 88, 0], 99, 1, 7, 4),
                op([99, 55, 45, 60], [99, 80, 75, 0], 86, 1, 8, 6),
                op([99, 50, 45, 60], [99, 88, 86, 0], 90, 1, 4, 4),
                op([99, 50, 45, 60], [99, 88, 86, 0], 90, 1, 10, 4),
                op([99, 50, 45, 60], [99, 86, 84, 0], 80, 2, 7, 3),
                op([99, 55, 45, 60], [99, 78, 72, 0], 82, 1, 7, 6),
            ],
        ),
        40,
        30,
        28,
        0,
        4,
        3,
    )
}

/// Algorithm 2 with the modulator at 2:1 and its own feedback: a modulator
/// at twice the carrier adds odd harmonics, which is what a square wave is.
fn square() -> Voice {
    lfo(
        voice(
            b"RF SQUARE ",
            1,
            6,
            [
                op([99, 45, 45, 58], [99, 90, 88, 0], 99, 1, 7, 3),
                op([99, 50, 45, 58], [99, 82, 78, 0], 84, 2, 7, 5),
                op([99, 45, 45, 58], [99, 80, 78, 0], 60, 1, 9, 3),
                op([99, 50, 45, 58], [99, 60, 55, 0], 60, 2, 7, 5),
                op([99, 55, 45, 58], [99, 40, 30, 0], 40, 1, 7, 4),
                op([99, 60, 45, 58], [99, 30, 20, 0], 40, 1, 7, 4),
            ],
        ),
        38,
        30,
        20,
        0,
        4,
        2,
    )
}

/// Algorithm 5 with a 4:1 modulator and no feedback, under a fast tremolo:
/// the tremolo is the motor of a vibraphone, and it is the whole point.
fn vibes() -> Voice {
    lfo(
        voice(
            b"RF VIBES  ",
            4,
            0,
            [
                tremolo(
                    struck(op([99, 30, 20, 45], [99, 85, 50, 0], 99, 1, 7, 3), 4),
                    2,
                ),
                struck(op([99, 40, 25, 50], [99, 60, 10, 0], 72, 4, 7, 6), 5),
                tremolo(
                    struck(op([99, 28, 20, 44], [99, 82, 48, 0], 80, 1, 8, 3), 4),
                    2,
                ),
                struck(op([99, 45, 28, 52], [99, 50, 0, 0], 66, 1, 7, 5), 5),
                tremolo(
                    struck(op([99, 32, 22, 46], [99, 80, 44, 0], 60, 2, 6, 3), 4),
                    2,
                ),
                struck(op([99, 50, 30, 54], [99, 40, 0, 0], 56, 7, 7, 5), 6),
            ],
        ),
        45,
        0,
        0,
        60,
        4,
        0,
    )
}

/// Algorithm 5 on ratios no harmonic series contains — 3.5, 2.41, 6.3 — and
/// the longest decays in the bank. A tubular bell is inharmonic or it is a
/// flute.
fn tubular() -> Voice {
    voice(
        b"RF TUBULAR",
        4,
        5,
        [
            struck(op([99, 22, 16, 40], [99, 80, 40, 0], 99, 1, 7, 3), 2),
            struck(
                fine(op([99, 28, 20, 44], [99, 70, 25, 0], 78, 3, 8, 4), 50),
                3,
            ),
            struck(op([99, 20, 15, 38], [99, 78, 38, 0], 86, 1, 5, 3), 2),
            struck(
                fine(op([99, 26, 18, 42], [99, 66, 20, 0], 74, 2, 6, 4), 41),
                3,
            ),
            struck(op([99, 24, 18, 40], [99, 74, 34, 0], 72, 1, 10, 3), 2),
            struck(
                fine(op([99, 30, 22, 46], [99, 60, 15, 0], 70, 6, 7, 4), 30),
                3,
            ),
        ],
    )
}

/// Algorithm 19: three carriers, one under a two-deep stack and two under a
/// shared feedback modulator, all on inharmonic ratios. Bright, and gone.
fn steel() -> Voice {
    voice(
        b"RF STEEL  ",
        18,
        5,
        [
            struck(op([99, 45, 32, 60], [99, 66, 20, 0], 99, 1, 7, 4), 5),
            struck(
                fine(op([99, 55, 38, 64], [99, 50, 0, 0], 78, 1, 8, 6), 41),
                5,
            ),
            struck(
                fine(op([99, 65, 44, 68], [99, 38, 0, 0], 68, 2, 7, 6), 88),
                6,
            ),
            struck(op([99, 42, 30, 58], [99, 64, 18, 0], 84, 2, 5, 4), 5),
            struck(op([99, 48, 34, 60], [99, 60, 16, 0], 74, 3, 10, 4), 5),
            struck(
                fine(op([99, 60, 40, 66], [99, 44, 0, 0], 76, 1, 7, 6), 57),
                6,
            ),
        ],
    )
}

/// Algorithm 16 an octave down with a pitch envelope that falls after the
/// strike, which is what a drumhead does when it is hit.
fn timpani() -> Voice {
    octave_down(pitch_envelope(
        voice(
            b"RF TIMPANI",
            15,
            6,
            [
                struck(op([99, 40, 28, 55], [99, 70, 20, 0], 99, 1, 7, 4), 3),
                struck(
                    fine(op([99, 55, 36, 60], [99, 45, 0, 0], 74, 1, 8, 6), 50),
                    4,
                ),
                struck(
                    fine(op([99, 62, 40, 62], [99, 38, 0, 0], 66, 2, 6, 6), 30),
                    4,
                ),
                struck(op([99, 70, 46, 66], [99, 30, 0, 0], 58, 3, 7, 5), 5),
                struck(op([99, 66, 42, 64], [99, 34, 0, 0], 62, 1, 9, 6), 4),
                struck(op([99, 80, 52, 70], [99, 22, 0, 0], 70, 0, 7, 5), 5),
            ],
        ),
        [99, 75, 99, 99],
        [62, 50, 50, 50],
    ))
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
    fn the_factory_library_is_one_full_cartridge() {
        let library = factory_library();
        assert_eq!(library.len(), FACTORY_VOICES);
        assert_eq!(library.found(), FACTORY_VOICES);
        assert!(library.corrections().is_clean());
        assert_eq!(library.voice(0), Some(&factory_voice(0)));
        assert_eq!(library.voice(FACTORY_VOICES - 1), Some(&factory_voice(31)));
        assert_eq!(library.voice(FACTORY_VOICES), None);
    }

    #[test]
    fn every_factory_voice_has_its_own_name() {
        // The crate is no_std, so no set: a nested loop over thirty-two names.
        for index in 0..FACTORY_VOICES {
            for earlier in 0..index {
                assert_ne!(
                    factory_voice(index).name,
                    factory_voice(earlier).name,
                    "voices {earlier} and {index} share a name"
                );
            }
            assert!(
                factory_voice(index).name.starts_with(b"RF "),
                "voice {index} is not marked as RF-7's own"
            );
        }
    }
}
