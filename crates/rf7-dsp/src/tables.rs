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
/// Unity gain. 127 scaled output-level steps of 32 units each. This is the
/// level of an operator at output level 99 with velocity sensitivity 0 — the
/// firmware's reference, fifteen sixteenths of an octave under the hardware.
pub const LEVEL_FULL: f32 = 4064.0;
/// How far above [`LEVEL_FULL`] a level may go. The firmware clamps every
/// operator's attenuation byte at 4 sixteenths, and the reference sits at 15,
/// so a velocity-sensitive operator played hard has eleven sixteenths of an
/// octave in hand above the reference. That is the instrument's headroom,
/// and a note that uses all of it on every carrier can leave a voice above
/// full scale — which is left visible rather than normalised away.
pub const LEVEL_HEADROOM: f32 = 11.0 * LEVEL_UNITS_PER_OCTAVE / 16.0;
/// Below this the operator is inaudible and is treated as silent outright.
///
/// Envelope level 0 is 127 scaled steps under the operator's ceiling, and a
/// velocity-sensitive operator's ceiling can sit up to [`LEVEL_HEADROOM`]
/// above the reference — so its level 0 lands that far above zero units.
/// That is ninety decibels below the reference, which is silence by any
/// measure, and a voice that waited for exactly zero would never be freed.
pub const LEVEL_SILENT: f32 = LEVEL_HEADROOM;

