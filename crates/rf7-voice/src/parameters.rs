//! One voice, as the DX7 front panel presents it.
//!
//! Every field keeps the instrument's own units, including the awkward ones:
//! `algorithm` is 0..=31 and is displayed as 1..=32, `detune` is 0..=14 with 7
//! as centre, and `transpose` is 0..=48 with 24 as centre. Normalising them
//! here would make the byte layouts in [`crate::packed`] and
//! [`crate::unpacked`] harder to check against the documented tables.

pub const OPERATORS: usize = 6;
pub const NAME_LENGTH: usize = 10;

/// Keyboard level scaling shape on one side of the break point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Curve {
    NegativeLinear,
    NegativeExponential,
    PositiveExponential,
    PositiveLinear,
}

impl Curve {
    pub const fn from_index(index: u8) -> Self {
        match index & 3 {
            0 => Self::NegativeLinear,
            1 => Self::NegativeExponential,
            2 => Self::PositiveExponential,
            _ => Self::PositiveLinear,
        }
    }

    pub const fn is_exponential(self) -> bool {
        matches!(self, Self::NegativeExponential | Self::PositiveExponential)
    }

    /// The negative curves attenuate away from the break point; the positive
    /// ones boost. Nothing else about the two pairs differs.
    pub const fn is_positive(self) -> bool {
        matches!(self, Self::PositiveExponential | Self::PositiveLinear)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LfoWaveform {
    Triangle,
    SawDown,
    SawUp,
    Square,
    Sine,
    SampleAndHold,
}

impl LfoWaveform {
    pub const fn from_index(index: u8) -> Self {
        match index {
            0 => Self::Triangle,
            1 => Self::SawDown,
            2 => Self::SawUp,
            3 => Self::Square,
            4 => Self::Sine,
            _ => Self::SampleAndHold,
        }
    }
}

/// One of the six operators. Index 0 is OP1 throughout RF-7; the byte layouts
/// store OP6 first and reverse the order while decoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Operator {
    /// Envelope rates 1..4, 0..=99. Higher is faster.
    pub eg_rate: [u8; 4],
    /// Envelope levels 1..4, 0..=99.
    pub eg_level: [u8; 4],
    /// Keyboard level scaling break point, 0..=99. 39 is C3.
    pub break_point: u8,
    pub left_depth: u8,
    pub right_depth: u8,
    /// Curve index 0..=3, read through [`Curve::from_index`].
    pub left_curve: u8,
    pub right_curve: u8,
    /// Keyboard rate scaling, 0..=7.
    pub rate_scaling: u8,
    /// Amplitude modulation sensitivity, 0..=3.
    pub amp_mod_sensitivity: u8,
    /// Key velocity sensitivity, 0..=7.
    pub velocity_sensitivity: u8,
    pub output_level: u8,
    /// False is the ratio mode tracked by the key; true is a fixed frequency.
    pub fixed_frequency: bool,
    /// Frequency coarse, 0..=31. In ratio mode 0 means the half ratio.
    pub coarse: u8,
    /// Frequency fine, 0..=99.
    pub fine: u8,
    /// Detune 0..=14, 7 is centre.
    pub detune: u8,
}

impl Operator {
    /// The panel's INIT operator: a silent, instantly opening 1:1 operator.
    pub const fn init() -> Self {
        Self {
            eg_rate: [99, 99, 99, 99],
            eg_level: [99, 99, 99, 0],
            break_point: 39,
            left_depth: 0,
            right_depth: 0,
            left_curve: 0,
            right_curve: 0,
            rate_scaling: 0,
            amp_mod_sensitivity: 0,
            velocity_sensitivity: 0,
            output_level: 0,
            fixed_frequency: false,
            coarse: 1,
            fine: 0,
            detune: 7,
        }
    }

    pub const fn left(&self) -> Curve {
        Curve::from_index(self.left_curve)
    }

    pub const fn right(&self) -> Curve {
        Curve::from_index(self.right_curve)
    }

