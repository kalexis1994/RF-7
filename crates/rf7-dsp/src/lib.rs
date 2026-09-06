//! The RF-7 six-operator FM engine.
//!
//! One [`Engine`] owns a cartridge, the sixteen voices the DX7 had, the single
//! shared LFO, and nothing else. It opens no files and no devices: a cartridge
//! arrives as bytes from whoever loaded them, and audio leaves one sample at a
//! time. Construction allocates everything; rendering allocates nothing.
//!
//! What is a documented property of the instrument and what is an RF-7
//! approximation is recorded per mapping in `docs/MODEL.md`.

mod algorithm;
/// The four-segment envelope, public so a laboratory can plot one without
/// rendering audio through it.
pub mod envelope;
mod lfo;
mod sine;
mod tables;
mod voice;

pub use algorithm::{ALGORITHMS, Algorithm};
pub use envelope::Envelope;
pub use sine::Sine;
pub use tables::{LEVEL_FULL, LEVEL_HEADROOM, modulation_index_at_level};
pub use voice::{Glide, Key, MODULATION_CYCLES, NoteVoice, Performance, VoiceSetup};

use lfo::Lfo;
use rf7_voice::{Library, Voice, factory_library, printable_name};

/// Six operators, as on the instrument. Not a parameter.
pub const OPERATORS: usize = 6;
/// Sixteen notes, as on the instrument.
pub const POLYPHONY: usize = 16;

pub const SAMPLE_RATE_MIN: f32 = 8_000.0;
pub const SAMPLE_RATE_MAX: f32 = 192_000.0;

/// Pitch bend range. The DX7 keeps this outside the voice data, among its
/// function parameters, so it belongs to the engine and not to a patch.
pub const BEND_SEMITONES_DEFAULT: f32 = 2.0;
pub const BEND_SEMITONES_MAX: f32 = 12.0;

/// Master tune, either side of concert pitch.
pub const TUNE_CENTS_MAX: f32 = 50.0;
/// Engine transpose, on top of whatever the voice itself asks for.
pub const TRANSPOSE_MAX: i32 = 24;
/// Brightness scales the modulation depth of every operator at once.
pub const BRIGHTNESS_MAX: f32 = 2.0;
/// Envelope time stretches or shortens every segment of every envelope.
pub const ENVELOPE_TIME_MIN: f32 = 0.25;
pub const ENVELOPE_TIME_MAX: f32 = 4.0;
/// Velocity depth scales how much level a soft key gives away.
pub const VELOCITY_DEPTH_MAX: f32 = 2.0;
/// The performance layer over the program's own LFO: a factor either side of
/// the speed it asks for, extra vibrato added to the depth it asks for, and
/// seconds added to its delay. Neutral at 1, 0 and 0.
pub const LFO_RATE_MIN: f32 = 0.25;
pub const LFO_RATE_MAX: f32 = 4.0;
pub const LFO_DEPTH_MAX: f32 = 1.0;
pub const LFO_DELAY_MAX: f32 = 4.0;
/// The portamento time, on the instrument's own 0..=99 dial.
pub const PORTAMENTO_TIME_MAX: f32 = 99.0;
/// How many keys a mono phrase can hold before the oldest is forgotten. The
/// instrument tracks its whole keyboard; sixteen is as deep as any player
/// reaches with ten fingers and a sustain pedal.
pub const MONO_STACK: usize = 16;

/// How the allocator hands out notes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VoiceMode {
    /// Sixteen notes at once.
    #[default]
    Poly,
    /// One note at a time, last-note priority: a key played over another
    /// takes the voice without restarting its envelopes, and releasing it
    /// hands the voice back to whichever key is still down.
    Mono,
}

impl VoiceMode {
    pub const fn from_index(index: u32) -> Self {
        match index {
            0 => Self::Poly,
            _ => Self::Mono,
        }
    }
}
/// Every operator heard.
pub const ALL_OPERATORS: u8 = (1 << OPERATORS) - 1;

/// Chosen against the loudest patches on a real cartridge rather than against
/// this project's own quieter factory voices: a four-note chord of a
/// four-carrier patch lands near −3 dBFS. Sixteen voices still sum past full
/// scale, and nothing here normalises that away behind the user.
pub const DEFAULT_GAIN: f64 = 0.3;
pub const GAIN_MAX: f64 = 2.0;

const CONTROL_MODULATION: u8 = 1;
const CONTROL_BREATH: u8 = 2;
const CONTROL_FOOT: u8 = 4;
const CONTROL_VOLUME: u8 = 7;
const CONTROL_EXPRESSION: u8 = 11;
const CONTROL_SUSTAIN: u8 = 64;
const CONTROL_PORTAMENTO: u8 = 65;
const CONTROL_ALL_SOUND_OFF: u8 = 120;
const CONTROL_ALL_NOTES_OFF: u8 = 123;

/// What a wheel, a pressure sensor, a breath controller or a pedal is wired
/// to.
///
/// The DX7 assigns each of its controllers to pitch, amplitude or the envelope
/// bias, independently. The first two are vibrato and tremolo through the LFO;
/// the third is what a breath controller is for: the operators that answer to
/// amplitude modulation sit below their programmed level while the controller
/// rests, and come up to it as the controller travels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Target {
    #[default]
    Pitch,
    Amplitude,
    Both,
    Bias,
}

impl Target {
    /// From the parameter schema's enum value.
    pub const fn from_index(index: u32) -> Self {
        match index {
            1 => Self::Amplitude,
            2 => Self::Both,
            3 => Self::Bias,
            _ => Self::Pitch,
        }
    }

    pub const fn index(self) -> u32 {
        match self {
            Self::Pitch => 0,
            Self::Amplitude => 1,
            Self::Both => 2,
            Self::Bias => 3,
        }
    }

    const fn moves_bias(self) -> bool {
        matches!(self, Self::Bias)
    }

    const fn moves_pitch(self) -> bool {
        matches!(self, Self::Pitch | Self::Both)
    }

    const fn moves_amplitude(self) -> bool {
        matches!(self, Self::Amplitude | Self::Both)
    }
}

/// Everything the player sets that is not part of a voice.
///
/// Some of this is the DX7's own function-parameter layer, which never lived
/// in a cartridge. The rest — brightness, envelope time, velocity depth and the
/// operator switches — the DX7 did not have as continuous controls, and RF-7
/// adds them because a cartridge cannot be edited yet. Every one of them is
/// neutral at its default, so a cartridge plays exactly as programmed until
/// something is moved. `docs/MODEL.md` says which is which.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Controls {
    /// How far the pitch wheel reaches, in semitones.
    pub bend_semitones: f32,
    pub master_tune_cents: f32,
    pub transpose: i32,
    pub wheel_range: f32,
    pub wheel_target: Target,
    pub aftertouch_range: f32,
    pub aftertouch_target: Target,
    /// The breath controller (controller 2) and the foot controller
    /// (controller 4), each with a reach and a destination of its own like
    /// the wheel. Both reaches rest at zero, so the program plays as written
    /// until one is opened.
    pub breath_range: f32,
    pub breath_target: Target,
    pub foot_range: f32,
    pub foot_target: Target,
    /// Scales the modulation depth of every operator. 1.0 is the patch's own.
    pub brightness: f32,
    pub envelope_time: f32,
    pub velocity_depth: f32,
    /// A factor on the program's LFO speed. 1.0 is the program's own.
    pub lfo_rate: f32,
    /// Vibrato added to whatever the program and the controllers already ask
    /// for. 0.0 adds none.
    pub lfo_depth: f32,
    /// Seconds added to the program's LFO delay. 0.0 adds none.
    pub lfo_delay: f32,
    pub voice_mode: VoiceMode,
    /// The instrument's portamento dial, 0..=99. Zero is no glide.
    pub portamento_time: f32,
    /// Bit `i` set means OP(i+1) is heard.
    pub operators: u8,
}

