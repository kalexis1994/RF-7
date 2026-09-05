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
pub use tables::LEVEL_FULL;
pub use voice::{MODULATION_CYCLES, NoteVoice, Performance, VoiceSetup};

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
/// Every operator heard.
pub const ALL_OPERATORS: u8 = (1 << OPERATORS) - 1;

/// Chosen against the loudest patches on a real cartridge rather than against
/// this project's own quieter factory voices: a four-note chord of a
/// four-carrier patch lands near −3 dBFS. Sixteen voices still sum past full
/// scale, and nothing here normalises that away behind the user.
pub const DEFAULT_GAIN: f64 = 0.3;
pub const GAIN_MAX: f64 = 2.0;

const CONTROL_MODULATION: u8 = 1;
const CONTROL_SUSTAIN: u8 = 64;
const CONTROL_ALL_SOUND_OFF: u8 = 120;
const CONTROL_ALL_NOTES_OFF: u8 = 123;

/// What a wheel or a pressure sensor is wired to.
///
/// The DX7 assigns each of its controllers to pitch, amplitude or the envelope
/// bias, independently. RF-7 offers the first two, which is what the wheel is
/// used for in practice.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Target {
    #[default]
    Pitch,
    Amplitude,
    Both,
}

impl Target {
    /// From the parameter schema's enum value.
    pub const fn from_index(index: u32) -> Self {
        match index {
            1 => Self::Amplitude,
            2 => Self::Both,
            _ => Self::Pitch,
        }
    }

    pub const fn index(self) -> u32 {
        match self {
            Self::Pitch => 0,
            Self::Amplitude => 1,
            Self::Both => 2,
        }
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
    /// Scales the modulation depth of every operator. 1.0 is the patch's own.
    pub brightness: f32,
    pub envelope_time: f32,
    pub velocity_depth: f32,
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
            brightness: 1.0,
            envelope_time: 1.0,
            velocity_depth: 1.0,
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
            brightness: finite(self.brightness, 0.0, BRIGHTNESS_MAX, 1.0),
            envelope_time: finite(
                self.envelope_time,
                ENVELOPE_TIME_MIN,
                ENVELOPE_TIME_MAX,
                1.0,
            ),
            velocity_depth: finite(self.velocity_depth, 0.0, VELOCITY_DEPTH_MAX, 1.0),
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
    /// One bit per MIDI channel.
    pedals: u16,
    /// Semitones of LFO pitch modulation at full depth.
    pitch_depth: f32,
    /// How much amplitude modulation the patch asks for, 0.0..=1.0.
    amp_depth: f32,
    /// Notes that arrived with every voice already busy.
    stolen: u64,
}

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
            pedals: 0,
            pitch_depth: 0.0,
            amp_depth: 0.0,
            stolen: 0,
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

    fn apply_patch(&mut self) {
        self.lfo = Lfo::new(&self.patch.lfo, self.sample_rate);
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
        let slot = self.allocate();
        let setup = self.controls.setup();
        self.voices[slot].start(
            &self.patch,
            channel,
            note,
            velocity,
            &setup,
            self.sample_rate,
        );
        self.sustained[slot] = false;
    }

    pub fn note_off(&mut self, channel: u8, note: u8) {
        let held = self.pedals & (1 << (channel & 15)) != 0;
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

    /// `value` is normalised to 0.0..=1.0 by the caller, which is what lets a
    /// seven-bit controller and a thirty-two-bit one arrive the same way.
    pub fn control_change(&mut self, channel: u8, controller: u8, value: f64) {
        let value = value.clamp(0.0, 1.0);
        match controller {
            CONTROL_MODULATION => self.wheel = value as f32,
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
            CONTROL_ALL_NOTES_OFF => {
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
        self.stolen = 0;
        self.lfo = Lfo::new(&self.patch.lfo, self.sample_rate);
    }

    pub fn active_voices(&self) -> usize {
        self.voices.iter().filter(|voice| voice.is_active()).count()
    }

    pub fn next_sample(&mut self) -> f32 {
        let modulation = self.lfo.advance();
        // Both controllers open the same two destinations, so they are summed
        // per destination rather than each fighting for the whole depth.
        let wheel = self.wheel * self.controls.wheel_range;
        let pressure = self.pressure * self.controls.aftertouch_range;
        let mut added_pitch = 0.0;
        let mut added_amplitude = 0.0;
        for (amount, target) in [
            (wheel, self.controls.wheel_target),
            (pressure, self.controls.aftertouch_target),
        ] {
            if target.moves_pitch() {
                added_pitch += amount;
            }
            if target.moves_amplitude() {
                added_amplitude += amount;
            }
        }
        let pitch_depth =
            (f32::from(self.patch.lfo.pitch_mod_depth.min(99)) / 99.0 + added_pitch).min(1.0);
        let amplitude_depth = (self.amp_depth + added_amplitude).min(1.0);
        let performance = Performance {
            pitch: self.bend_position * self.controls.bend_semitones
                + self.controls.master_tune_cents / 100.0
                + modulation * self.pitch_depth * pitch_depth,
            // Amplitude modulation only ever takes level away, so the LFO is
            // read as a unipolar dip rather than a swing either side of it.
            amplitude: amplitude_depth * (1.0 - modulation) * 0.5,
            modulation: MODULATION_CYCLES * self.controls.brightness,
            operators: self.controls.operators,
        };
        let mut sum = 0.0;
        for voice in &mut self.voices {
            sum += voice.next_sample(&self.sine, &performance);
        }
        sum * self.gain as f32
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
