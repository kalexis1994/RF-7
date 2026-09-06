//! The four-segment envelope, in the level domain and in the pitch domain.
//!
//! A DX7 envelope is not attack/decay/sustain/release. It is four rates and
//! four levels, each level reached at its own rate, with the third level held
//! while the key is down and the fourth taken on release. An envelope whose
//! fourth level is not silence keeps sounding after the key is up, and the
//! engine treats that as the instrument does: the voice stays busy.

use crate::tables::{
    ATTACK_JUMP, LEVEL_FULL, LEVEL_HEADROOM, LEVEL_SILENT, key_rate_offset, level_gain,
    quantised_rate, rate_units_per_second, rising_step, scale_output_level,
};
use rf7_voice::Operator;

const SEGMENTS: usize = 4;
const SUSTAIN: usize = 2;
const RELEASE: usize = 3;

/// One envelope, in whatever unit its targets were configured with.
#[derive(Clone, Copy, Debug)]
pub struct Envelope {
    level: f32,
    targets: [f32; SEGMENTS],
    /// Units per sample, already divided by the sample rate.
    steps: [f32; SEGMENTS],
    segment: usize,
    /// Rising segments take the hardware's attack curve and skip its bottom;
    /// only the level domain wants that.
    curved: bool,
    /// Where envelope level 0 sits for this operator, in level units.
    floor: f32,
    released: bool,
    settled: bool,
}

impl Default for Envelope {
    fn default() -> Self {
        Self {
            level: 0.0,
            targets: [0.0; SEGMENTS],
            steps: [0.0; SEGMENTS],
            segment: RELEASE,
            curved: false,
            floor: 0.0,
            released: true,
            settled: true,
        }
    }
}

impl Envelope {
    /// One operator's amplitude envelope at one key.
    ///
    /// `ceiling` is where envelope level 99 lands, in level units. The caller
    /// has already folded the programmed output level, keyboard level scaling
    /// and velocity into it, so the segment levels here stay the programmed
    /// ones and only their common ceiling moves.
    ///
    /// `time_scale` stretches every segment: 2.0 makes the whole envelope take
    /// twice as long. It is read once, here, so changing it does not disturb
    /// notes that are already sounding.
    pub fn operator(
        operator: &Operator,
        note: u8,
        ceiling: f32,
        time_scale: f32,
        sample_rate: f32,
    ) -> Self {
        let key_rate = key_rate_offset(note, operator.rate_scaling);
        let ceiling = ceiling.clamp(0.0, LEVEL_FULL + LEVEL_HEADROOM);
        let seconds = sample_rate * time_scale.max(f32::MIN_POSITIVE);
        let mut targets = [0.0; SEGMENTS];
        let mut steps = [0.0; SEGMENTS];
        for segment in 0..SEGMENTS {
            let programmed = scale_output_level(operator.eg_level[segment]) as f32 * 32.0;
            // A segment level is a fraction of the operator's own ceiling, so
            // an operator turned down keeps the shape of its envelope.
            targets[segment] =
                (ceiling - (LEVEL_FULL - programmed)).clamp(0.0, LEVEL_FULL + LEVEL_HEADROOM);
            let quantised = quantised_rate(operator.eg_rate[segment], key_rate);
            steps[segment] = rate_units_per_second(quantised) / seconds;
        }
        Self {
            level: targets[RELEASE],
            targets,
            steps,
            segment: 0,
            curved: true,
            floor: (ceiling - LEVEL_FULL).max(0.0),
            released: false,
            settled: false,
        }
    }

    /// The pitch envelope, shared by every operator in the voice. Its unit is
    /// the semitone and it has no ceiling to approach, so it does not curve.
    pub fn pitch(
        targets: [f32; SEGMENTS],
        rates: [u8; SEGMENTS],
        time_scale: f32,
        sample_rate: f32,
    ) -> Self {
        let mut steps = [0.0; SEGMENTS];
        let seconds = sample_rate * time_scale.max(f32::MIN_POSITIVE);
        for segment in 0..SEGMENTS {
            let quantised = quantised_rate(rates[segment], 0);
            // The same rate curve, rescaled from level units to semitones.
            steps[segment] = rate_units_per_second(quantised) * (96.0 / LEVEL_FULL) / seconds;
        }
        Self {
            level: targets[RELEASE],
            targets,
            steps,
            segment: 0,
            curved: false,
            floor: 0.0,
            released: false,
            settled: false,
        }
    }

    pub fn release(&mut self) {
        if !self.released {
            self.released = true;
            self.segment = RELEASE;
            self.settled = false;
        }
    }

    /// True once the envelope has reached its release level and stopped.
    pub fn is_settled(&self) -> bool {
        self.released && self.settled
    }

    /// True when this envelope can no longer be heard, whatever happens next.
    pub fn is_silent(&self) -> bool {
        self.released && self.level <= LEVEL_SILENT
    }

    pub fn level(&self) -> f32 {
        self.level
    }

    pub fn gain(&self) -> f32 {
        level_gain(self.level)
    }