impl Default for Controls {
    fn default() -> Self {
        Self {
            bend_semitones: BEND_SEMITONES_DEFAULT,
            master_tune_cents: 0.0,
            transpose: 0,
            wheel_range: 1.0,
            wheel_target: Target::Pitch,
            aftertouch_range: 0.0,
            aftertouch_target: Target::Pitch,
            breath_range: 0.0,
            breath_target: Target::Bias,
            foot_range: 0.0,
            foot_target: Target::Bias,
            brightness: 1.0,
            envelope_time: 1.0,
            velocity_depth: 1.0,
            lfo_rate: 1.0,
            lfo_depth: 0.0,
            lfo_delay: 0.0,
            voice_mode: VoiceMode::Poly,
            portamento_time: 0.0,
            operators: ALL_OPERATORS,
        }
    }
}

impl Controls {
    /// Every field forced into the range the engine will act on. A caller that
    /// validated already loses nothing; one that did not cannot break the
    /// audio path with a stray value.
    pub fn clamped(self) -> Self {
        fn finite(value: f32, low: f32, high: f32, fallback: f32) -> f32 {
            if value.is_finite() {
                value.clamp(low, high)
            } else {
                fallback
            }
        }
        Self {
            bend_semitones: finite(
                self.bend_semitones,
                0.0,
                BEND_SEMITONES_MAX,
                BEND_SEMITONES_DEFAULT,
            ),
            master_tune_cents: finite(self.master_tune_cents, -TUNE_CENTS_MAX, TUNE_CENTS_MAX, 0.0),
            transpose: self.transpose.clamp(-TRANSPOSE_MAX, TRANSPOSE_MAX),
            wheel_range: finite(self.wheel_range, 0.0, 1.0, 1.0),
            aftertouch_range: finite(self.aftertouch_range, 0.0, 1.0, 0.0),
            breath_range: finite(self.breath_range, 0.0, 1.0, 0.0),
            foot_range: finite(self.foot_range, 0.0, 1.0, 0.0),
            brightness: finite(self.brightness, 0.0, BRIGHTNESS_MAX, 1.0),
            envelope_time: finite(
                self.envelope_time,
                ENVELOPE_TIME_MIN,
                ENVELOPE_TIME_MAX,
                1.0,
            ),
            velocity_depth: finite(self.velocity_depth, 0.0, VELOCITY_DEPTH_MAX, 1.0),
            lfo_rate: finite(self.lfo_rate, LFO_RATE_MIN, LFO_RATE_MAX, 1.0),
            lfo_depth: finite(self.lfo_depth, 0.0, LFO_DEPTH_MAX, 0.0),
            lfo_delay: finite(self.lfo_delay, 0.0, LFO_DELAY_MAX, 0.0),
            portamento_time: finite(self.portamento_time, 0.0, PORTAMENTO_TIME_MAX, 0.0),
            operators: self.operators & ALL_OPERATORS,
            ..self
        }
    }

    fn setup(&self) -> VoiceSetup {
        VoiceSetup {
            transpose: self.transpose,
            envelope_time: self.envelope_time,
            velocity_depth: self.velocity_depth,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineError {
    SampleRate,
}

impl core::fmt::Display for EngineError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::SampleRate => write!(
                formatter,
                "sample rate must be between {SAMPLE_RATE_MIN} and {SAMPLE_RATE_MAX} hertz"
            ),
        }
    }
}

impl core::error::Error for EngineError {}

pub struct Engine {
    voices: [NoteVoice; POLYPHONY],
    /// A note whose key is up but whose channel still holds the pedal.
    sustained: [bool; POLYPHONY],
    sine: Sine,
    library: Library,
    program: usize,
    patch: Voice,
    lfo: Lfo,
    sample_rate: f32,
    gain: f64,
    controls: Controls,
    /// Where the pitch wheel sits, -1.0..=1.0. Held as a position rather than
    /// as semitones so that widening the range moves a note already bent.
    bend_position: f32,
    /// Modulation wheel, 0.0..=1.0.
    wheel: f32,
    /// Channel pressure, 0.0..=1.0.
    pressure: f32,
    /// Breath controller and foot controller, 0.0..=1.0 each.
    breath: f32,
    foot: f32,
    /// Channel volume (controller 7) and expression (controller 11), both
    /// full until a message says otherwise. They are levels, not positions,
    /// so a reset leaves them where the keyboard put them.
    volume: f32,
    expression: f32,
    /// One bit per MIDI channel.
    pedals: u16,
    /// Semitones of LFO pitch modulation at full depth.
    pitch_depth: f32,
    /// How much amplitude modulation the patch asks for, 0.0..=1.0.
    amp_depth: f32,
    /// Notes that arrived with every voice already busy.
    stolen: u64,
    /// The keys a mono phrase is holding, oldest first. The last of them is
    /// the one that sounds.
    keys: [(u8, u8); MONO_STACK],
    key_count: usize,
    /// The note a glide starts from: whatever was played last.
    last_note: Option<u8>,
    /// The portamento switch, controller 65. On unless a pedal says otherwise.
    portamento_switch: bool,
}

/// A mono phrase always sounds on the same voice, so a line never stacks the
/// release tails of the notes it has left behind.
const MONO_SLOT: usize = 0;

impl Engine {
    pub fn new(sample_rate: f32) -> Result<Self, EngineError> {
        if !sample_rate.is_finite() || !(SAMPLE_RATE_MIN..=SAMPLE_RATE_MAX).contains(&sample_rate) {
            return Err(EngineError::SampleRate);
        }
        let library = factory_library();
        let patch = *library.voice(0).expect("a library is never empty");
        let mut engine = Self {
            voices: [NoteVoice::default(); POLYPHONY],
            sustained: [false; POLYPHONY],
            sine: Sine::new(),
            library,
            program: 0,
            patch,
            lfo: Lfo::new(&patch.lfo, sample_rate),
            sample_rate,
            gain: DEFAULT_GAIN,
            controls: Controls::default(),
            bend_position: 0.0,
            wheel: 0.0,
            pressure: 0.0,
            breath: 0.0,
            foot: 0.0,
            volume: 1.0,
            expression: 1.0,
            pedals: 0,
            pitch_depth: 0.0,
            amp_depth: 0.0,
            stolen: 0,
            keys: [(0, 0); MONO_STACK],
            key_count: 0,
            last_note: None,
            portamento_switch: true,
        };
        engine.apply_patch();
        Ok(engine)
    }

    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    /// Notes that took a voice from another note because all sixteen were
    /// busy, since the last [`Engine::reset`]. Reported, not hidden.
    pub fn stolen_notes(&self) -> u64 {
        self.stolen
    }

    pub fn set_gain(&mut self, gain: f64) -> bool {
        if !gain.is_finite() || !(0.0..=GAIN_MAX).contains(&gain) {
            return false;
        }
        self.gain = gain;
        true
    }

    pub fn gain(&self) -> f64 {
        self.gain
    }