    pub(crate) fn clamp(&mut self, corrections: &mut u32) {
        for rate in &mut self.eg_rate {
            clamp(rate, 99, corrections);
        }
        for level in &mut self.eg_level {
            clamp(level, 99, corrections);
        }
        clamp(&mut self.break_point, 99, corrections);
        clamp(&mut self.left_depth, 99, corrections);
        clamp(&mut self.right_depth, 99, corrections);
        clamp(&mut self.left_curve, 3, corrections);
        clamp(&mut self.right_curve, 3, corrections);
        clamp(&mut self.rate_scaling, 7, corrections);
        clamp(&mut self.amp_mod_sensitivity, 3, corrections);
        clamp(&mut self.velocity_sensitivity, 7, corrections);
        clamp(&mut self.output_level, 99, corrections);
        clamp(&mut self.coarse, 31, corrections);
        clamp(&mut self.fine, 99, corrections);
        clamp(&mut self.detune, 14, corrections);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lfo {
    pub speed: u8,
    pub delay: u8,
    pub pitch_mod_depth: u8,
    pub amp_mod_depth: u8,
    /// Restart the shared LFO phase on the first key of a new phrase.
    pub sync: bool,
    /// Waveform index 0..=5, read through [`LfoWaveform::from_index`].
    pub waveform: u8,
}

impl Lfo {
    pub const fn init() -> Self {
        Self {
            speed: 35,
            delay: 0,
            pitch_mod_depth: 0,
            amp_mod_depth: 0,
            sync: true,
            waveform: 0,
        }
    }

    pub const fn shape(&self) -> LfoWaveform {
        LfoWaveform::from_index(self.waveform)
    }

    fn clamp(&mut self, corrections: &mut u32) {
        clamp(&mut self.speed, 99, corrections);
        clamp(&mut self.delay, 99, corrections);
        clamp(&mut self.pitch_mod_depth, 99, corrections);
        clamp(&mut self.amp_mod_depth, 99, corrections);
        clamp(&mut self.waveform, 5, corrections);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Voice {
    /// Index 0 is OP1.
    pub operators: [Operator; OPERATORS],
    pub pitch_eg_rate: [u8; 4],
    pub pitch_eg_level: [u8; 4],
    /// 0..=31, shown on the panel as algorithm 1..=32.
    pub algorithm: u8,
    pub feedback: u8,
    pub oscillator_sync: bool,
    pub lfo: Lfo,
    pub pitch_mod_sensitivity: u8,
    /// 0..=48, 24 is centre (no transposition).
    pub transpose: u8,
    /// Ten bytes of the DX7 display character set, stored as written.
    pub name: [u8; NAME_LENGTH],
}

impl Voice {
    /// INIT VOICE: OP1 alone at full output on algorithm 1.
    pub const fn init() -> Self {
        let mut operators = [Operator::init(); OPERATORS];
        operators[0].output_level = 99;
        Self {
            operators,
            pitch_eg_rate: [99, 99, 99, 99],
            pitch_eg_level: [50, 50, 50, 50],
            algorithm: 0,
            feedback: 0,
            oscillator_sync: true,
            lfo: Lfo::init(),
            pitch_mod_sensitivity: 3,
            transpose: 24,
            name: *b"INIT VOICE",
        }
    }

    /// Force every field into its documented range, counting each change.
    pub fn clamp(&mut self) -> u32 {
        let mut corrections = 0;
        for operator in &mut self.operators {
            operator.clamp(&mut corrections);
        }
        for rate in &mut self.pitch_eg_rate {
            clamp(rate, 99, &mut corrections);
        }
        for level in &mut self.pitch_eg_level {
            clamp(level, 99, &mut corrections);
        }
        clamp(&mut self.algorithm, 31, &mut corrections);
        clamp(&mut self.feedback, 7, &mut corrections);
        self.lfo.clamp(&mut corrections);
        clamp(&mut self.pitch_mod_sensitivity, 7, &mut corrections);
        clamp(&mut self.transpose, 48, &mut corrections);
        for byte in &mut self.name {
            if !(0x20..0x7f).contains(byte) {
                *byte = b' ';
                corrections += 1;
            }
        }
        corrections
    }
}

impl Default for Voice {
    fn default() -> Self {
        Self::init()
    }
}

/// The trailing-space-trimmed name as ASCII, never longer than ten bytes.
///
/// A name that never passed through [`Voice::clamp`] can hold bytes the DX7
/// display maps to its own glyphs; those decode to the empty string rather
/// than to a guess.
pub fn printable_name(name: &[u8; NAME_LENGTH]) -> &str {
    let end = name
        .iter()
        .rposition(|byte| *byte != b' ')
        .map_or(0, |index| index + 1);
    core::str::from_utf8(&name[..end]).unwrap_or("")
}

fn clamp(value: &mut u8, maximum: u8, corrections: &mut u32) {
    if *value > maximum {
        *value = maximum;
        *corrections += 1;
    }
}