/// Level units per second at the slowest quantised rate: on the hardware the
/// level moves one step every 4096 samples of its 49096 Hz clock, which is
/// 0.28 dB a second, so a full decay at rate 0 takes over five minutes.
const RATE_BASE_UNITS: f32 = 49_096.0 / 4096.0;
/// The hardware's full scale, in level units, which is what the attack
/// measures its distance from.
const ATTACK_FULL_SCALE: f32 = 4095.0;
/// A rising segment that starts under this many units above the operator's
/// floor jumps to it first: the hardware skips the inaudible bottom forty
/// decibels of an attack, which is what makes its attacks crisp.
pub const ATTACK_JUMP: f32 = 1700.0;
/// One sixteenth of an octave, which is the unit the firmware's velocity
/// term is counted in, expressed in level units.
const SIXTEENTH_OCTAVE: f32 = LEVEL_UNITS_PER_OCTAVE / 16.0;
/// The velocity term at sensitivity 0, which the firmware applies to every
/// operator regardless of velocity. It is the reference the others are
/// measured from, and it is where the modulation index gets its 2^(−15/16).
const VELOCITY_REFERENCE: i32 = 15;
/// The firmware adds this to a break point before looking it up on the same
/// key scale a played note is on: break point 0 is A-1, the twenty-first key
/// from the bottom of its table.
const BREAK_POINT_OFFSET: i32 = 20;
/// The firmware's two keyboard scaling curves, verbatim: the distance from
/// the break point in groups of three semitones indexes them, and the value
/// is multiplied by the depth. The linear curve is not quite linear — its
/// twenty-third entry is 0xB2 rather than 0xB0 — and both saturate.
const SCALING_CURVE_EXP: [u8; 36] = [
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0B, 0x0E, 0x10, 0x13, 0x17, 0x1C,
    0x21, 0x27, 0x2F, 0x39, 0x43, 0x50, 0x5F, 0x71, 0x86, 0xA0, 0xBE, 0xE0, 0xFF, 0xFF, 0xFF, 0xFF,
    0xFF, 0xFF, 0xFF, 0xFF,
];
const SCALING_CURVE_LIN: [u8; 36] = [
    0x00, 0x08, 0x10, 0x18, 0x20, 0x28, 0x30, 0x38, 0x40, 0x48, 0x50, 0x58, 0x60, 0x68, 0x70, 0x78,
    0x80, 0x88, 0x90, 0x98, 0xA0, 0xA8, 0xB2, 0xB8, 0xC0, 0xC8, 0xD0, 0xD8, 0xE0, 0xE8, 0xF0, 0xF8,
    0xFF, 0xFF, 0xFF, 0xFF,
];
/// Detune, measured rather than read. The firmware only hands the EGS a
/// sign and a magnitude, and the chip applies it, so no table says how far
/// a step goes. The Dexed project's author measured a DX7 and fitted one
/// step to `0.0209 / 7 × log2(f) × e^(−0.396 × log2(f))` octaves at a key
/// of `f` hertz: 2.6 cents at A0, 1.2 at middle C, half a cent at C7. The
/// beat between two detuned operators therefore grows with pitch, but more
/// slowly than the pitch does — it is neither a fixed interval nor a fixed
/// number of hertz.
const DETUNE_FIT_SCALE: f64 = 0.0209 / 7.0;
const DETUNE_FIT_DECAY: f64 = 0.396;
/// The firmware's pitch envelope level table, verbatim: a level 0..=99 to a
/// byte whose top seven bits sit in the voice's pitch word. 128 is the
/// centre; the table is one step a level through the middle and steepens at
/// the ends, so 99 is four octaves up and 0 four octaves down.
const PITCH_EG_LEVEL: [u8; 100] = [
    0x00, 0x0C, 0x18, 0x21, 0x2B, 0x34, 0x3C, 0x43, 0x48, 0x4C, 0x4F, 0x52, 0x55, 0x57, 0x59, 0x5B,
    0x5D, 0x5F, 0x60, 0x61, 0x62, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6A, 0x6B, 0x6C, 0x6D,
    0x6E, 0x6F, 0x70, 0x71, 0x72, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7A, 0x7B, 0x7C, 0x7D,
    0x7E, 0x7F, 0x80, 0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8A, 0x8B, 0x8C, 0x8D,
    0x8E, 0x8F, 0x90, 0x91, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9A, 0x9B, 0x9C, 0x9D,
    0x9E, 0x9F, 0xA0, 0xA1, 0xA2, 0xA3, 0xA6, 0xA8, 0xAB, 0xAE, 0xB1, 0xB5, 0xBA, 0xC1, 0xC9, 0xD2,
    0xDC, 0xE7, 0xF3, 0xFF,
];
/// The pitch envelope's centre in that table.
const PITCH_EG_CENTRE: f32 = 128.0;
/// One step of the level table, in semitones: the byte is shifted seven
/// places into a pitch word that counts 4096 to the octave, so a step is
/// 128 / 4096 of an octave.
const PITCH_EG_STEP_SEMITONES: f32 = 12.0 * 128.0 / 4096.0;
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

/// Which of the firmware's forty-three scaling groups a key falls in.
///
/// The firmware indexes its curve with the top byte of the key's logarithmic
/// pitch shifted down two places, which makes a group of every three keys
/// with the boundaries where its pitch table puts them: keys 1, 2 and 3 are
/// group 0, keys 4 to 6 group 1, and so on.
fn scaling_group(key: i32) -> i32 {
    (key.clamp(0, 127) - 1).max(0) / 3
}

/// Keyboard level scaling for one operator at one key, on the 0..=127 scale.
///
/// `key` is the key the firmware would see: the note played plus the voice's
/// transpose, because the instrument scales the pitch it sounds rather than
/// the key that was pressed. Positive above the break point when the right
/// curve is a positive one, and so on for the other three combinations. The
/// result is added to the scaled output level before it is clamped, which is
/// where the firmware adds it to the operator's logarithmic level.
pub fn key_level_offset(key: i32, operator: &Operator) -> i32 {
    let distance = scaling_group(key)
        - scaling_group(i32::from(operator.break_point.min(99)) + BREAK_POINT_OFFSET);
    if distance > 0 {
        signed_curve(distance, operator.right_depth, operator.right())
    } else {
        signed_curve(-distance, operator.left_depth, operator.left())
    }
}