    /// Replace the whole control set. Values are clamped, never rejected: a
    /// control is not an event, and refusing one would leave the engine in a
    /// state the caller did not ask for and cannot see.
    pub fn set_controls(&mut self, controls: Controls) {
        self.controls = controls.clamped();
        self.retune_lfo();
    }

    /// The LFO reads its speed and delay from the program and the performance
    /// layer together, so either changing means taking them again.
    fn retune_lfo(&mut self) {
        self.lfo.retune(
            &self.patch.lfo,
            self.sample_rate,
            self.controls.lfo_rate,
            self.controls.lfo_delay,
        );
    }

    pub fn controls(&self) -> Controls {
        self.controls
    }

    /// Replace every program at once. Sounding notes keep the patch they began
    /// with, which is what lets this land mid-phrase.
    ///
    /// The selected program is carried across where it still exists, and
    /// otherwise falls back to the first: a library can be shorter than the
    /// one it replaces, and a program index pointing past the end would be a
    /// silent instrument with no explanation.
    pub fn load_library(&mut self, library: Library) {
        self.library = library;
        let program = self.program.min(self.library.len().saturating_sub(1));
        self.select_program(program);
    }

    pub fn library(&self) -> &Library {
        &self.library
    }

    pub fn program_count(&self) -> usize {
        self.library.len()
    }

    pub fn program(&self) -> usize {
        self.program
    }

    pub fn program_name(&self, index: usize) -> &str {
        self.library
            .voice(index)
            .map_or("", |voice| printable_name(&voice.name))
    }

    pub fn select_program(&mut self, index: usize) -> bool {
        let Some(voice) = self.library.voice(index) else {
            return false;
        };
        self.program = index;
        self.patch = *voice;
        self.apply_patch();
        true
    }

    /// The patch a new note would use.
    pub fn patch(&self) -> &Voice {
        &self.patch
    }

    /// Play a voice that is not in the library — a draft under edit. The
    /// program index is left where it was, so a later program change lands
    /// on the same program it would have before.
    pub fn load_patch(&mut self, voice: Voice) {
        self.patch = voice;
        self.apply_patch();
    }

    fn apply_patch(&mut self) {
        self.lfo = Lfo::new(&self.patch.lfo, self.sample_rate);
        self.retune_lfo();
        self.pitch_depth = tables::pitch_mod_semitones(self.patch.pitch_mod_sensitivity);
        self.amp_depth = f32::from(self.patch.lfo.amp_mod_depth.min(99)) / 99.0;
    }

    pub fn note_on(&mut self, channel: u8, note: u8, velocity: u8) {
        if note > 127 {
            return;
        }
        if velocity == 0 {
            self.note_off(channel, note);
            return;
        }
        if !self.voices.iter().any(NoteVoice::is_active) {
            // The LFO delay and its phase restart belong to the start of a
            // phrase, not to every key in it.
            self.lfo.key_down();
        }
        match self.controls.voice_mode {
            VoiceMode::Poly => {
                let slot = self.allocate();
                self.start(slot, channel, note, velocity);
            }
            VoiceMode::Mono => {
                // A key played while another is still down takes the voice
                // without starting it again: on the instrument a legato note
                // does not retrigger its envelopes.
                let legato = self.push_key(note, velocity)
                    && self.voices[MONO_SLOT].is_active()
                    && self.voices[MONO_SLOT].is_held();
                if legato {
                    self.retune(note);
                } else {
                    self.start(MONO_SLOT, channel, note, velocity);
                }
            }
        }
        self.last_note = Some(note);
    }

    pub fn note_off(&mut self, channel: u8, note: u8) {
        let held = self.pedals & (1 << (channel & 15)) != 0;
        if self.controls.voice_mode == VoiceMode::Mono {
            if !self.remove_key(note) {
                return;
            }
            match self.keys[..self.key_count].last().copied() {
                // A key is still down: the voice goes back to it, again
                // without starting over.
                Some((held_note, _)) => self.retune(held_note),
                None => {
                    if held {
                        self.sustained[MONO_SLOT] = true;
                    } else {
                        self.voices[MONO_SLOT].release();
                    }
                }
            }
            return;
        }
        for (slot, voice) in self.voices.iter_mut().enumerate() {
            if voice.is_active()
                && voice.is_held()
                && voice.channel() == channel
                && voice.note() == note
            {
                if held {
                    self.sustained[slot] = true;
                } else {
                    voice.release();
                }
            }
        }
    }

    /// Start a note in a slot, gliding from whatever was played before it.
    fn start(&mut self, slot: usize, channel: u8, note: u8, velocity: u8) {
        let setup = self.controls.setup();
        let glide = self.glide();
        self.voices[slot].start(
            &self.patch,
            Key {
                channel,
                note,
                velocity,
            },
            &setup,
            self.sample_rate,
            glide,
        );
        self.sustained[slot] = false;
    }

    /// Send the mono voice to another note, keeping its envelopes.
    fn retune(&mut self, note: u8) {
        let setup = self.controls.setup();
        let glide = self.glide();
        self.voices[MONO_SLOT].retune(&self.patch, note, &setup, self.sample_rate, glide);
        self.sustained[MONO_SLOT] = false;
    }

    /// Where a new note starts from and how fast it arrives. A portamento
    /// time of zero is no glide at all, so the instrument plays as it always
    /// has until the dial is moved; the switch on controller 65 turns off a
    /// glide that is set.
    fn glide(&self) -> Glide {
        let time = self.controls.portamento_time.round().clamp(0.0, 99.0) as u8;
        let gliding = self.portamento_switch && time > 0;
        Glide {
            from: self.last_note.filter(|_| gliding),
            semitones_per_second: if gliding {
                tables::portamento_semitones_per_second(time)
            } else {
                0.0
            },
        }
    }

    /// Remember a held key. Returns whether another was already down, which
    /// is what makes the new note legato.
    fn push_key(&mut self, note: u8, velocity: u8) -> bool {
        let legato = self.key_count > 0;
        self.forget_key(note);
        if self.key_count == MONO_STACK {
            self.keys.copy_within(1.., 0);
            self.key_count -= 1;
        }
        self.keys[self.key_count] = (note, velocity);
        self.key_count += 1;
        legato
    }

    /// Release a held key. Returns whether it was the one sounding.
    fn remove_key(&mut self, note: u8) -> bool {
        let sounding = self.keys[..self.key_count]
            .last()
            .is_some_and(|(held, _)| *held == note);
        self.forget_key(note);
        sounding
    }

    fn forget_key(&mut self, note: u8) {
        if let Some(index) = self.keys[..self.key_count]
            .iter()
            .position(|(held, _)| *held == note)
        {
            self.keys.copy_within(index + 1..self.key_count, index);
            self.key_count -= 1;
        }
    }