    /// Advance one sample and return the new level.
    pub fn advance(&mut self) -> f32 {
        let target = self.targets[self.segment];
        let step = self.steps[self.segment];
        if self.level < target {
            let rise = if self.curved {
                // The hardware starts every rise from forty decibels under
                // the top rather than from silence.
                let jump = self.floor + ATTACK_JUMP;
                if self.level < jump {
                    self.level = jump.min(target);
                }
                rising_step(self.level, step)
            } else {
                step
            };
            self.level = (self.level + rise).min(target);
        } else if self.level > target {
            self.level = (self.level - step).max(target);
        }
        if self.level == target {
            if self.released || self.segment >= SUSTAIN {
                self.settled = true;
            } else {
                self.segment += 1;
                self.settled = false;
            }
        }
        self.level
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rf7_voice::Voice;

    fn full_operator() -> Operator {
        let mut operator = Voice::init().operators[0];
        operator.output_level = 99;
        operator
    }

    fn at_full(operator: &Operator, note: u8) -> Envelope {
        Envelope::operator(operator, note, LEVEL_FULL, 1.0, 48_000.0)
    }

    fn run(envelope: &mut Envelope, samples: usize) -> f32 {
        for _ in 0..samples {
            envelope.advance();
        }
        envelope.level()
    }

    #[test]
    fn the_fastest_envelope_opens_in_a_few_milliseconds() {
        let mut envelope = at_full(&full_operator(), 60);
        assert_eq!(envelope.level(), 0.0);
        let after_ten_ms = run(&mut envelope, 480);
        assert_eq!(after_ten_ms, LEVEL_FULL);
        assert_eq!(envelope.gain(), 1.0);
    }

    #[test]
    fn a_slow_envelope_is_still_climbing_after_a_second() {
        let mut operator = full_operator();
        operator.eg_rate = [0, 0, 0, 0];
        let mut envelope = at_full(&operator, 60);
        let after_a_second = run(&mut envelope, 48_000);
        assert!(after_a_second > 0.0);
        assert!(after_a_second < LEVEL_FULL);
    }

    #[test]
    fn the_segments_run_in_order_and_hold_at_the_third() {
        let mut operator = full_operator();
        operator.eg_rate = [99, 99, 99, 99];
        operator.eg_level = [99, 60, 30, 0];
        let mut envelope = at_full(&operator, 60);
        let held = run(&mut envelope, 48_000);
        let third = scale_output_level(30) as f32 * 32.0;
        assert!((held - third).abs() < 1.0);
        // Holding the key does not move it further.
        assert_eq!(run(&mut envelope, 48_000), held);
        assert!(!envelope.is_settled());
        envelope.release();
        assert_eq!(run(&mut envelope, 48_000), 0.0);
        assert!(envelope.is_settled());
        assert!(envelope.is_silent());
    }

    #[test]
    fn a_sustaining_release_level_keeps_the_envelope_audible() {
        let mut operator = full_operator();
        operator.eg_level = [99, 99, 99, 80];
        let mut envelope = at_full(&operator, 60);
        run(&mut envelope, 48_000);
        envelope.release();
        run(&mut envelope, 96_000);
        assert!(envelope.is_settled());
        assert!(!envelope.is_silent(), "release level 80 is not silence");
        assert!(envelope.gain() > 0.0);
    }

    #[test]
    fn a_lower_ceiling_lowers_the_whole_envelope() {
        let quiet = {
            let mut envelope =
                Envelope::operator(&full_operator(), 60, LEVEL_FULL / 2.0, 1.0, 48_000.0);
            run(&mut envelope, 48_000)
        };
        let loud = {
            let mut envelope = at_full(&full_operator(), 60);
            run(&mut envelope, 48_000)
        };
        assert!(quiet < loud);
        assert!(quiet > 0.0);
    }

    #[test]
    fn rate_scaling_shortens_the_top_of_the_keyboard() {
        let mut operator = full_operator();
        operator.eg_rate = [40, 40, 40, 40];
        operator.rate_scaling = 7;
        let low = run(&mut at_full(&operator, 24), 2_400);
        let high = run(&mut at_full(&operator, 108), 2_400);
        assert!(high > low, "the high key must open first");
    }

    #[test]
    fn stretching_time_slows_every_segment_by_the_same_factor() {
        let mut operator = full_operator();
        operator.eg_rate = [50, 50, 50, 50];
        let plain = {
            let mut envelope = Envelope::operator(&operator, 60, LEVEL_FULL, 1.0, 48_000.0);
            let mut samples = 0;
            while envelope.level() < LEVEL_FULL && samples < 48_000 * 10 {
                envelope.advance();
                samples += 1;
            }
            samples
        };
        let stretched = {
            let mut envelope = Envelope::operator(&operator, 60, LEVEL_FULL, 4.0, 48_000.0);
            let mut samples = 0;
            while envelope.level() < LEVEL_FULL && samples < 48_000 * 10 {
                envelope.advance();
                samples += 1;
            }
            samples
        };
        let ratio = stretched as f32 / plain as f32;
        assert!((ratio - 4.0).abs() < 0.05, "four times slower gave {ratio}");
    }

    #[test]
    fn the_pitch_envelope_rests_where_it_was_told_to() {
        let mut envelope =
            Envelope::pitch([12.0, 12.0, 12.0, 0.0], [99, 60, 60, 60], 1.0, 48_000.0);
        assert_eq!(envelope.level(), 0.0);
        let held = run(&mut envelope, 480);
        assert!(
            (held - 12.0).abs() < 0.01,
            "the sweep should be up at {held}"
        );
        assert_eq!(run(&mut envelope, 48_000), held);
        envelope.release();
        let settled = run(&mut envelope, 48_000);
        assert!(
            settled.abs() < 0.01,
            "the key is up and the pitch is {settled}"
        );
    }
}
