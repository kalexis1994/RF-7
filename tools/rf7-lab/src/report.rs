//! What a library's voices actually ask of the operators.
//!
//! For each voice: the algorithm, and for each operator whether it is heard,
//! modulates, or feeds back, its output level, and — for a modulator — the
//! index that level produces at the derived modulation depth. The summary at
//! the end is the reason the command exists: run over a real cartridge it is a
//! statistical portrait of how the instrument's own designers set their
//! modulators, which is the only calibration a hand-written bank can have
//! short of ears.

use rf7_dsp::{ALGORITHMS, modulation_index_at_level};
use rf7_voice::{Library, Voice, printable_name};
use std::fmt::Write;

pub struct Summary {
    pub voices: usize,
    pub modulator_levels: Vec<u8>,
    pub carrier_levels: Vec<u8>,
    pub feedback_levels: Vec<u8>,
    pub top_modulator_index: Vec<f32>,
}

impl Summary {
    pub fn of(library: &Library) -> Self {
        let mut summary = Self {
            voices: library.len(),
            modulator_levels: Vec::new(),
            carrier_levels: Vec::new(),
            feedback_levels: Vec::new(),
            top_modulator_index: Vec::new(),
        };
        for voice in library.voices() {
            let algorithm = &ALGORITHMS[usize::from(voice.algorithm.min(31))];
            let mut top = 0.0f32;
            for (index, operator) in voice.operators.iter().enumerate() {
                if algorithm.is_carrier(index) {
                    summary.carrier_levels.push(operator.output_level);
                } else {
                    summary.modulator_levels.push(operator.output_level);
                    top = top.max(modulation_index_at_level(operator.output_level));
                }
            }
            summary.top_modulator_index.push(top);
            summary.feedback_levels.push(voice.feedback);
        }
        summary
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "  {} voices", self.voices);
        let _ = writeln!(
            out,
            "  modulator output levels   p10 {:>3}  p25 {:>3}  p50 {:>3}  p75 {:>3}  p90 {:>3}   ({} operators)",
            percentile(&self.modulator_levels, 10),
            percentile(&self.modulator_levels, 25),
            percentile(&self.modulator_levels, 50),
            percentile(&self.modulator_levels, 75),
            percentile(&self.modulator_levels, 90),
            self.modulator_levels.len()
        );
        let _ = writeln!(
            out,
            "  carrier output levels     p10 {:>3}  p25 {:>3}  p50 {:>3}  p75 {:>3}  p90 {:>3}   ({} operators)",
            percentile(&self.carrier_levels, 10),
            percentile(&self.carrier_levels, 25),
            percentile(&self.carrier_levels, 50),
            percentile(&self.carrier_levels, 75),
            percentile(&self.carrier_levels, 90),
            self.carrier_levels.len()
        );
        let mut indexes = self.top_modulator_index.clone();
        indexes.sort_by(|a, b| a.total_cmp(b));
        let at = |p: usize| indexes.get(indexes.len() * p / 100).copied().unwrap_or(0.0);
        let _ = writeln!(
            out,
            "  strongest modulator index p10 {:>5.2}  p25 {:>5.2}  p50 {:>5.2}  p75 {:>5.2}  p90 {:>5.2}  (radians per voice)",
            at(10),
            at(25),
            at(50),
            at(75),
            at(90)
        );
        let _ = writeln!(
            out,
            "  feedback                  p50 {}   voices at 6 or 7: {} of {}",
            percentile(&self.feedback_levels, 50),
            self.feedback_levels.iter().filter(|f| **f >= 6).count(),
            self.voices
        );
        out
    }
}

fn percentile(values: &[u8], p: usize) -> u8 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted.get(sorted.len() * p / 100).copied().unwrap_or(0)
}

pub fn voice_line(slot: usize, voice: &Voice) -> String {
    let algorithm = &ALGORITHMS[usize::from(voice.algorithm.min(31))];
    let (from, to) = algorithm.feedback;
    let mut out = format!(
        "  {:>3}  {:<10}  alg {:>2}  fb {}  ",
        slot + 1,
        printable_name(&voice.name),
        voice.algorithm + 1,
        voice.feedback
    );
    for (index, operator) in voice.operators.iter().enumerate() {
        let role = if algorithm.is_carrier(index) {
            'C'
        } else {
            'M'
        };
        let loop_mark = if voice.feedback > 0 && usize::from(from) == index && from == to {
            '*'
        } else {
            ' '
        };
        if role == 'M' {
            let _ = write!(
                out,
                " {role}{loop_mark}{:>2}({:>4.1})",
                operator.output_level,
                modulation_index_at_level(operator.output_level)
            );
        } else {
            let _ = write!(out, " {role}{loop_mark}{:>2}      ", operator.output_level);
        }
    }
    out
}