    /// `value` is normalised to 0.0..=1.0 by the caller, which is what lets a
    /// seven-bit controller and a thirty-two-bit one arrive the same way.
    pub fn control_change(&mut self, channel: u8, controller: u8, value: f64) {
        let value = value.clamp(0.0, 1.0);
        match controller {
            CONTROL_MODULATION => self.wheel = value as f32,
            CONTROL_BREATH => self.breath = value as f32,
            CONTROL_FOOT => self.foot = value as f32,
            CONTROL_VOLUME => self.volume = value as f32,
            CONTROL_EXPRESSION => self.expression = value as f32,
            CONTROL_SUSTAIN => {
                let bit = 1 << (channel & 15);
                if value >= 0.5 {
                    self.pedals |= bit;
                } else {
                    self.pedals &= !bit;
                    for (slot, voice) in self.voices.iter_mut().enumerate() {
                        if self.sustained[slot] && voice.channel() == channel {
                            voice.release();
                            self.sustained[slot] = false;
                        }
                    }
                }
            }
            CONTROL_PORTAMENTO => self.portamento_switch = value >= 0.5,
            CONTROL_ALL_NOTES_OFF => {
                self.key_count = 0;
                for (slot, voice) in self.voices.iter_mut().enumerate() {
                    voice.release();
                    self.sustained[slot] = false;
                }
            }
            CONTROL_ALL_SOUND_OFF => self.reset(),
            _ => {}
        }
    }

    /// `value` is -1.0..=1.0 across the whole wheel travel.
    pub fn pitch_bend(&mut self, _channel: u8, value: f64) {
        self.bend_position = value.clamp(-1.0, 1.0) as f32;
    }

    /// Channel pressure, 0.0..=1.0. Its destination and how far it reaches are
    /// both controls, and it does nothing until the range is opened.
    pub fn channel_pressure(&mut self, _channel: u8, value: f64) {
        self.pressure = value.clamp(0.0, 1.0) as f32;
    }

    /// Stop everything at once, with no release segment.
    pub fn reset(&mut self) {
        for voice in &mut self.voices {
            voice.silence();
        }
        self.sustained = [false; POLYPHONY];
        self.pedals = 0;
        self.bend_position = 0.0;
        self.wheel = 0.0;
        self.pressure = 0.0;
        self.breath = 0.0;
        self.foot = 0.0;
        self.stolen = 0;
        self.key_count = 0;
        self.last_note = None;
        self.lfo = Lfo::new(&self.patch.lfo, self.sample_rate);
        self.retune_lfo();
    }

    pub fn active_voices(&self) -> usize {
        self.voices.iter().filter(|voice| voice.is_active()).count()
    }

    pub fn next_sample(&mut self) -> f32 {
        let modulation = self.lfo.advance();
        // Every controller opens the same destinations, so they are summed
        // per destination rather than each fighting for the whole depth.
        let controls = &self.controls;
        let mut added_pitch = 0.0;
        let mut added_amplitude = 0.0;
        let mut bias = 0.0;
        for (position, range, target) in [
            (self.wheel, controls.wheel_range, controls.wheel_target),
            (
                self.pressure,
                controls.aftertouch_range,
                controls.aftertouch_target,
            ),
            (self.breath, controls.breath_range, controls.breath_target),
            (self.foot, controls.foot_range, controls.foot_target),
        ] {
            let amount = position * range;
            if target.moves_pitch() {
                added_pitch += amount;
            }
            if target.moves_amplitude() {
                added_amplitude += amount;
            }
            // The bias is what the controller has not yet given back: at rest
            // the operators sit the whole reach below their level, and at full
            // travel they are where the program put them.
            if target.moves_bias() {
                bias += range - amount;
            }
        }
        let pitch_depth = (f32::from(self.patch.lfo.pitch_mod_depth.min(99)) / 99.0
            + added_pitch
            + self.controls.lfo_depth)
            .min(1.0);
        let amplitude_depth = (self.amp_depth + added_amplitude).min(1.0);
        let performance = Performance {
            pitch: self.bend_position * self.controls.bend_semitones
                + self.controls.master_tune_cents / 100.0
                + modulation * self.pitch_depth * pitch_depth,
            // Amplitude modulation only ever takes level away, so the LFO is
            // read as a unipolar dip rather than a swing either side of it.
            amplitude: amplitude_depth * (1.0 - modulation) * 0.5,
            bias: bias.min(1.0),
            modulation: MODULATION_CYCLES * self.controls.brightness,
            operators: self.controls.operators,
        };
        let mut sum = 0.0;
        for voice in &mut self.voices {
            sum += voice.next_sample(&self.sine, &performance);
        }
        sum * self.gain as f32 * self.volume * self.expression
    }

