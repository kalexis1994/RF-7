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

/// A fixed pitch, ignoring the key: `decade` 0..=3 is 1, 10, 100 or
/// 1000 Hz and `fine` climbs a decade in hundredths, so (1, 80) is 63 Hz.
fn fixed_hz(mut operator: Operator, decade: u8, fine: u8) -> Operator {
    operator.fixed_frequency = true;
    operator.coarse = decade;
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
        6,
        [
            struck(op([96, 30, 28, 52], [99, 88, 52, 0], 99, 1, 7, 6), 3),
            struck(op([97, 55, 32, 60], [99, 58, 0, 0], 82, 14, 8, 7), 3),
            struck(op([95, 30, 26, 50], [99, 90, 55, 0], 94, 1, 6, 5), 3),
            struck(op([95, 29, 20, 50], [99, 95, 0, 0], 88, 1, 5, 6), 3),
            struck(op([93, 30, 24, 48], [99, 86, 48, 0], 68, 1, 9, 4), 2),
            struck(op([95, 29, 20, 50], [99, 95, 0, 0], 80, 3, 7, 4), 3),
        ],
    )
}

/// Algorithm 5 again, with modulator ratios far from any harmonic series.
fn bells() -> Voice {
    voice(
        b"RF BELLS  ",
        4,
        4,
        [
            op([92, 25, 20, 42], [99, 80, 40, 0], 99, 1, 7, 4),
            op([95, 30, 22, 45], [99, 70, 20, 0], 94, 7, 9, 5),
            op([90, 22, 18, 40], [99, 78, 36, 0], 90, 1, 5, 3),
            op([93, 28, 20, 44], [99, 66, 18, 0], 86, 11, 6, 4),
            op([88, 20, 16, 38], [99, 72, 30, 0], 76, 1, 10, 3),
            op([94, 26, 20, 42], [99, 60, 14, 0], 82, 17, 4, 4),
        ],
    )
}

/// Algorithm 16: everything converges on OP1, which is what makes the low
/// register dense without spreading energy across several carriers.
fn bass() -> Voice {
    voice(
        b"RF BASS   ",
        15,
        7,
        [
            struck(op([98, 60, 35, 62], [99, 80, 45, 0], 99, 1, 7, 5), 2),
            struck(op([99, 70, 40, 66], [99, 55, 0, 0], 90, 1, 8, 6), 2),
            struck(op([99, 72, 45, 68], [99, 40, 0, 0], 72, 2, 6, 5), 3),
            struck(op([99, 80, 50, 70], [99, 30, 0, 0], 68, 3, 7, 4), 3),
            struck(op([99, 85, 55, 72], [99, 25, 0, 0], 58, 5, 9, 4), 3),
            struck(op([99, 90, 60, 75], [99, 20, 0, 0], 62, 1, 7, 3), 3),
        ],
    )
}

/// Algorithm 22, the instrument's own brass architecture: OP6 at 1:1 with
/// full feedback drives three carriers detuned a few cents apart, beside a
/// sub-octave pair. The modulator blooms in behind the carriers and then
/// holds, darkened away from middle C by its keyboard scaling.
fn brass() -> Voice {
    pitch_envelope(
        lfo(
            voice(
                b"RF BRASS  ",
                21,
                7,
                [
                    tremolo(
                        struck(op([76, 70, 90, 70], [99, 97, 98, 0], 96, 0, 12, 1), 1),
                        3,
                    ),
                    op([58, 50, 30, 70], [88, 96, 96, 0], 80, 0, 12, 1),
                    tremolo(
                        struck(op([77, 40, 45, 70], [99, 97, 97, 0], 99, 1, 5, 2), 1),
                        3,
                    ),
                    tremolo(
                        struck(op([77, 40, 45, 70], [99, 97, 97, 0], 96, 1, 7, 2), 1),
                        3,
                    ),
                    tremolo(
                        struck(op([77, 40, 45, 70], [99, 97, 97, 0], 93, 1, 8, 2), 1),
                        3,
                    ),
                    tremolo(
                        scaled(
                            struck(op([50, 85, 30, 68], [97, 96, 90, 0], 80, 1, 7, 3), 3),
                            39,
                            50,
                            48,
                            1,
                            1,
                        ),
                        2,
                    ),
                ],
            ),
            37,
            25,
            6,
            0,
            4,
            3,
        ),
        [90, 80, 99, 99],
        [47, 50, 50, 50],
    )
}

