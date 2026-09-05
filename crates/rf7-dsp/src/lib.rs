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
pub use voice::{MODULATION_CYCLES, NoteVoice};

use lfo::Lfo;
use rf7_voice::{Cartridge, VOICES_PER_CARTRIDGE, Voice, factory_cartridge, printable_name};

/// Six operators, as on the instrument. Not a parameter.
pub const OPERATORS: usize = 6;
/// Sixteen notes, as on the instrument.
pub const POLYPHONY: usize = 16;

pub const SAMPLE_RATE_MIN: f32 = 8_000.0;
pub const SAMPLE_RATE_MAX: f32 = 192_000.0;

/// Pitch bend range. The DX7 keeps this outside the voice data, among its
/// function parameters, so it belongs to the engine and not to a patch.
pub const BEND_SEMITONES: f32 = 2.0;

/// Conservative by default: six carriers across sixteen voices can sum well
/// past full scale, and nothing here normalises that away behind the user.
const DEFAULT_GAIN: f64 = 0.2;
pub const GAIN_MAX: f64 = 2.0;

const CONTROL_MODULATION: u8 = 1;
const CONTROL_SUSTAIN: u8 = 64;
const CONTROL_ALL_SOUND_OFF: u8 = 120;
const CONTROL_ALL_NOTES_OFF: u8 = 123;

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
    cartridge: Cartridge,
    program: usize,
    patch: Voice,
    lfo: Lfo,
    sample_rate: f32,
    gain: f64,
    /// Semitones, from the pitch wheel.
    bend: f32,
    /// Modulation wheel, 0.0..=1.0.
    wheel: f32,
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
        let cartridge = factory_cartridge();
        let patch = *cartridge.voice(0).expect("a cartridge has 32 voices");
        let mut engine = Self {
            voices: [NoteVoice::default(); POLYPHONY],
            sustained: [false; POLYPHONY],
            sine: Sine::new(),
            cartridge,
            program: 0,
            patch,
            lfo: Lfo::new(&patch.lfo, sample_rate),
            sample_rate,
            gain: DEFAULT_GAIN,
            bend: 0.0,
            wheel: 0.0,
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

    /// Replace the whole cartridge. Sounding notes keep the patch they began
    /// with, which is what lets a program change land mid-phrase.
    pub fn load_cartridge(&mut self, cartridge: Cartridge) {
        self.cartridge = cartridge;
        self.select_program(self.program.min(VOICES_PER_CARTRIDGE - 1));
    }

    pub fn program_count(&self) -> usize {
        VOICES_PER_CARTRIDGE
    }

    pub fn program(&self) -> usize {
        self.program
    }

    pub fn program_name(&self, index: usize) -> &str {
        self.cartridge
            .voice(index)
            .map_or("", |voice| printable_name(&voice.name))
    }

    pub fn select_program(&mut self, index: usize) -> bool {
        let Some(voice) = self.cartridge.voice(index) else {
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
        self.voices[slot].start(&self.patch, channel, note, velocity, self.sample_rate);
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
        self.bend = value.clamp(-1.0, 1.0) as f32 * BEND_SEMITONES;
    }

    /// Stop everything at once, with no release segment.
    pub fn reset(&mut self) {
        for voice in &mut self.voices {
            voice.silence();
        }
        self.sustained = [false; POLYPHONY];
        self.pedals = 0;
        self.bend = 0.0;
        self.wheel = 0.0;
        self.stolen = 0;
        self.lfo = Lfo::new(&self.patch.lfo, self.sample_rate);
    }

    pub fn active_voices(&self) -> usize {
        self.voices.iter().filter(|voice| voice.is_active()).count()
    }

    pub fn next_sample(&mut self) -> f32 {
        let modulation = self.lfo.advance();
        let depth =
            (f32::from(self.patch.lfo.pitch_mod_depth.min(99)) / 99.0 + self.wheel).min(1.0);
        let pitch = self.bend + modulation * self.pitch_depth * depth;
        // Amplitude modulation only ever takes level away, so the LFO is read
        // as a unipolar dip rather than as a swing either side of the level.
        let amplitude = self.amp_depth * (1.0 - modulation) * 0.5;
        let mut sum = 0.0;
        for voice in &mut self.voices {
            sum += voice.next_sample(&self.sine, pitch, amplitude);
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
    use rf7_voice::{decode_bulk_dump, encode_bulk_dump};

    fn engine() -> Engine {
        Engine::new(48_000.0).expect("48 kHz is a supported rate")
    }

    fn render(engine: &mut Engine, samples: usize) -> Vec<f32> {
        (0..samples).map(|_| engine.next_sample()).collect()
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0f32, |peak, s| peak.max(s.abs()))
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
        for program in 0..8 {
            let mut engine = engine();
            assert!(engine.select_program(program));
            assert!(!engine.program_name(program).is_empty());
            engine.note_on(0, 57, 100);
            engine.note_on(0, 64, 90);
            let rendered = render(&mut engine, 24_000);
            assert!(peak(&rendered) > 0.001, "program {program} is silent");
            assert!(rendered.iter().all(|sample| sample.is_finite()));
        }
        assert!(!engine().select_program(VOICES_PER_CARTRIDGE));
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
        engine.load_cartridge(decode_bulk_dump(&dump).expect("our own dump decodes"));
        assert_eq!(engine.program_name(0), "LOADED");
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