    /// The slot a new note should take: a free one, else the oldest released
    /// note, else the oldest note of all.
    fn allocate(&mut self) -> usize {
        let mut free = None;
        let mut released: Option<usize> = None;
        let mut oldest = 0;
        for (slot, voice) in self.voices.iter().enumerate() {
            if !voice.is_active() {
                free = Some(slot);
                break;
            }
            if !voice.is_held()
                && !self.sustained[slot]
                && released.is_none_or(|best| voice.age() > self.voices[best].age())
            {
                released = Some(slot);
            }
            if voice.age() > self.voices[oldest].age() {
                oldest = slot;
            }
        }
        if let Some(slot) = free {
            return slot;
        }
        self.stolen += 1;
        released.unwrap_or(oldest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rf7_voice::{
        Cartridge, FACTORY_VOICES, VOICES_PER_CARTRIDGE, decode_library, encode_bulk_dump,
    };

    fn engine() -> Engine {
        Engine::new(48_000.0).expect("48 kHz is a supported rate")
    }

    fn engine_with(controls: Controls) -> Engine {
        let mut engine = engine();
        engine.set_controls(controls);
        engine
    }

    fn render(engine: &mut Engine, samples: usize) -> Vec<f32> {
        (0..samples).map(|_| engine.next_sample()).collect()
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0f32, |peak, s| peak.max(s.abs()))
    }

    fn rms(samples: &[f32]) -> f32 {
        let energy: f64 = samples.iter().map(|s| f64::from(*s) * f64::from(*s)).sum();
        (energy / samples.len().max(1) as f64).sqrt() as f32
    }

    fn crossings(samples: &[f32]) -> usize {
        samples
            .windows(2)
            .filter(|pair| pair[0] <= 0.0 && pair[1] > 0.0)
            .count()
    }

    #[test]
    fn a_rate_outside_the_supported_range_is_refused() {
        for rate in [0.0, 7_999.0, 192_001.0, f32::NAN, f32::INFINITY] {
            assert_eq!(Engine::new(rate).err(), Some(EngineError::SampleRate));
        }
        for rate in [8_000.0, 44_100.0, 48_000.0, 96_000.0, 192_000.0] {
            assert_eq!(Engine::new(rate).map(|e| e.sample_rate()).ok(), Some(rate));
        }
    }

    #[test]
    fn silence_before_a_key_is_pressed() {
        let mut engine = engine();
        assert_eq!(peak(&render(&mut engine, 4_800)), 0.0);
        assert_eq!(engine.active_voices(), 0);
    }

    #[test]
    fn a_key_makes_sound_and_releasing_it_frees_the_voice() {
        let mut engine = engine();
        engine.select_program(4); // RF MARIMBA, which ends on its own
        engine.note_on(0, 60, 100);
        assert_eq!(engine.active_voices(), 1);
        assert!(peak(&render(&mut engine, 4_800)) > 0.0);
        engine.note_off(0, 60);
        render(&mut engine, 48_000 * 4);
        assert_eq!(engine.active_voices(), 0);
    }

    #[test]
    fn the_pedal_holds_a_note_whose_key_is_up() {
        let mut engine = engine();
        engine.select_program(4);
        engine.control_change(0, CONTROL_SUSTAIN, 1.0);
        engine.note_on(0, 60, 100);
        engine.note_off(0, 60);
        render(&mut engine, 4_800);
        assert_eq!(engine.active_voices(), 1, "the pedal is down");
        engine.control_change(0, CONTROL_SUSTAIN, 0.0);
        render(&mut engine, 48_000 * 4);
        assert_eq!(engine.active_voices(), 0, "the pedal came up");
    }

    #[test]
    fn a_seventeenth_note_takes_a_voice_and_says_so() {
        let mut engine = engine();
        for note in 40..56 {
            engine.note_on(0, note, 100);
        }
        assert_eq!(engine.active_voices(), POLYPHONY);
        assert_eq!(engine.stolen_notes(), 0);
        engine.note_on(0, 60, 100);
        assert_eq!(engine.active_voices(), POLYPHONY);
        assert_eq!(engine.stolen_notes(), 1);
        assert!(
            engine.voices.iter().any(|voice| voice.note() == 60),
            "the new note must actually sound"
        );
    }

    #[test]
    fn a_released_note_is_taken_before_a_held_one() {
        let mut engine = engine();
        for note in 40..56 {
            engine.note_on(0, note, 100);
        }
        engine.note_off(0, 47);
        render(&mut engine, 480);
        engine.note_on(0, 90, 100);
        assert!(
            !engine.voices.iter().any(|voice| voice.note() == 47),
            "the released note should have been the one taken"
        );
        for note in 40..47 {
            assert!(engine.voices.iter().any(|voice| voice.note() == note));
        }
    }

    #[test]
    fn every_factory_program_sounds_and_stays_finite() {
        assert_eq!(engine().program_count(), FACTORY_VOICES);
        for program in 0..FACTORY_VOICES {
            let mut engine = engine();
            assert!(engine.select_program(program));
            assert!(!engine.program_name(program).is_empty());
            engine.note_on(0, 57, 100);
            engine.note_on(0, 64, 90);
            // Two seconds: a pad with a forty-rate attack is not audible in
            // half a second, and that is what makes it a pad.
            let rendered = render(&mut engine, 96_000);
            assert!(peak(&rendered) > 0.001, "program {program} is silent");
            assert!(rendered.iter().all(|sample| sample.is_finite()));
        }
        assert!(
            !engine().select_program(FACTORY_VOICES),
            "and offers no more"
        );
    }

    #[test]
    fn a_program_change_does_not_cut_a_sounding_note() {
        let mut engine = engine();
        engine.select_program(6); // RF ORGAN, which sustains
        engine.note_on(0, 60, 100);
        let before = peak(&render(&mut engine, 4_800));
        engine.select_program(1);
        let after = peak(&render(&mut engine, 4_800));
        assert!(after > before * 0.5, "the note was cut short");
        assert_eq!(engine.active_voices(), 1);
    }

    #[test]
    fn a_cartridge_the_user_loaded_replaces_the_programs() {
        let mut engine = engine();
        let mut voices = [rf7_voice::factory_voice(3); VOICES_PER_CARTRIDGE];
        voices[0].name = *b"LOADED    ";
        let dump = encode_bulk_dump(&Cartridge::from_voices(voices), 0);
        engine.load_library(decode_library(&dump).expect("our own dump decodes"));
        assert_eq!(engine.program_count(), VOICES_PER_CARTRIDGE);
        assert_eq!(engine.program_name(0), "LOADED");
        engine.note_on(0, 60, 100);
        assert!(peak(&render(&mut engine, 4_800)) > 0.0);
    }

    #[test]
    fn a_shorter_library_does_not_leave_the_program_past_its_end() {
        let mut engine = engine();
        let dump = encode_bulk_dump(
            &Cartridge::from_voices([rf7_voice::factory_voice(1); VOICES_PER_CARTRIDGE]),
            0,
        );
        engine.load_library(decode_library(&dump).expect("a dump decodes"));
        assert!(engine.select_program(28));
        // Down to a library of one: program 28 no longer exists.
        engine.load_library(Library::from_voices(&[rf7_voice::factory_voice(2)]));
        assert_eq!(engine.program_count(), 1);
        assert_eq!(engine.program(), 0, "the selection followed the library");
        engine.note_on(0, 60, 100);
        assert!(peak(&render(&mut engine, 4_800)) > 0.0);
    }

    #[test]
    fn everything_off_leaves_no_sound_behind() {
        let mut engine = engine();
        for note in 40..56 {
            engine.note_on(0, note, 100);
        }
        render(&mut engine, 480);
        engine.control_change(0, CONTROL_ALL_SOUND_OFF, 1.0);
        assert_eq!(engine.active_voices(), 0);
        assert_eq!(peak(&render(&mut engine, 4_800)), 0.0);
        assert_eq!(engine.stolen_notes(), 0);
    }

    #[test]
    fn all_notes_off_releases_rather_than_cuts() {
        let mut engine = engine();
        engine.select_program(4);
        engine.note_on(0, 60, 100);
        render(&mut engine, 480);
        engine.control_change(0, CONTROL_ALL_NOTES_OFF, 1.0);
        assert_eq!(engine.active_voices(), 1, "a release is not a cut");
        render(&mut engine, 48_000 * 4);
        assert_eq!(engine.active_voices(), 0);
    }

    #[test]
    fn the_bend_wheel_moves_the_pitch_and_returns_it() {
        let mut engine = engine();
        engine.select_program(6);
        engine.note_on(0, 60, 100);
        let plain = crossings(&render(&mut engine, 24_000));
        engine.pitch_bend(0, 1.0);
        let bent = crossings(&render(&mut engine, 24_000));
        assert!(bent > plain, "bending up should raise the pitch");
        engine.pitch_bend(0, 0.0);
        let restored = crossings(&render(&mut engine, 24_000));
        assert!(restored < bent);
    }

    #[test]
    fn out_of_range_controls_are_clamped_rather_than_refused() {
        let mut engine = engine();
        engine.set_controls(Controls {
            bend_semitones: 99.0,
            master_tune_cents: -400.0,
            transpose: 100,
            wheel_range: 5.0,
            aftertouch_range: -1.0,
            brightness: f32::NAN,
            envelope_time: 0.0,
            velocity_depth: f32::INFINITY,
            operators: 0xff,
            ..Controls::default()
        });
        let controls = engine.controls();
        assert_eq!(controls.bend_semitones, BEND_SEMITONES_MAX);
        assert_eq!(controls.master_tune_cents, -TUNE_CENTS_MAX);
        assert_eq!(controls.transpose, TRANSPOSE_MAX);
        assert_eq!(controls.wheel_range, 1.0);
        assert_eq!(controls.aftertouch_range, 0.0);
        assert_eq!(controls.brightness, 1.0, "a broken value falls back");
        assert_eq!(controls.envelope_time, ENVELOPE_TIME_MIN);
        assert_eq!(controls.velocity_depth, 1.0);
        assert_eq!(controls.operators, ALL_OPERATORS);
    }

    #[test]
    fn the_defaults_leave_a_cartridge_exactly_as_programmed() {
        // Every added control is neutral where it starts, so this render and
        // one through an engine that never heard of them must agree.
        let mut plain = engine();
        plain.select_program(0);
        plain.note_on(0, 60, 100);
        let reference = render(&mut plain, 12_000);

        let mut set = engine();
        set.set_controls(Controls::default());
        set.select_program(0);
        set.note_on(0, 60, 100);
        assert_eq!(render(&mut set, 12_000), reference);
    }

    #[test]
    fn the_bend_range_widens_a_note_that_is_already_bent() {
        let mut engine = engine();
        engine.select_program(6);
        engine.note_on(0, 60, 100);
        engine.pitch_bend(0, 1.0);
        let two = crossings(&render(&mut engine, 24_000));
        engine.set_controls(Controls {
            bend_semitones: 12.0,
            ..Controls::default()
        });
        let twelve = crossings(&render(&mut engine, 24_000));
        assert!(
            twelve > two,
            "a wider range must reach further: {two} {twelve}"
        );
        engine.set_controls(Controls {
            bend_semitones: 0.0,
            ..Controls::default()
        });
        let none = crossings(&render(&mut engine, 24_000));
        assert!(none < two, "a range of zero must not bend at all");
    }

    #[test]
    fn master_tune_moves_the_whole_instrument() {
        let mut flat = engine();
        flat.set_controls(Controls {
            master_tune_cents: -50.0,
            ..Controls::default()
        });
        flat.select_program(6);
        flat.note_on(0, 69, 100);
        let low = crossings(&render(&mut flat, 96_000));

        let mut sharp = engine();
        sharp.set_controls(Controls {
            master_tune_cents: 50.0,
            ..Controls::default()
        });
        sharp.select_program(6);
        sharp.note_on(0, 69, 100);
        let high = crossings(&render(&mut sharp, 96_000));
        assert!(high > low, "a semitone of tuning should be audible");
    }

    #[test]
    fn aftertouch_does_nothing_until_its_range_is_opened() {
        let mut engine = engine();
        engine.select_program(0);
        engine.note_on(0, 60, 100);
        engine.channel_pressure(0, 1.0);
        let closed = render(&mut engine, 12_000);

        let mut open = engine_with(Controls {
            aftertouch_range: 1.0,
            aftertouch_target: Target::Pitch,
            ..Controls::default()
        });
        open.select_program(0);
        open.note_on(0, 60, 100);
        open.channel_pressure(0, 1.0);
        let moved = render(&mut open, 12_000);
        assert_ne!(closed, moved, "an opened range should be heard");
    }

    #[test]
    fn a_target_reaches_only_what_it_names() {
        // Built here rather than taken from the factory bank: no factory voice
        // sets an operator's amplitude modulation sensitivity, and a control
        // that reaches nothing would prove nothing.
        let mut voice = Voice::init();
        voice.operators[0].amp_mod_sensitivity = 3;
        voice.pitch_mod_sensitivity = 7;
        let library = rf7_voice::Library::from_voices(&[voice]);
        let sounded = |target: Target| {
            let mut engine = engine_with(Controls {
                wheel_target: target,
                ..Controls::default()
            });
            engine.load_library(library.clone());
            engine.control_change(0, CONTROL_MODULATION, 1.0);
            engine.note_on(0, 60, 100);
            render(&mut engine, 24_000)
        };
        let pitch = sounded(Target::Pitch);
        let amplitude = sounded(Target::Amplitude);
        let both = sounded(Target::Both);
        assert_ne!(pitch, amplitude, "the two destinations are not the same");
        assert_ne!(pitch, both, "both must reach further than pitch alone");
        assert_ne!(amplitude, both);
        assert_eq!(Target::from_index(Target::Both.index()), Target::Both);
        assert_eq!(Target::from_index(99), Target::Pitch);
    }

    #[test]
    fn the_breath_controller_lifts_the_envelope_bias_as_it_is_blown() {
        // The bias reaches the operators through the same sensitivity as the
        // amplitude modulation, which no factory voice sets.
        let mut voice = Voice::init();
        voice.operators[0].amp_mod_sensitivity = 3;
        let library = rf7_voice::Library::from_voices(&[voice]);
        let level_at = |breath: f64| {
            let mut engine = engine_with(Controls {
                breath_range: 1.0,
                breath_target: Target::Bias,
                ..Controls::default()
            });
            engine.load_library(library.clone());
            engine.control_change(0, CONTROL_BREATH, breath);
            engine.note_on(0, 60, 100);
            rms(&render(&mut engine, 12_000))
        };
        let rest = level_at(0.0);
        let half = level_at(0.5);
        let full = level_at(1.0);
        assert!(rest < half && half < full, "{rest} {half} {full}");
        // Full travel gives the program back exactly as written, and a closed
        // reach leaves the controller nothing to move.
        let mut untouched = engine();
        untouched.load_library(library.clone());
        untouched.note_on(0, 60, 100);
        let plain = rms(&render(&mut untouched, 12_000));
        assert!((full - plain).abs() < 1e-5, "{full} {plain}");
        let mut closed_reach = engine();
        closed_reach.load_library(library);
        closed_reach.control_change(0, CONTROL_BREATH, 0.0);
        closed_reach.note_on(0, 60, 100);
        let closed = rms(&render(&mut closed_reach, 12_000));
        assert!((closed - plain).abs() < 1e-5, "{closed} {plain}");
        assert_eq!(Target::from_index(Target::Bias.index()), Target::Bias);
    }

    #[test]
    fn the_sustained_factory_voices_swell_under_the_breath_controller() {
        // The instrument's breath-controlled voices set the amplitude
        // modulation sensitivity on the operators the bias should lift, and
        // RF-7's sustained voices do the same, so a breath controller has
        // something to move out of the box; a struck voice does not answer.
        let level = |name: &str, breath: f64| {
            let mut engine = engine_with(Controls {
                breath_range: 1.0,
                breath_target: Target::Bias,
                ..Controls::default()
            });
            let program = (0..engine.library().len())
                .find(|i| {
                    rf7_voice::printable_name(&engine.library().voice(*i).unwrap().name).trim()
                        == name
                })
                .expect(name);
            engine.select_program(program);
            engine.control_change(0, CONTROL_BREATH, breath);
            engine.note_on(0, 60, 100);
            rms(&render(&mut engine, 24_000))
        };
        for name in ["RF BRASS", "RF SAX", "RF STRINGS", "RF LEAD", "RF PIPE"] {
            let (rest, blown) = (level(name, 0.0), level(name, 1.0));
            assert!(blown > rest * 1.5, "{name}: {rest} at rest, {blown} blown");
        }
        let (rest, blown) = (level("RF TINES", 0.0), level("RF TINES", 1.0));
        assert!(
            (blown - rest).abs() < 1e-6,
            "a struck voice: {rest} {blown}"
        );
    }

    #[test]
    fn the_foot_controller_is_a_controller_of_its_own() {
        let mut voice = Voice::init();
        voice.pitch_mod_sensitivity = 7;
        let library = rf7_voice::Library::from_voices(&[voice]);
        let sounded = |controller: u8| {
            let mut engine = engine_with(Controls {
                foot_range: 1.0,
                foot_target: Target::Pitch,
                ..Controls::default()
            });
            engine.load_library(library.clone());
            engine.control_change(0, controller, 1.0);
            engine.note_on(0, 60, 100);
            render(&mut engine, 24_000)
        };
        assert_ne!(
            sounded(CONTROL_FOOT),
            sounded(CONTROL_BREATH),
            "only the foot controller is wired to pitch here"
        );
    }

    #[test]
    fn volume_and_expression_scale_the_output_and_survive_a_reset() {
        let mut turned_down = engine();
        turned_down.select_program(0);
        turned_down.control_change(0, CONTROL_VOLUME, 0.5);
        turned_down.control_change(0, CONTROL_EXPRESSION, 0.5);
        turned_down.reset();
        turned_down.note_on(0, 60, 100);
        let scaled = peak(&render(&mut turned_down, 12_000));
        let mut untouched = engine();
        untouched.select_program(0);
        untouched.note_on(0, 60, 100);
        let plain = peak(&render(&mut untouched, 12_000));
        assert!((scaled - plain * 0.25).abs() < 1e-5, "{scaled} vs {plain}");
    }

    #[test]
    fn brightness_and_the_operator_switches_reach_the_output() {
        let sounded = |controls: Controls| {
            let mut engine = engine_with(controls);
            engine.select_program(0);
            engine.note_on(0, 60, 100);
            render(&mut engine, 12_000)
        };
        let plain = sounded(Controls::default());
        let bright = sounded(Controls {
            brightness: 2.0,
            ..Controls::default()
        });
        assert_ne!(plain, bright);
        let silent = sounded(Controls {
            operators: 0,
            ..Controls::default()
        });
        assert_eq!(peak(&silent), 0.0, "no operator is heard");
    }

    #[test]
    fn stretching_the_envelopes_makes_a_struck_note_ring_longer() {
        // Half a second of a patch that decays to nothing on its own: the
        // slower its envelopes run, the more of that half second still has
        // sound in it. The exact factor is asserted on the envelope itself.
        let ringing = |time: f32| {
            let mut engine = engine_with(Controls {
                envelope_time: time,
                ..Controls::default()
            });
            engine.select_program(4); // RF MARIMBA.
            engine.note_on(0, 60, 100);
            rms(&render(&mut engine, 24_000))
        };
        let quick = ringing(1.0);
        let slow = ringing(4.0);
        assert!(quick > 0.0);
        assert!(slow > quick * 1.5, "stretched to {slow} from {quick}");
    }

    /// One note of a voice with the given controls, from silence.
    fn vibrato_render(controls: Controls, samples: usize) -> Vec<f32> {
        let mut voice = rf7_voice::factory_voice(0);
        // A program that asks for no vibrato of its own, but answers to it.
        voice.lfo.pitch_mod_depth = 0;
        voice.lfo.delay = 0;
        voice.lfo.speed = 70;
        voice.pitch_mod_sensitivity = 7;
        let mut engine = engine();
        engine.load_patch(voice);
        engine.set_controls(controls);
        engine.note_on(0, 60, 100);
        render(&mut engine, samples)
    }

    fn difference(left: &[f32], right: &[f32]) -> f32 {
        left.iter()
            .zip(right)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max)
    }