/// The firmware's `curve[distance] × scale(depth)`, where `scale(depth)` is
/// the top byte of `depth × 660` — 255 at depth 99 — and the product keeps
/// its top byte, clamped to 127.
fn signed_curve(distance: i32, depth: u8, curve: Curve) -> i32 {
    let table = if curve.is_exponential() {
        &SCALING_CURVE_EXP
    } else {
        &SCALING_CURVE_LIN
    };
    let index = distance.clamp(0, table.len() as i32 - 1) as usize;
    let depth = (i32::from(depth.min(99)) * 660) >> 8;
    let magnitude = ((i32::from(table[index]) * depth) >> 8).min(127);
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

/// Level units per second at a quantised rate. Four steps double the speed,
/// and the steps between are linear — the hardware's `(1 + (q mod 4) / 4) ×
/// 2^(q div 4)` — so a decay is a straight line in decibels.
pub fn rate_units_per_second(quantised: i32) -> f32 {
    let quantised = quantised.clamp(0, 63);
    RATE_BASE_UNITS * ((quantised / 4) as f32).exp2() * (1.0 + 0.25 * (quantised % 4) as f32)
}

/// The step a rising segment takes: the decay's step, multiplied by how far
/// the level still is from full scale, in 256-unit bands, plus two. That is
/// the hardware's attack — fast from the bottom, slowing towards the top,
/// which reads as roughly linear in decibels.
pub fn rising_step(level: f32, units: f32) -> f32 {
    units * (2.0 + ((ATTACK_FULL_SCALE - level) / 256.0).floor()).max(1.0)
}

/// The modulation index, in radians, that an operator at this output level
/// produces in the operator it feeds — at full envelope, velocity
/// sensitivity 0, no keyboard scaling. What a patch designer is choosing
/// when they set a modulator's level.
pub fn modulation_index_at_level(level: u8) -> f32 {
    core::f32::consts::TAU
        * crate::MODULATION_CYCLES
        * level_gain(scale_output_level(level) as f32 * 32.0)
}

/// Level units to linear gain.
pub fn level_gain(units: f32) -> f32 {
    if units <= LEVEL_SILENT {
        return 0.0;
    }
    ((units.min(LEVEL_FULL + LEVEL_HEADROOM) - LEVEL_FULL) / LEVEL_UNITS_PER_OCTAVE).exp2()
}

/// Level units a key's velocity adds or removes, as the DX7's firmware does it.
///
/// Two tables from the v1.8 ROM. `MIDI_VELOCITY` turns the MIDI value into
/// the instrument's own, which runs the other way: 127 becomes 0. That then
/// indexes `VELOCITY_SCALE`, and the firmware computes, in sixteenths of an
/// octave of attenuation,
///
/// ```text
/// ((sensitivity × 32 × VELOCITY_SCALE[v]) >> 8) + (15 − 2 × sensitivity)
/// ```
///
/// At sensitivity 0 that is the constant 15 for every velocity, so velocity
/// does nothing — and that constant is taken as the zero here, because it is
/// already inside [`crate::MODULATION_CYCLES`]. A sensitive operator played
/// hard comes out *above* it: at sensitivity 7 and velocity 127 the term is 1,
/// fourteen sixteenths — very nearly an octave — over the reference. That is
/// the instrument, not a rounding: sensitivity buys more level at the top as
/// well as much less at the bottom, and [`LEVEL_HEADROOM`] is what lets the
/// top through.
pub fn velocity_offset(velocity: u8, sensitivity: u8) -> f32 {
    const MIDI_VELOCITY: [u8; 32] = [
        0x6e, 0x64, 0x5a, 0x55, 0x50, 0x4b, 0x46, 0x41, 0x3a, 0x36, 0x32, 0x2e, 0x2a, 0x26, 0x22,
        0x1e, 0x1c, 0x1a, 0x18, 0x16, 0x14, 0x12, 0x10, 0x0e, 0x0c, 0x0a, 0x08, 0x06, 0x04, 0x02,
        0x01, 0x00,
    ];
    const VELOCITY_SCALE: [u8; 32] = [
        0x00, 0x04, 0x0c, 0x15, 0x1e, 0x28, 0x2e, 0x34, 0x3a, 0x40, 0x46, 0x4c, 0x52, 0x58, 0x5e,
        0x64, 0x67, 0x6a, 0x6d, 0x70, 0x72, 0x74, 0x76, 0x78, 0x7a, 0x7c, 0x7e, 0x80, 0x82, 0x83,
        0x84, 0x85,
    ];
    let sensitivity = i32::from(sensitivity.min(7));
    let internal = MIDI_VELOCITY[usize::from(velocity.min(127) >> 2)];
    let scale = i32::from(VELOCITY_SCALE[usize::from(internal >> 2)]);
    let attenuation = ((sensitivity * 32 * scale) >> 8) + (15 - 2 * sensitivity);
    (VELOCITY_REFERENCE - attenuation) as f32 * SIXTEENTH_OCTAVE
}

/// The frequency ratio of a key-tracking operator, before its detune.
///
/// Coarse 0 is the half ratio, not silence; fine adds hundredths of the coarse
/// ratio. Detune is applied by [`detune_factor`], because how far a step goes
/// depends on the key.
pub fn operator_ratio(operator: &Operator) -> f64 {
    let coarse = if operator.coarse == 0 {
        0.5
    } else {
        f64::from(operator.coarse.min(31))
    };
    let fine = 1.0 + f64::from(operator.fine.min(99)) / 100.0;
    coarse * fine
}

/// The frequency of a fixed operator, in hertz, before its detune. Four
/// decades from 1 Hz.
pub fn fixed_frequency(operator: &Operator) -> f64 {
    let decade = f64::from(operator.coarse & 3);
    let fraction = f64::from(operator.fine.min(99)) / 100.0;
    10f64.powf(decade + fraction)
}

/// How far a detune of 0..=14 (7 is none) moves an operator, in octaves,
/// when the key sounding is `key_hertz`. A fixed operator passes its own
/// frequency, which is the nearest thing it has to a key.
pub fn detune_octaves(detune: u8, key_hertz: f64) -> f64 {
    let log2 = key_hertz.max(1.0).log2();
    (f64::from(detune.min(14)) - 7.0) * DETUNE_FIT_SCALE * log2 * (-DETUNE_FIT_DECAY * log2).exp()
}

/// [`detune_octaves`] as a factor on the frequency.
pub fn detune_factor(detune: u8, key_hertz: f64) -> f64 {
    detune_octaves(detune, key_hertz).exp2()
}

/// The firmware's LFO phase increment for a speed 0..=99: added to a
/// sixteen-bit phase word on every timer tick.
///
/// `PATCH_ACTIVATE_SCALE_LFO_SPEED`: the speed scaled to 0..=255 (the top
/// byte of `speed × 660`), multiplied by 11 — and, from 160 up, by
/// `11 + (scaled − 160) / 4`, which is what bends the top of the dial
/// upwards. Speed 0 is treated as 1 so the arithmetic never stalls.
pub fn lfo_phase_increment(speed: u8) -> u32 {
    let scaled = if speed == 0 {
        1
    } else {
        (u32::from(speed.min(99)) * 660) >> 8
    };
    let multiplier = if scaled >= 160 {
        11 + (scaled - 160) / 4
    } else {
        11
    };
    scaled * multiplier
}

/// LFO speed 0..=99 to hertz: the phase increment over the word's 65536,
/// at the timer's rate.
pub fn lfo_hertz(speed: u8) -> f32 {
    lfo_phase_increment(speed) as f32 * TIMER_TICKS_PER_SECOND / 65536.0
}

/// The firmware's LFO delay increment for a delay 0..=99: added to a
/// sixteen-bit delay word on every tick until it overflows, which is when
/// the fade-in begins.
///
/// `PATCH_ACTIVATE_SCALE_LFO_DELAY`: with `x = 99 − delay`, the increment is
/// `(16 + (x mod 16)) << (2 + x / 16)` — a mantissa and an exponent, so the
/// delay doubles every sixteen steps of the dial.
pub fn lfo_delay_increment(delay: u8) -> u32 {
    let x = 99 - u32::from(delay.min(99));
    (16 + (x & 15)) << (2 + (x >> 4))
}

/// LFO delay 0..=99 to the seconds before the fade-in begins. Even delay 0
/// waits a few dozen milliseconds, as the instrument does.
pub fn lfo_delay_seconds(delay: u8) -> f32 {
    65536.0 / lfo_delay_increment(delay) as f32 / TIMER_TICKS_PER_SECOND
}

/// The seconds the fade-in takes once the delay is over: the fade counter
/// climbs to 255 by the top byte of the delay increment — at least one — on
/// every tick, so a long delay brings a fade about as long.
pub fn lfo_fade_seconds(delay: u8) -> f32 {
    255.0 / (lfo_delay_increment(delay) >> 8).max(1) as f32 / TIMER_TICKS_PER_SECOND
}

/// The firmware's pitch rate table, `TABLE_PITCH_EG_RATE`, indexed 0..=99.
///
/// It quantises a panel rate to the increment the hardware adds per update.
/// The portamento reads it backwards — `[99 - time]` — so a time of 0 gives
/// 255, which crosses any interval within one update, and a time of 99 gives
/// 1, the slowest glide the instrument can make.
const PITCH_RATE: [u8; 100] = [
    1, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13, 14, 14, 15, 16,
    16, 17, 18, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 30, 31, 33, 34, 36, 37, 38, 39, 41, 42,
    44, 46, 47, 49, 51, 53, 54, 56, 58, 60, 62, 64, 66, 68, 70, 72, 74, 76, 79, 82, 85, 88, 91, 94,
    98, 102, 106, 110, 115, 120, 125, 130, 135, 141, 147, 153, 159, 165, 171, 178, 185, 193, 202,
    211, 232, 243, 254, 255,
];

/// The instrument's pitch is logarithmic with 1024 units to the octave, which
/// is what `VOICE_CONVERT_NOTE_TO_LOG_FREQ` documents and what the portamento
/// increment is counted in.
const PITCH_UNITS_PER_OCTAVE: f32 = 1024.0;

/// How often the hardware moves a gliding voice, in updates per second.
///
/// The firmware's periodic timer: `SYSTEM_TICK_PERIOD` is 3140 counts,
/// which is about 374 a second on the instrument's clock. The LFO advances
/// on every tick; `PORTA_PROCESS` takes half the sixteen voices each tick
/// and `PITCH_EG_PROCESS` runs every second tick over all of them, so both
/// move a voice's pitch every second tick. The tables are the firmware's;
/// this number is the one part of the timing that is inferred rather than
/// read, and it is what a recording would settle. The LFO's top speed is
/// the check on it: the literature's 47 Hz at speed 99 would make the tick
/// 355, and this makes it 49.5.
const TIMER_TICKS_PER_SECOND: f32 = 374.0;
const PITCH_UPDATES_PER_SECOND: f32 = TIMER_TICKS_PER_SECOND / 2.0;

/// Portamento time 0..=99 to the semitones a glide covers each second.
///
/// The firmware's step is `((distance >> 10) + 1) * rate` in pitch units, so
/// within an octave the glide runs at a constant rate — a straight line in
/// the logarithmic pitch domain — and speeds up by one whole step for every
/// further octave. [`portamento_octave_boost`] is that multiplier.
pub fn portamento_semitones_per_second(time: u8) -> f32 {
    let rate = f32::from(PITCH_RATE[usize::from(99 - time.min(99))]);
    rate * PITCH_UPDATES_PER_SECOND / PITCH_UNITS_PER_OCTAVE * 12.0
}

/// The firmware's `(distance >> 10) + 1`: one extra step of speed for every
/// octave still to cross.
pub fn portamento_octave_boost(semitones: f32) -> f32 {
    (semitones.abs() / 12.0).floor() + 1.0
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
    (f32::from(PITCH_EG_LEVEL[usize::from(level.min(99))]) - PITCH_EG_CENTRE)
        * PITCH_EG_STEP_SEMITONES
}

/// Pitch envelope rate 0..=99 to the semitones a segment moves each second.
///
/// `PITCH_EG_PROCESS` adds the rate table's entry to the voice's pitch word
/// — 4096 to the octave — on every second timer tick, and a segment ends
/// when it reaches or crosses its level. So a segment is a straight line in
/// the logarithmic pitch domain, at the same speed whatever its distance.
pub fn pitch_eg_semitones_per_second(rate: u8) -> f32 {
    f32::from(PITCH_RATE[usize::from(rate.min(99))]) * PITCH_UPDATES_PER_SECOND
        / PITCH_WORD_PER_OCTAVE
        * 12.0
}

/// What the pitch envelope's word counts to the octave: the frequency's
/// top byte steps sixteen times an octave, shifted up eight.
const PITCH_WORD_PER_OCTAVE: f32 = 4096.0;

#[cfg(test)]
mod tests {
    use super::*;
    use rf7_voice::Voice;

    #[test]
    fn the_pitch_envelope_reads_the_firmwares_level_and_rate_tables() {
        // Level 50 is the centre, 99 is four octaves up less a step, 0 is
        // four octaves down, and through the middle a level is 3/8 of a
        // semitone.
        assert_eq!(pitch_eg_semitones(50), 0.0);
        assert!((pitch_eg_semitones(99) - 127.0 * 0.375).abs() < 1e-4);
        assert!((pitch_eg_semitones(0) + 48.0).abs() < 1e-4);
        assert!((pitch_eg_semitones(40) + 3.75).abs() < 1e-4);
        assert!((pitch_eg_semitones(60) - 3.75).abs() < 1e-4);
        // The rate table's entry, 187 times a second, over 4096 an octave.
        assert!((pitch_eg_semitones_per_second(99) - 255.0 * 187.0 / 4096.0 * 12.0).abs() < 1e-3);
        assert!((pitch_eg_semitones_per_second(0) - 187.0 / 4096.0 * 12.0).abs() < 1e-3);
        assert!(pitch_eg_semitones_per_second(50) < pitch_eg_semitones_per_second(51));
    }

    #[test]
    fn the_level_scale_spans_ninety_six_decibels() {
        assert_eq!(scale_output_level(0), 0);
        assert_eq!(scale_output_level(99), 127);
        assert_eq!(scale_output_level(200), 127);
        assert_eq!(level_gain(LEVEL_FULL), 1.0);
        assert_eq!(level_gain(0.0), 0.0);
        // Above the reference there is the firmware's eleven sixteenths of
        // headroom, and no more.
        let ceiling = (11.0f32 / 16.0).exp2();
        assert!((level_gain(LEVEL_FULL + LEVEL_HEADROOM) - ceiling).abs() < 1e-5);
        assert!((level_gain(LEVEL_FULL + 1000.0) - ceiling).abs() < 1e-5);
        let half = level_gain(LEVEL_FULL - LEVEL_UNITS_PER_OCTAVE);
        assert!((half - 0.5).abs() < 1e-6);
    }

    #[test]
    fn the_velocity_curve_is_the_firmwares() {
        // Sensitivity 0: the constant 15, so nothing moves with velocity.
        for velocity in 0..=127 {
            assert_eq!(velocity_offset(velocity, 0), 0.0);
        }
        for sensitivity in 1..=7 {
            // Never louder for a softer key.
            for velocity in 1..=127u8 {
                assert!(
                    velocity_offset(velocity - 1, sensitivity)
                        <= velocity_offset(velocity, sensitivity)
                );
            }
            assert!(velocity_offset(0, sensitivity) < 0.0);
        }
        // Sensitivity 7 at full velocity: the MIDI table gives 0, the scale
        // table gives 0, so the term is 15 − 14 = 1: fourteen sixteenths
        // above the reference. Three of those are beyond the headroom.
        assert_eq!(velocity_offset(127, 7), 14.0 * SIXTEENTH_OCTAVE);
        assert!(velocity_offset(127, 7) > LEVEL_HEADROOM);
        // And at velocity 0: index 0x6e>>2 = 27, scale 0x80: (7·32·128)>>8 + 1
        // = 113, ninety-eight sixteenths below it — six octaves and a bit.
        assert_eq!(velocity_offset(0, 7), -98.0 * SIXTEENTH_OCTAVE);
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
        operator.break_point = 39; // C3, which is MIDI 60.
        operator.left_depth = 99;
        operator.right_depth = 99;
        operator.left_curve = 0; // -LIN
        operator.right_curve = 3; // +LIN
        // The break point's own group of three keys is neutral, and the
        // group above it is the first step of the right curve.
        for key in 58..=60 {
            assert_eq!(key_level_offset(key, &operator), 0, "key {key}");
        }
        assert_eq!(key_level_offset(61, &operator), (8 * 255) >> 8);
        assert_eq!(key_level_offset(57, &operator), -((8 * 255) >> 8));
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
    fn key_scaling_is_the_firmwares_curve_times_its_depth() {
        // From the firmware: linear is eight a group, exponential follows
        // its own table, the depth scales to 255 at 99 and 128 at 50, the
        // product keeps its top byte and never exceeds 127.
        let mut operator = Voice::init().operators[0];
        operator.break_point = 39;
        operator.right_curve = 3; // +LIN
        operator.right_depth = 99;
        assert_eq!(key_level_offset(72, &operator), (32 * 255) >> 8); // four groups
        operator.right_depth = 50;
        assert_eq!(
            key_level_offset(72, &operator),
            (32 * ((50 * 660) >> 8)) >> 8
        );
        operator.right_curve = 2; // +EXP
        operator.right_depth = 99;
        assert_eq!(key_level_offset(72, &operator), (4 * 255) >> 8);
        assert_eq!(key_level_offset(90, &operator), (0x0B * 255) >> 8); // ten groups
        // Twenty-three groups up: the exponential table has not saturated yet.
        assert_eq!(key_level_offset(127, &operator), (0x71 * 255) >> 8);
        // Four octaves of full linear depth is the whole dial.
        operator.right_curve = 3;
        assert_eq!(key_level_offset(108, &operator), 127);
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
        // The ratio is the panel's alone; detune is applied at the key.
        operator.fine = 0;
        operator.detune = 14;
        assert!((operator_ratio(&operator) - 14.0).abs() < 1e-9);
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
    fn detune_is_the_measured_curve_and_shrinks_with_pitch() {
        let cents = |detune: u8, hertz: f64| detune_octaves(detune, hertz) * 1200.0;
        assert_eq!(cents(7, 261.63), 0.0);
        // One step: about 2.6 cents at A0, 1.2 at middle C, half at C7.
        assert!((cents(8, 27.5) - 2.6).abs() < 0.2, "{}", cents(8, 27.5));
        assert!((cents(8, 261.63) - 1.2).abs() < 0.1, "{}", cents(8, 261.63));
        assert!((cents(8, 2093.0) - 0.5).abs() < 0.1, "{}", cents(8, 2093.0));
        // Symmetric about the centre, and seven steps each way.
        assert_eq!(cents(6, 440.0), -cents(8, 440.0));
        assert!((cents(14, 440.0) - 7.0 * cents(8, 440.0)).abs() < 1e-9);
        assert!((detune_factor(8, 440.0) * detune_factor(6, 440.0) - 1.0).abs() < 1e-12);
        // The beat in hertz still grows with pitch, just more slowly.
        let beat = |hertz: f64| hertz * (detune_factor(8, hertz) - 1.0);
        assert!(beat(261.63) > beat(27.5) && beat(2093.0) > beat(261.63));
        assert!(
            beat(2093.0) < beat(27.5) * 76.0,
            "not proportional to pitch"
        );
    }

    #[test]
    fn the_modulation_dials_are_centred_and_bounded() {
        assert_eq!(pitch_eg_semitones(50), 0.0);
        assert!(pitch_eg_semitones(99) > 40.0);
        assert!(pitch_eg_semitones(0) < -40.0);
        assert!((pitch_eg_semitones(55) - 1.875).abs() < 1e-4);
        assert_eq!(pitch_mod_semitones(0), 0.0);
        assert!(pitch_mod_semitones(7) > pitch_mod_semitones(3));
        assert_eq!(amp_mod_units(0), 0.0);
        assert!(amp_mod_units(3) > amp_mod_units(1));
        assert!(lfo_hertz(0) < 0.1 && lfo_hertz(99) > 40.0);
        assert!(lfo_hertz(50) > lfo_hertz(49));
        assert!(lfo_delay_seconds(0) < 0.05);
        assert!(lfo_delay_seconds(99) > 2.5);
    }

    #[test]
    fn the_lfo_runs_on_the_firmwares_own_arithmetic() {
        // Speed: 11 a tick at 0 and 1, then the scaled speed times 11, and
        // from a scaled 160 up the multiplier climbs too.
        assert_eq!(lfo_phase_increment(0), 11);
        assert_eq!(lfo_phase_increment(1), 22);
        assert_eq!(lfo_phase_increment(50), ((50 * 660) >> 8) * 11);
        assert_eq!(lfo_phase_increment(99), 255 * (11 + (255 - 160) / 4));
        assert!((lfo_hertz(99) - 8670.0 * 374.0 / 65536.0).abs() < 1e-3);
        for speed in 1..99 {
            assert!(lfo_hertz(speed + 1) >= lfo_hertz(speed), "speed {speed}");
        }
        // Delay: a mantissa and an exponent, doubling every sixteen steps.
        assert_eq!(lfo_delay_increment(99), 16 << 2);
        assert_eq!(lfo_delay_increment(0), (16 + 3) << 8);
        assert_eq!(lfo_delay_increment(50), 17 << 5);
        assert!((lfo_delay_seconds(99) - 65536.0 / 64.0 / 374.0).abs() < 1e-3);
        assert!((lfo_fade_seconds(99) - 255.0 / 374.0).abs() < 1e-3);
        for delay in 0..99 {
            assert!(
                lfo_delay_seconds(delay + 1) >= lfo_delay_seconds(delay),
                "delay {delay}"
            );
        }
    }

    #[test]
    fn the_portamento_runs_from_the_firmwares_own_rate_table() {
        // Time 0 crosses an octave inside a single update, which is what the
        // instrument means by an instant switch.
        assert!(portamento_semitones_per_second(0) > 12.0 * 187.0 / 12.0);
        // The slowest glide takes several seconds to the octave.
        let slowest = portamento_semitones_per_second(99);
        assert!(
            (12.0 / slowest - 5.5).abs() < 0.2,
            "{slowest} semitones a second"
        );
        // Monotonic: more time is always slower.
        for time in 1..=99u8 {
            assert!(
                portamento_semitones_per_second(time) <= portamento_semitones_per_second(time - 1),
                "time {time} is not slower than {}",
                time - 1
            );
        }
        assert_eq!(portamento_octave_boost(0.0), 1.0);
        assert_eq!(portamento_octave_boost(-11.9), 1.0);
        assert_eq!(portamento_octave_boost(12.0), 2.0);
        assert_eq!(portamento_octave_boost(-25.0), 3.0);
    }
}