/// Algorithm 5, with no sustain segment at all: level 3 is silence.
fn marimba() -> Voice {
    voice(
        b"RF MARIMBA",
        4,
        2,
        [
            struck(op([99, 45, 45, 70], [99, 60, 0, 0], 99, 1, 7, 5), 3),
            struck(op([99, 70, 50, 72], [99, 40, 0, 0], 72, 4, 8, 6), 3),
            struck(op([99, 45, 45, 70], [99, 52, 0, 0], 84, 1, 5, 4), 3),
            struck(op([99, 70, 52, 74], [99, 30, 0, 0], 62, 9, 6, 5), 3),
            struck(op([99, 45, 45, 72], [99, 44, 0, 0], 66, 1, 10, 4), 3),
            struck(op([99, 70, 56, 76], [99, 22, 0, 0], 56, 13, 7, 5), 3),
        ],
    )
}

/// Algorithm 25: five carriers on a harmonic ladder, one modulator across the
/// top two. The ladder is the body; OP6 is the shimmer on the attack.
fn glass() -> Voice {
    voice(
        b"RF GLASS  ",
        24,
        6,
        [
            op([80, 30, 25, 45], [99, 85, 55, 0], 98, 1, 7, 3),
            op([78, 28, 24, 44], [99, 82, 52, 0], 88, 2, 9, 3),
            op([76, 26, 22, 42], [99, 80, 48, 0], 76, 3, 5, 4),
            op([74, 24, 20, 40], [99, 76, 44, 0], 68, 4, 10, 4),
            op([72, 22, 18, 38], [99, 72, 40, 0], 60, 6, 4, 4),
            op([85, 35, 28, 48], [99, 65, 30, 0], 88, 8, 8, 5),
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
            tremolo(op([88, 50, 45, 70], [99, 95, 95, 0], 99, 1, 7, 2), 2),
            op([88, 50, 45, 70], [99, 95, 95, 0], 94, 0, 7, 2),
            tremolo(op([88, 50, 45, 70], [99, 95, 95, 0], 86, 2, 7, 2), 2),
            op([88, 50, 45, 70], [99, 95, 95, 0], 76, 3, 7, 2),
            tremolo(op([88, 50, 45, 70], [99, 95, 95, 0], 68, 4, 7, 2), 2),
            op([88, 50, 45, 70], [99, 95, 95, 0], 61, 6, 7, 2),
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
            struck(op([99, 45, 40, 68], [99, 70, 20, 0], 99, 1, 7, 6), 3),
            struck(op([99, 65, 45, 70], [99, 45, 0, 0], 86, 3, 8, 6), 3),
            struck(op([99, 70, 50, 72], [99, 25, 0, 0], 70, 7, 6, 5), 3),
            struck(op([99, 45, 42, 68], [99, 66, 16, 0], 88, 1, 5, 5), 3),
            struck(op([99, 68, 48, 70], [99, 38, 0, 0], 76, 5, 9, 5), 3),
            struck(op([99, 70, 52, 74], [99, 20, 0, 0], 66, 11, 7, 5), 3),
        ],
    )
}