    #[test]
    fn the_performance_layer_adds_the_vibrato_the_program_never_asked_for() {
        let plain = vibrato_render(Controls::default(), 24_000);
        let vibrato = vibrato_render(
            Controls {
                lfo_depth: 1.0,
                ..Controls::default()
            },
            24_000,
        );
        assert!(
            difference(&plain, &vibrato) > 0.01,
            "the depth control moved nothing"
        );

        // The rate changes how fast it moves...
        let faster = vibrato_render(
            Controls {
                lfo_depth: 1.0,
                lfo_rate: 4.0,
                ..Controls::default()
            },
            24_000,
        );
        assert!(
            difference(&vibrato, &faster) > 0.01,
            "the rate moved nothing"
        );

        // ...and the delay holds it off, so the first fiftieth of a second is
        // the unmodulated note and only later does it part from it.
        let delayed = vibrato_render(
            Controls {
                lfo_depth: 1.0,
                lfo_delay: 0.25,
                ..Controls::default()
            },
            24_000,
        );
        assert_eq!(
            difference(&plain[..960], &delayed[..960]),
            0.0,
            "the delay did not hold the modulation off"
        );
        assert!(
            difference(&plain[..960], &vibrato[..960]) > 0.0,
            "without the delay it starts at once"
        );
        assert!(difference(&plain, &delayed) > 0.0, "and it does arrive");
    }