/// Algorithm 5 again, gentler than RF TINES: the 14:1 tine sits low and only
/// opens with velocity, so a soft touch is mostly the warm 1:1 pair.
fn ep_soft() -> Voice {
    voice(
        b"RF EP SOFT",
        4,
        5,
        [
            struck(op([95, 30, 25, 50], [99, 90, 60, 0], 99, 1, 7, 3), 3),
            struck(op([95, 29, 20, 50], [99, 95, 0, 0], 86, 1, 8, 6), 3),
            struck(op([94, 30, 24, 48], [99, 88, 58, 0], 91, 1, 5, 3), 3),
            struck(
                scaled(
                    op([97, 50, 32, 58], [99, 50, 0, 0], 84, 14, 7, 7),
                    55,
                    0,
                    30,
                    0,
                    0,
                ),
                3,
            ),
            struck(op([92, 30, 22, 46], [99, 85, 50, 0], 76, 1, 9, 2), 2),
            struck(op([95, 29, 20, 50], [99, 95, 0, 0], 78, 2, 7, 4), 3),
        ],
    )
}

/// Algorithm 7: two carriers, one of them fed by a pair and a stack. The
/// bark is OP4 at 14:1, fully velocity-dependent.
fn ep_hard() -> Voice {
    voice(
        b"RF EP HARD",
        6,
        6,
        [
            struck(op([97, 30, 28, 52], [99, 85, 45, 0], 99, 1, 7, 4), 3),
            struck(op([95, 29, 20, 50], [99, 95, 0, 0], 92, 1, 8, 7), 3),
            struck(op([96, 30, 26, 50], [99, 84, 44, 0], 96, 1, 6, 4), 3),
            struck(
                scaled(
                    op([99, 55, 40, 62], [99, 45, 0, 0], 88, 14, 7, 7),
                    55,
                    0,
                    35,
                    0,
                    0,
                ),
                3,
            ),
            struck(op([95, 29, 20, 50], [99, 95, 0, 0], 84, 1, 9, 6), 3),
            struck(op([99, 55, 42, 64], [99, 30, 0, 0], 88, 3, 7, 5), 3),
        ],
    )
}

/// Algorithm 18: one carrier fed three ways. The stack behind OP4 is the
/// hammer, gone in a tenth of a second; OP2 at 1:1 is the body that stays.
fn piano() -> Voice {
    voice(
        b"RF PIANO  ",
        17,
        5,
        [
            struck(op([96, 30, 24, 45], [99, 88, 60, 0], 99, 1, 7, 3), 3),
            struck(op([97, 40, 28, 50], [99, 70, 20, 0], 94, 1, 9, 6), 3),
            struck(
                scaled(
                    op([98, 50, 32, 55], [99, 55, 0, 0], 84, 2, 6, 7),
                    50,
                    0,
                    40,
                    0,
                    0,
                ),
                3,
            ),
            struck(op([99, 55, 36, 60], [99, 40, 0, 0], 78, 3, 7, 6), 3),
            struck(op([99, 55, 40, 64], [99, 30, 0, 0], 70, 1, 8, 5), 3),
            struck(op([99, 55, 48, 70], [99, 20, 0, 0], 65, 5, 7, 5), 3),
        ],
    )
}

/// Algorithm 3: two three-deep stacks with almost no sustain. Deep stacks
/// with fast envelopes are what give a clavinet its snap.
fn clav() -> Voice {
    voice(
        b"RF CLAV   ",
        2,
        7,
        [
            struck(op([99, 30, 30, 72], [99, 55, 10, 0], 99, 1, 7, 5), 3),
            struck(op([99, 55, 48, 74], [99, 45, 0, 0], 98, 1, 8, 7), 3),
            struck(
                fine(op([99, 55, 52, 76], [99, 35, 0, 0], 99, 4, 6, 7), 50),
                3,
            ),
            struck(op([99, 30, 30, 72], [99, 52, 8, 0], 94, 2, 6, 5), 3),
            struck(op([99, 55, 50, 75], [99, 40, 0, 0], 90, 1, 9, 7), 3),
            struck(op([99, 55, 55, 78], [99, 28, 0, 0], 99, 8, 7, 6), 3),
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
            struck(op([99, 30, 30, 68], [99, 60, 0, 0], 99, 1, 7, 2), 3),
            struck(op([99, 55, 44, 70], [99, 52, 0, 0], 99, 4, 8, 4), 3),
            struck(op([99, 30, 30, 68], [99, 58, 0, 0], 90, 2, 5, 2), 3),
            struck(op([99, 55, 45, 70], [99, 48, 0, 0], 99, 6, 7, 4), 3),
            struck(op([99, 30, 30, 68], [99, 55, 0, 0], 76, 1, 10, 2), 3),
            struck(op([99, 55, 48, 72], [99, 40, 0, 0], 90, 1, 7, 3), 3),
        ],
    )
}