    /// An organ holds its level, so a rendered window can be read for pitch
    /// without the envelope moving under it.
    fn sustaining(controls: Controls) -> Engine {
        let mut engine = engine();
        engine.load_patch(rf7_voice::factory_voice(6));
        engine.set_controls(controls);
        engine
    }

    fn mono(portamento_time: f32) -> Engine {
        sustaining(Controls {
            voice_mode: VoiceMode::Mono,
            portamento_time,
            ..Controls::default()
        })
    }

    /// The rendered pitch of a window, in crossings per second.
    fn pitch(engine: &mut Engine, samples: usize) -> f32 {
        let rendered = render(engine, samples);
        crossings(&rendered) as f32 * 48_000.0 / samples as f32
    }

    #[test]
    fn mono_holds_one_voice_and_follows_the_last_key_down() {
        let mut engine = mono(0.0);
        engine.note_on(0, 60, 100);
        let middle = pitch(&mut engine, 12_000);
        assert_eq!(engine.active_voices(), 1);

        // A key played over the first takes the voice rather than adding one.
        engine.note_on(0, 72, 100);
        let above = pitch(&mut engine, 12_000);
        assert_eq!(engine.active_voices(), 1, "mono never sounds two notes");
        assert!(
            (above / middle - 2.0).abs() < 0.1,
            "an octave up read as {above} against {middle}"
        );

        // Releasing it hands the voice back to the key still down.
        engine.note_off(0, 72);
        let back = pitch(&mut engine, 12_000);
        assert_eq!(engine.active_voices(), 1);
        assert!(
            (back / middle - 1.0).abs() < 0.05,
            "the voice did not return to the held key: {back} against {middle}"
        );

        // And releasing the last key ends the note.
        engine.note_off(0, 60);
        render(&mut engine, 48_000);
        assert_eq!(engine.active_voices(), 0);
    }

    #[test]
    fn a_legato_note_does_not_start_the_envelopes_again() {
        // A struck voice is the one that shows it: its attack is loud and
        // its body is not.
        let mut engine = mono(0.0);
        engine.load_patch(rf7_voice::factory_voice(0));
        engine.note_on(0, 60, 100);
        let attack = peak(&render(&mut engine, 2_400));
        let body = peak(&render(&mut engine, 12_000));
        assert!(
            body < attack * 0.9,
            "the voice does not decay enough to tell"
        );

        // Held: the second note carries on from where the first was.
        engine.note_on(0, 64, 100);
        let legato = peak(&render(&mut engine, 2_400));
        assert!(
            legato < attack * 0.9,
            "a legato note struck again: {legato} against an attack of {attack}"
        );

        // Released first: the second note is a new one, and strikes.
        engine.note_off(0, 64);
        engine.note_off(0, 60);
        render(&mut engine, 48_000);
        engine.note_on(0, 64, 100);
        let struck = peak(&render(&mut engine, 2_400));
        assert!(
            struck > legato * 1.5,
            "a note after a release did not strike: {struck} against {legato}"
        );
    }