/// Algorithm 8: the feedback sits on OP4, a modulator, so the pluck has a
/// nail in it and the ringing does not.
fn guitar() -> Voice {
    voice(
        b"RF GUITAR ",
        7,
        6,
        [
            struck(op([99, 30, 30, 66], [99, 70, 25, 0], 99, 1, 7, 4), 3),
            struck(op([99, 55, 42, 70], [99, 45, 0, 0], 92, 1, 9, 7), 3),
            struck(op([99, 30, 30, 65], [99, 68, 22, 0], 92, 1, 5, 4), 3),
            struck(op([99, 55, 46, 72], [99, 36, 0, 0], 86, 1, 7, 7), 3),
            struck(op([99, 55, 40, 68], [99, 48, 0, 0], 80, 2, 8, 6), 3),
            struck(op([99, 55, 50, 74], [99, 30, 0, 0], 70, 3, 7, 5), 3),
        ],
    )
}

/// Algorithm 5 with odd ratios and a short upward pitch blip at the pluck.
fn koto() -> Voice {
    pitch_envelope(
        voice(
            b"RF KOTO   ",
            4,
            4,
            [
                struck(op([99, 45, 44, 70], [99, 55, 0, 0], 99, 1, 7, 3), 3),
                struck(op([99, 64, 48, 72], [99, 42, 0, 0], 96, 3, 8, 6), 3),
                struck(op([99, 45, 42, 70], [99, 52, 0, 0], 86, 2, 5, 3), 3),
                struck(op([99, 70, 52, 74], [99, 35, 0, 0], 84, 5, 7, 6), 3),
                struck(op([99, 45, 45, 70], [99, 50, 0, 0], 68, 1, 10, 3), 3),
                struck(op([99, 70, 54, 76], [99, 30, 0, 0], 76, 7, 7, 5), 3),
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
            6,
            [
                struck(op([75, 45, 30, 60], [99, 85, 60, 0], 99, 1, 7, 3), 2),
                struck(op([80, 50, 32, 62], [99, 65, 20, 0], 90, 1, 8, 5), 2),
                struck(op([78, 55, 36, 64], [99, 50, 0, 0], 72, 2, 6, 6), 3),
                struck(op([85, 60, 40, 66], [99, 40, 0, 0], 65, 1, 7, 5), 3),
                struck(op([82, 58, 38, 64], [99, 45, 0, 0], 68, 3, 9, 6), 3),
                struck(op([88, 66, 44, 70], [99, 30, 0, 0], 62, 1, 7, 4), 3),
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
            struck(op([99, 45, 45, 70], [99, 75, 45, 0], 99, 1, 7, 5), 3),
            struck(op([99, 70, 50, 72], [99, 40, 0, 0], 98, 1, 9, 7), 3),
            struck(op([99, 45, 44, 70], [99, 70, 40, 0], 90, 1, 5, 5), 3),
            struck(op([99, 70, 54, 74], [99, 32, 0, 0], 88, 2, 7, 7), 3),
            struck(op([99, 70, 58, 76], [99, 25, 0, 0], 80, 3, 8, 7), 3),
            struck(op([99, 70, 62, 78], [99, 18, 0, 0], 86, 4, 7, 6), 3),
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
            struck(op([99, 50, 40, 68], [99, 96, 95, 0], 99, 1, 7, 2), 1),
            struck(op([99, 50, 40, 68], [99, 96, 95, 0], 98, 0, 7, 2), 1),
            struck(op([99, 60, 45, 70], [99, 60, 30, 0], 66, 2, 7, 4), 3),
            op([99, 99, 99, 99], [99, 99, 99, 0], 0, 1, 7, 0),
            op([99, 99, 99, 99], [99, 99, 99, 0], 0, 1, 7, 0),
            op([99, 99, 99, 99], [99, 99, 99, 0], 0, 1, 7, 0),
        ],
    ))
}

/// Algorithm 22 again, darker and slower: a mild modulator, a sub-octave
/// carrier under the three, a deeper scoop and a vibrato that waits.
fn horns() -> Voice {
    pitch_envelope(
        lfo(
            voice(
                b"RF HORNS  ",
                21,
                6,
                [
                    tremolo(
                        struck(op([64, 40, 40, 66], [99, 96, 96, 0], 99, 1, 7, 1), 2),
                        3,
                    ),
                    op([56, 45, 30, 66], [92, 96, 96, 0], 80, 1, 7, 1),
                    tremolo(
                        struck(op([66, 40, 40, 66], [99, 97, 97, 0], 96, 0, 7, 1), 2),
                        3,
                    ),
                    tremolo(
                        struck(op([66, 40, 40, 66], [99, 97, 97, 0], 99, 1, 5, 1), 2),
                        3,
                    ),
                    tremolo(
                        struck(op([66, 40, 40, 66], [99, 97, 97, 0], 96, 1, 9, 1), 2),
                        3,
                    ),
                    tremolo(
                        scaled(
                            struck(op([46, 60, 30, 66], [96, 96, 94, 0], 82, 1, 7, 2), 3),
                            39,
                            46,
                            50,
                            1,
                            1,
                        ),
                        2,
                    ),
                ],
            ),
            33,
            45,
            5,
            0,
            4,
            3,
        ),
        [84, 70, 99, 99],
        [46, 50, 50, 50],
    )
}

/// Algorithm 18, solo brass: OP1 carries; OP3 at 1:1 with feedback is the
/// sawtooth core and OP4 the body's brightness; OP2 spits at 1:1 and dies,
/// OP5 bites at 3.67:1 under velocity, OP6 flutters at 63 Hz beneath the
/// bite. A scoop into the note and a vibrato that arrives after it.
fn trumpet() -> Voice {
    pitch_envelope(
        lfo(
            voice(
                b"RF TRUMPET",
                17,
                5,
                [
                    tremolo(
                        struck(op([64, 60, 40, 70], [99, 95, 94, 0], 99, 1, 7, 2), 2),
                        3,
                    ),
                    op([90, 60, 20, 70], [95, 0, 0, 0], 40, 1, 7, 5),
                    tremolo(
                        scaled(
                            struck(op([52, 40, 30, 60], [99, 94, 92, 0], 78, 1, 7, 2), 2),
                            39,
                            40,
                            45,
                            1,
                            1,
                        ),
                        2,
                    ),
                    tremolo(op([48, 40, 30, 60], [96, 92, 90, 0], 60, 1, 7, 1), 2),
                    fine(op([88, 70, 40, 70], [99, 0, 0, 0], 38, 3, 7, 6), 67),
                    fixed_hz(op([95, 60, 40, 70], [99, 0, 0, 0], 40, 1, 7, 0), 1, 80),
                ],
            ),
            34,
            40,
            5,
            0,
            4,
            2,
        ),
        [85, 62, 99, 99],
        [44, 50, 50, 50],
    )
}

/// Algorithm 18, the instrument's own reed: one carrier under three
/// half-ratio modulators — one of them the feedback loop — which is where a
/// reed's odd harmonics come from, and a partial near 5.8:1 at the top of
/// the stack for the edge of the tone. Every level holds; the breath
/// controller lifts the carrier and the modulators through the envelope
/// bias, which is what a reed does when it is blown harder.
fn sax() -> Voice {
    pitch_envelope(
        lfo(
            voice(
                b"RF SAX    ",
                17,
                6,
                [
                    tremolo(op([66, 30, 20, 62], [99, 98, 98, 0], 99, 1, 7, 2), 3),
                    op([92, 40, 30, 55], [99, 97, 96, 0], 72, 0, 7, 2),
                    tremolo(op([96, 30, 25, 60], [99, 97, 97, 0], 78, 0, 6, 3), 2),
                    tremolo(op([95, 30, 25, 60], [99, 98, 98, 0], 68, 0, 8, 2), 2),
                    tremolo(
                        fine(op([96, 25, 20, 60], [98, 99, 99, 0], 50, 5, 12, 3), 80),
                        3,
                    ),
                    op([88, 50, 30, 55], [99, 96, 96, 0], 90, 0, 7, 6),
                ],
            ),
            36,
            45,
            10,
            0,
            4,
            3,
        ),
        [90, 70, 99, 99],
        [48, 50, 50, 50],
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
            6,
            [
                tremolo(op([50, 30, 40, 48], [99, 95, 94, 0], 99, 1, 5, 3), 3),
                op([52, 35, 38, 46], [99, 70, 64, 0], 84, 1, 9, 5),
                tremolo(op([48, 28, 40, 48], [99, 95, 94, 0], 98, 1, 10, 3), 3),
                op([50, 34, 38, 46], [99, 66, 60, 0], 84, 1, 6, 5),
                op([52, 36, 38, 46], [99, 60, 50, 0], 72, 2, 8, 4),
                op([54, 38, 38, 46], [99, 50, 40, 0], 66, 3, 7, 4),
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
            4,
            [
                tremolo(op([40, 25, 35, 42], [99, 95, 94, 0], 99, 1, 4, 2), 3),
                tremolo(op([38, 24, 35, 42], [99, 95, 94, 0], 96, 1, 11, 2), 3),
                op([42, 30, 32, 40], [99, 70, 62, 0], 86, 2, 7, 4),
                tremolo(op([36, 22, 34, 40], [99, 95, 94, 0], 92, 1, 7, 2), 3),
                op([44, 32, 32, 40], [99, 64, 56, 0], 76, 3, 6, 4),
                op([46, 34, 32, 40], [99, 58, 48, 0], 70, 1, 9, 4),
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
            5,
            [
                tremolo(op([45, 30, 40, 44], [99, 95, 94, 0], 96, 1, 7, 2), 1),
                tremolo(op([44, 30, 40, 44], [99, 95, 94, 0], 90, 2, 9, 2), 1),
                op([46, 30, 40, 44], [99, 95, 94, 0], 82, 3, 5, 2),
                op([42, 28, 40, 44], [99, 95, 94, 0], 86, 1, 10, 2),
                op([43, 28, 40, 44], [99, 95, 94, 0], 76, 4, 6, 2),
                op([50, 35, 36, 42], [99, 60, 52, 0], 74, 1, 8, 4),
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
        6,
        [
            tremolo(op([70, 40, 45, 60], [99, 95, 94, 0], 99, 1, 7, 1), 2),
            op([68, 40, 45, 60], [99, 95, 94, 0], 88, 2, 7, 1),
            tremolo(op([66, 40, 45, 60], [99, 94, 94, 0], 72, 4, 7, 1), 2),
            op([72, 40, 45, 60], [99, 94, 94, 0], 62, 0, 7, 1),
            tremolo(op([95, 60, 45, 60], [99, 95, 94, 0], 76, 3, 7, 2), 2),
            struck(op([99, 70, 50, 65], [99, 30, 0, 0], 92, 5, 7, 3), 4),
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
                tremolo(op([99, 50, 45, 60], [99, 95, 96, 0], 99, 1, 7, 4), 2),
                op([99, 55, 45, 60], [99, 80, 75, 0], 99, 1, 8, 6),
                tremolo(op([99, 50, 45, 60], [99, 94, 96, 0], 96, 1, 6, 4), 2),
                tremolo(op([99, 50, 45, 60], [99, 94, 96, 0], 93, 1, 8, 4), 2),
                tremolo(op([99, 50, 45, 60], [99, 86, 84, 0], 86, 2, 7, 3), 2),
                op([99, 55, 45, 60], [99, 78, 72, 0], 92, 1, 7, 6),
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
            7,
            [
                tremolo(op([99, 45, 45, 58], [99, 96, 96, 0], 99, 1, 7, 3), 2),
                op([99, 50, 45, 58], [99, 82, 78, 0], 98, 2, 7, 5),
                tremolo(op([99, 45, 45, 58], [99, 94, 95, 0], 66, 1, 9, 3), 2),
                op([99, 50, 45, 58], [99, 60, 55, 0], 70, 2, 7, 5),
                op([99, 55, 45, 58], [99, 40, 30, 0], 50, 1, 7, 4),
                op([99, 60, 45, 58], [99, 30, 20, 0], 50, 1, 7, 4),
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
                    struck(op([99, 30, 20, 45], [99, 85, 50, 0], 99, 1, 7, 3), 3),
                    2,
                ),
                struck(op([99, 40, 25, 50], [99, 60, 10, 0], 78, 4, 7, 6), 3),
                tremolo(
                    struck(op([99, 28, 20, 44], [99, 82, 48, 0], 86, 1, 8, 3), 3),
                    2,
                ),
                struck(op([99, 45, 28, 52], [99, 50, 0, 0], 68, 1, 7, 5), 3),
                tremolo(
                    struck(op([99, 30, 22, 46], [99, 80, 44, 0], 66, 2, 6, 3), 3),
                    2,
                ),
                struck(op([99, 50, 30, 54], [99, 40, 0, 0], 58, 7, 7, 5), 3),
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
        6,
        [
            struck(op([99, 22, 16, 40], [99, 80, 40, 0], 99, 1, 7, 3), 2),
            struck(
                fine(op([99, 28, 20, 44], [99, 70, 25, 0], 82, 3, 8, 4), 50),
                3,
            ),
            struck(op([99, 20, 15, 38], [99, 78, 38, 0], 92, 1, 5, 3), 2),
            struck(
                fine(op([99, 26, 18, 42], [99, 66, 20, 0], 74, 2, 6, 4), 41),
                3,
            ),
            struck(op([99, 24, 18, 40], [99, 74, 34, 0], 78, 1, 10, 3), 2),
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
        6,
        [
            struck(op([99, 45, 32, 60], [99, 66, 20, 0], 99, 1, 7, 4), 3),
            struck(
                fine(op([99, 55, 38, 64], [99, 50, 0, 0], 92, 1, 8, 6), 41),
                3,
            ),
            struck(
                fine(op([99, 65, 44, 68], [99, 38, 0, 0], 78, 2, 7, 6), 88),
                3,
            ),
            struck(op([99, 42, 30, 58], [99, 64, 18, 0], 90, 2, 5, 4), 3),
            struck(op([99, 45, 34, 60], [99, 60, 16, 0], 80, 3, 10, 4), 3),
            struck(
                fine(op([99, 60, 40, 66], [99, 44, 0, 0], 86, 1, 7, 6), 57),
                3,
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
            7,
            [
                struck(op([99, 40, 28, 55], [99, 70, 20, 0], 99, 1, 7, 4), 3),
                struck(
                    fine(op([99, 55, 36, 60], [99, 45, 0, 0], 88, 1, 8, 6), 50),
                    3,
                ),
                struck(
                    fine(op([99, 62, 40, 62], [99, 38, 0, 0], 76, 2, 6, 6), 30),
                    3,
                ),
                struck(op([99, 70, 46, 66], [99, 30, 0, 0], 68, 3, 7, 5), 3),
                struck(op([99, 66, 42, 64], [99, 34, 0, 0], 72, 1, 9, 6), 3),
                struck(op([99, 70, 52, 70], [99, 22, 0, 0], 80, 0, 7, 5), 3),
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