    #[test]
    fn portamento_slides_from_the_note_before_it_and_arrives() {
        // Slow enough that an octave takes about a second.
        let mut engine = mono(80.0);
        engine.note_on(0, 60, 100);
        let start = pitch(&mut engine, 12_000);
        engine.note_on(0, 72, 100);
        let sliding = pitch(&mut engine, 2_400);
        // At a time of 80 the octave takes about half a second, so the glide
        // is given a second before it is asked whether it arrived.
        render(&mut engine, 48_000);
        let arrived = pitch(&mut engine, 12_000);
        assert!(
            sliding < start * 1.4,
            "the glide jumped straight there: {sliding} against {start}"
        );
        assert!(
            (arrived / start - 2.0).abs() < 0.15,
            "the glide did not arrive: {arrived} against {start}"
        );

        // The switch on controller 65 turns it off, and then a note is where
        // it was played.
        let mut engine = mono(80.0);
        engine.note_on(0, 60, 100);
        let start = pitch(&mut engine, 12_000);
        engine.control_change(0, 65, 0.0);
        engine.note_on(0, 72, 100);
        let immediate = pitch(&mut engine, 2_400);
        assert!(
            (immediate / start - 2.0).abs() < 0.15,
            "the switch did not stop the glide: {immediate} against {start}"
        );

        // And a time of zero is the instrument as it was: no glide at all.
        let mut engine = mono(0.0);
        engine.note_on(0, 60, 100);
        let start = pitch(&mut engine, 12_000);
        engine.note_on(0, 72, 100);
        let immediate = pitch(&mut engine, 2_400);
        assert!((immediate / start - 2.0).abs() < 0.15);
    }

    #[test]
    fn portamento_glides_in_poly_too_and_a_glide_survives_the_stack() {
        let mut engine = sustaining(Controls {
            portamento_time: 80.0,
            ..Controls::default()
        });
        engine.note_on(0, 60, 100);
        let start = pitch(&mut engine, 12_000);
        engine.note_off(0, 60);
        render(&mut engine, 24_000);
        engine.note_on(0, 72, 100);
        let sliding = pitch(&mut engine, 2_400);
        assert!(
            sliding < start * 1.4,
            "a poly note did not glide from the one before it: {sliding} against {start}"
        );

        // The mono key stack is bounded, and the oldest key is the one that
        // goes when it overflows.
        let mut engine = mono(0.0);
        for note in 40..40 + MONO_STACK as u8 + 4 {
            engine.note_on(0, note, 100);
        }
        assert_eq!(engine.active_voices(), 1);
        for note in 40..40 + MONO_STACK as u8 + 3 {
            engine.note_off(0, note);
        }
        render(&mut engine, 4_800);
        assert_eq!(engine.active_voices(), 1, "the last key still holds it");
        engine.note_off(0, 40 + MONO_STACK as u8 + 3);
        render(&mut engine, 48_000);
        assert_eq!(engine.active_voices(), 0);
    }

    /// The loudest the attack gets, and what is left of it a second later
    /// while the key is still down.
    fn attack_and_body(name: &str) -> (f32, f32) {
        let index = (0..FACTORY_VOICES)
            .find(|index| rf7_voice::printable_name(&rf7_voice::factory_voice(*index).name) == name)
            .unwrap_or_else(|| panic!("the bank has no {name}"));
        let mut engine = engine();
        engine.load_patch(rf7_voice::factory_voice(index));
        engine.note_on(0, 60, 100);
        let attack = peak(&render(&mut engine, 24_000));
        let body = peak(&render(&mut engine, 24_000));
        let db = |value: f32| 20.0 * value.max(1e-9).log10();
        (db(attack), db(body))
    }

    /// A voice that is meant to hold a note has to hold it.
    ///
    /// Every sustained voice in the bank was once written to attack to 99 and
    /// then settle a long way under it, which is heard as a crescendo that
    /// breaks and drops away. The instrument's own cartridges do not do that:
    /// BRASS 1 falls 3, 1, 1, 1 and 7 points from its peak to the level it
    /// holds, PIPES 1 falls 1, 9, 9, 2, 6, 0. This is the shape those voices
    /// were reshaped to, and it is asserted so it cannot drift back.
    #[test]
    fn the_voices_that_should_hold_a_note_hold_it_and_the_struck_ones_do_not() {
        for name in [
            "RF BRASS",
            "RF ORGAN",
            "RF HORNS",
            "RF TRUMPET",
            "RF SAX",
            "RF STRINGS",
            "RF PAD",
            "RF CHOIR",
            "RF DRAWBAR",
            "RF PIPE",
            "RF LEAD",
            "RF SQUARE",
            "RF SUB",
        ] {
            let (attack, body) = attack_and_body(name);
            // The bank that this replaced fell 8 to 15 dB on four of these,
            // and BRASS 1 on the instrument's own cartridge falls under one.
            assert!(
                body >= attack - 7.0,
                "{name} swells and then collapses: {attack:.1} dB to {body:.1} dB"
            );
        }
        for name in [
            "RF TINES",
            "RF MARIMBA",
            "RF WOOD",
            "RF PIANO",
            "RF CLAV",
            "RF HARPSI",
            "RF KOTO",
        ] {
            let (attack, body) = attack_and_body(name);
            assert!(
                body <= attack - 20.0,
                "{name} is struck and should decay: {attack:.1} dB to {body:.1} dB"
            );
        }
    }

    #[test]
    fn the_lfo_layer_is_bounded_and_survives_a_program_change() {
        let wild = Controls {
            lfo_rate: f32::NAN,
            lfo_depth: 9.0,
            lfo_delay: -3.0,
            ..Controls::default()
        }
        .clamped();
        assert_eq!(wild.lfo_rate, 1.0);
        assert_eq!(wild.lfo_depth, LFO_DEPTH_MAX);
        assert_eq!(wild.lfo_delay, 0.0);

        // A program change rebuilds the LFO; the layer must still be on it.
        let mut layered = engine();
        layered.set_controls(Controls {
            lfo_depth: 1.0,
            lfo_rate: 4.0,
            ..Controls::default()
        });
        layered.select_program(3);
        layered.note_on(0, 60, 100);
        let with_layer = render(&mut layered, 24_000);
        let mut untouched = engine();
        untouched.select_program(3);
        untouched.note_on(0, 60, 100);
        assert!(difference(&with_layer, &render(&mut untouched, 24_000)) > 0.0);
    }

    #[test]
    fn a_draft_plays_without_entering_the_library() {
        let mut engine = engine();
        engine.select_program(3);
        let mut draft = rf7_voice::factory_voice(6);
        draft.name = *b"DRAFT     ";
        engine.load_patch(draft);
        assert_eq!(engine.program(), 3, "the selection is untouched");
        assert_eq!(
            engine.program_count(),
            FACTORY_VOICES,
            "and so is the library"
        );
        assert_eq!(engine.patch().name, *b"DRAFT     ");
        engine.note_on(0, 60, 100);
        assert!(peak(&render(&mut engine, 4_800)) > 0.0);
        // Selecting the program again restores it.
        engine.select_program(3);
        assert_eq!(engine.patch(), &rf7_voice::factory_voice(3));
    }

    #[test]
    fn gain_is_bounded_and_scales_the_output() {
        let mut engine = engine();
        assert!(!engine.set_gain(-0.1));
        assert!(!engine.set_gain(GAIN_MAX + 0.1));
        assert!(!engine.set_gain(f64::NAN));
        assert!(engine.set_gain(1.0));
        assert_eq!(engine.gain(), 1.0);
        engine.select_program(6);
        engine.note_on(0, 60, 100);
        let loud = peak(&render(&mut engine, 4_800));
        engine.reset();
        assert!(engine.set_gain(0.5));
        engine.note_on(0, 60, 100);
        let quiet = peak(&render(&mut engine, 4_800));
        assert!(
            (quiet * 2.0 - loud).abs() < loud * 0.02,
            "gain is not linear: {quiet} against {loud}"
        );
    }
}
