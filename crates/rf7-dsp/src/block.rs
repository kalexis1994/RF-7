//! The block: what the coordinator decides once, and what each voice does
//! with it.
//!
//! RackForge's `parallel_render_v1` splits a block into a serial pre-stage,
//! one job per voice the host may run on any of its threads, and a serial
//! post-stage. The engine is written in that shape whatever the host does
//! with it: the [`crate::Engine`] is the coordinator — MIDI, the controls,
//! the LFO, voice allocation, everything that decides — and a [`Unit`] is
//! one voice with its own operator tables, which never sees the engine.
//! Everything a voice needs in a block arrives by value, in two byte
//! regions:
//!
//! - the **block-shared payload**, one copy for every voice: the block's
//!   shape and, per frame, the [`Performance`] the coordinator computed —
//!   the pitch from the bend, the tuning and the LFO, the amplitude dip,
//!   the controllers' bias;
//! - a **dispatch payload** per voice: the commands the coordinator gave
//!   that voice this block, each at its frame — start this key with this
//!   patch, move to this note, release, stop.
//!
//! The sequential path and the pooled one run exactly this code, so they
//! render the same samples; the sum of the voices is taken in slot order
//! whatever order they finished in.

use crate::{
    OPERATORS, POLYPHONY,
    ops::Ops,
    voice::{Glide, Key, NoteVoice, Performance, VoiceSetup},
};
use rf7_voice::{PACKED_VOICE_LENGTH, Voice, decode_packed, encode_packed};

/// The longest block the engine renders.
pub const MAX_BLOCK_FRAMES: usize = 4096;
/// The shared payload's header: version, frames, sample rate, modulation
/// cycles, the operator mask, and a reserved word.
pub const SHARED_HEADER: usize = 24;
/// Per frame: the pitch in semitones, the amplitude dip and the bias.
pub const FRAME_BYTES: usize = 12;
/// The shared payload at its longest. A multiple of eight, as the host asks.
pub const SHARED_CAPACITY: usize = SHARED_HEADER + MAX_BLOCK_FRAMES * FRAME_BYTES;
/// Bytes per voice per block. Six fresh notes on one voice fit, which is
/// more retriggering than a block ever carries.
pub const DISPATCH_STRIDE: usize = 1024;
/// Commands one voice can be given in one block.
pub const COMMANDS_PER_BLOCK: usize = 8;
/// The wire version, in the shared header. A unit that sees another writes
/// silence rather than guessing.
const WIRE_VERSION: u32 = 1;

const KIND_START: u8 = 1;
const KIND_RETUNE: u8 = 2;
const KIND_RELEASE: u8 = 3;
const KIND_SILENCE: u8 = 4;
/// A glide from nowhere: `Glide::from` is `None`.
const NO_GLIDE_ORIGIN: u8 = 255;
/// The command header: the frame and the kind.
const COMMAND_HEADER: usize = 5;
const SETUP_BYTES: usize = 12;
const GLIDE_BYTES: usize = 5;
const KEY_BYTES: usize = 3;
const START_BYTES: usize =
    COMMAND_HEADER + KEY_BYTES + SETUP_BYTES + GLIDE_BYTES + PACKED_VOICE_LENGTH;
const RETUNE_BYTES: usize = COMMAND_HEADER + 1 + SETUP_BYTES + GLIDE_BYTES;
/// The dispatch header: the command count and padding.
const DISPATCH_HEADER: usize = 4;

/// What the coordinator tells one voice to do, at a frame.
#[derive(Clone, Copy, Debug)]
pub enum Command {
    /// Start this key, with this patch, under these controls, gliding from
    /// wherever the glide says.
    Start {
        key: Key,
        setup: VoiceSetup,
        glide: Glide,
        patch: Voice,
    },
    /// Move a sounding voice to another note without starting it again,
    /// under the controls of the moment.
    Retune {
        note: u8,
        setup: VoiceSetup,
        glide: Glide,
    },
    /// Let go of the key.
    Release,
    /// Stop at once, with no release.
    Silence,
}

#[derive(Clone, Copy, Debug)]
pub struct TimedCommand {
    pub frame: u32,
    pub command: Command,
}

/// The block's shape, as the shared header carries it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockShape {
    pub frames: u32,
    pub sample_rate: f32,
    /// Cycles of phase deviation per unit of modulator output.
    pub modulation: f32,
    /// Bit `i` set means OP(i+1) is heard.
    pub operators: u8,
}

fn write_u32(buffer: &mut [u8], at: usize, value: u32) {
    buffer[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_f32(buffer: &mut [u8], at: usize, value: f32) {
    buffer[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn read_u32(buffer: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(buffer.get(at..at + 4)?.try_into().ok()?))
}

fn read_f32(buffer: &[u8], at: usize) -> Option<f32> {
    Some(f32::from_le_bytes(buffer.get(at..at + 4)?.try_into().ok()?))
}

/// Writes the shared header. The per-frame performances follow, one
/// [`write_performance`] per frame.
pub fn write_shape(shared: &mut [u8], shape: BlockShape) {
    write_u32(shared, 0, WIRE_VERSION);
    write_u32(shared, 4, shape.frames);
    write_f32(shared, 8, shape.sample_rate);
    write_f32(shared, 12, shape.modulation);
    write_u32(shared, 16, u32::from(shape.operators));
    write_u32(shared, 20, 0);
}

pub fn write_performance(shared: &mut [u8], frame: usize, performance: &Performance) {
    let at = SHARED_HEADER + frame * FRAME_BYTES;
    write_f32(shared, at, performance.pitch);
    write_f32(shared, at + 4, performance.amplitude);
    write_f32(shared, at + 8, performance.bias);
}

/// The bytes a shared payload of this many frames occupies.
pub const fn shared_length(frames: usize) -> usize {
    SHARED_HEADER + frames * FRAME_BYTES
}

/// Reads the header, refusing another version, an impossible frame count,
/// or a payload shorter than its frames.
pub fn read_shape(shared: &[u8]) -> Option<BlockShape> {
    if read_u32(shared, 0)? != WIRE_VERSION {
        return None;
    }
    let frames = read_u32(shared, 4)?;
    if frames == 0
        || frames as usize > MAX_BLOCK_FRAMES
        || shared.len() < shared_length(frames as usize)
    {
        return None;
    }
    let sample_rate = read_f32(shared, 8)?;
    let modulation = read_f32(shared, 12)?;
    if !sample_rate.is_finite() || sample_rate <= 0.0 || !modulation.is_finite() {
        return None;
    }
    Some(BlockShape {
        frames,
        sample_rate,
        modulation,
        operators: (read_u32(shared, 16)? & 0xff) as u8,
    })
}

fn read_performance(shared: &[u8], frame: usize, shape: &BlockShape) -> Performance {
    let at = SHARED_HEADER + frame * FRAME_BYTES;
    // The header's length check guarantees the frame is there.
    Performance {
        pitch: read_f32(shared, at).unwrap_or(0.0),
        amplitude: read_f32(shared, at + 4).unwrap_or(0.0),
        bias: read_f32(shared, at + 8).unwrap_or(0.0),
        modulation: shape.modulation,
        operators: shape.operators,
    }
}

fn write_glide(buffer: &mut [u8], at: usize, glide: Glide) {
    buffer[at] = glide.from.unwrap_or(NO_GLIDE_ORIGIN);
    write_f32(buffer, at + 1, glide.semitones_per_second);
}

fn read_glide(buffer: &[u8], at: usize) -> Option<Glide> {
    let from = *buffer.get(at)?;
    Some(Glide {
        from: (from != NO_GLIDE_ORIGIN).then_some(from),
        semitones_per_second: read_f32(buffer, at + 1)?,
    })
}

/// Encodes a voice's commands for the block. Returns the bytes used, or
/// `None` when they do not fit the stride — the caller decides what to drop.
pub fn write_dispatch(buffer: &mut [u8], commands: &[TimedCommand]) -> Option<usize> {
    let mut at = DISPATCH_HEADER;
    if buffer.len() < at || commands.len() > COMMANDS_PER_BLOCK {
        return None;
    }
    for timed in commands {
        let needed = match timed.command {
            Command::Start { .. } => START_BYTES,
            Command::Retune { .. } => RETUNE_BYTES,
            Command::Release | Command::Silence => COMMAND_HEADER,
        };
        if buffer.len() < at + needed {
            return None;
        }
        write_u32(buffer, at, timed.frame);
        match timed.command {
            Command::Start {
                key,
                setup,
                glide,
                patch,
            } => {
                buffer[at + 4] = KIND_START;
                let mut cursor = at + COMMAND_HEADER;
                buffer[cursor] = key.channel;
                buffer[cursor + 1] = key.note;
                buffer[cursor + 2] = key.velocity;
                cursor += KEY_BYTES;
                write_u32(buffer, cursor, setup.transpose as u32);
                write_f32(buffer, cursor + 4, setup.envelope_time);
                write_f32(buffer, cursor + 8, setup.velocity_depth);
                cursor += SETUP_BYTES;
                write_glide(buffer, cursor, glide);
                cursor += GLIDE_BYTES;
                buffer[cursor..cursor + PACKED_VOICE_LENGTH]
                    .copy_from_slice(&encode_packed(&patch));
            }
            Command::Retune { note, setup, glide } => {
                buffer[at + 4] = KIND_RETUNE;
                let cursor = at + COMMAND_HEADER;
                buffer[cursor] = note;
                write_u32(buffer, cursor + 1, setup.transpose as u32);
                write_f32(buffer, cursor + 5, setup.envelope_time);
                write_f32(buffer, cursor + 9, setup.velocity_depth);
                write_glide(buffer, cursor + 1 + SETUP_BYTES, glide);
            }
            Command::Release => buffer[at + 4] = KIND_RELEASE,
            Command::Silence => buffer[at + 4] = KIND_SILENCE,
        }
        at += needed;
    }
    buffer[0] = commands.len() as u8;
    buffer[1..DISPATCH_HEADER].fill(0);
    Some(at)
}

/// Decodes a dispatch payload into `into`, returning how many commands it
/// held, or `None` for a malformed payload.
pub fn read_dispatch(
    payload: &[u8],
    into: &mut [TimedCommand; COMMANDS_PER_BLOCK],
) -> Option<usize> {
    if payload.is_empty() {
        return Some(0);
    }
    let count = usize::from(*payload.first()?);
    if count > COMMANDS_PER_BLOCK || payload.len() < DISPATCH_HEADER {
        return None;
    }
    let mut at = DISPATCH_HEADER;
    for slot in into.iter_mut().take(count) {
        let frame = read_u32(payload, at)?;
        let kind = *payload.get(at + 4)?;
        let cursor = at + COMMAND_HEADER;
        let command = match kind {
            KIND_START => {
                let key = Key {
                    channel: *payload.get(cursor)?,
                    note: *payload.get(cursor + 1)?,
                    velocity: *payload.get(cursor + 2)?,
                };
                let cursor = cursor + KEY_BYTES;
                let setup = VoiceSetup {
                    transpose: read_u32(payload, cursor)? as i32,
                    envelope_time: read_f32(payload, cursor + 4)?,
                    velocity_depth: read_f32(payload, cursor + 8)?,
                };
                let cursor = cursor + SETUP_BYTES;
                let glide = read_glide(payload, cursor)?;
                let cursor = cursor + GLIDE_BYTES;
                let packed: &[u8; PACKED_VOICE_LENGTH] = payload
                    .get(cursor..cursor + PACKED_VOICE_LENGTH)?
                    .try_into()
                    .ok()?;
                at += START_BYTES;
                Command::Start {
                    key,
                    setup,
                    glide,
                    patch: decode_packed(packed).voice,
                }
            }
            KIND_RETUNE => {
                let note = *payload.get(cursor)?;
                let setup = VoiceSetup {
                    transpose: read_u32(payload, cursor + 1)? as i32,
                    envelope_time: read_f32(payload, cursor + 5)?,
                    velocity_depth: read_f32(payload, cursor + 9)?,
                };
                let glide = read_glide(payload, cursor + 1 + SETUP_BYTES)?;
                at += RETUNE_BYTES;
                Command::Retune { note, setup, glide }
            }
            KIND_RELEASE => {
                at += COMMAND_HEADER;
                Command::Release
            }
            KIND_SILENCE => {
                at += COMMAND_HEADER;
                Command::Silence
            }
            _ => return None,
        };
        *slot = TimedCommand { frame, command };
    }
    Some(count)
}

/// One voice as the host schedules it: its note, its operator tables, and
/// the patch its note began with, which a retune reads.
///
/// A unit never sees the engine. Everything it needs each block is in its
/// dispatch payload and the shared payload, so it renders the same on the
/// coordinator's thread, on a host worker, or in an isolated instance.
#[derive(Clone, Debug)]
pub struct Unit {
    voice: NoteVoice,
    ops: Ops,
    patch: Voice,
}

impl Default for Unit {
    fn default() -> Self {
        Self {
            voice: NoteVoice::default(),
            ops: Ops::new(),
            patch: Voice::init(),
        }
    }
}

impl Unit {
    /// The note this voice is playing, for tests and tools.
    pub fn voice(&self) -> &NoteVoice {
        &self.voice
    }

    /// Renders the block into `output`, one sample per frame, applying
    /// each command at its frame. A payload the unit cannot read leaves
    /// silence and returns `false`; the voice is left as it was.
    pub fn render(&mut self, payload: &[u8], shared: &[u8], output: &mut [f32]) -> bool {
        let Some(shape) = read_shape(shared) else {
            output.fill(0.0);
            return false;
        };
        let frames = shape.frames as usize;
        let mut commands = [TimedCommand {
            frame: 0,
            command: Command::Release,
        }; COMMANDS_PER_BLOCK];
        let Some(count) = read_dispatch(payload, &mut commands) else {
            output.fill(0.0);
            return false;
        };
        if output.len() < frames || commands[..count].iter().any(|c| c.frame as usize >= frames) {
            output.fill(0.0);
            return false;
        }
        let mut next = 0;
        for (frame, sample) in output.iter_mut().enumerate().take(frames) {
            while next < count && commands[next].frame as usize <= frame {
                self.apply(&commands[next].command, shape.sample_rate);
                next += 1;
            }
            let performance = read_performance(shared, frame, &shape);
            *sample = self.voice.next_sample(&self.ops, &performance);
        }
        output[frames..].fill(0.0);
        true
    }

    fn apply(&mut self, command: &Command, sample_rate: f32) {
        match *command {
            Command::Start {
                key,
                setup,
                glide,
                patch,
            } => {
                self.patch = patch;
                self.voice.start(&patch, key, &setup, sample_rate, glide);
            }
            Command::Retune { note, setup, glide } => {
                self.voice
                    .retune(&self.patch, note, &setup, sample_rate, glide);
            }
            Command::Release => self.voice.release(),
            Command::Silence => self.voice.silence(),
        }
    }
}

/// The slot count the block contract plans for: one unit per voice.
pub const UNITS: usize = POLYPHONY;

#[allow(dead_code)]
const _: () = assert!(OPERATORS == 6);

#[cfg(test)]
mod tests {
    use super::*;
    use rf7_voice::factory_voice;

    #[test]
    fn the_shape_and_the_performances_round_trip() {
        let mut shared = [0_u8; shared_length(3)];
        let shape = BlockShape {
            frames: 3,
            sample_rate: 48_000.0,
            modulation: 2.0887,
            operators: 0b10_1011,
        };
        write_shape(&mut shared, shape);
        for frame in 0..3 {
            write_performance(
                &mut shared,
                frame,
                &Performance {
                    pitch: frame as f32 * 0.5,
                    amplitude: 0.25,
                    bias: 0.125,
                    modulation: 0.0,
                    operators: 0,
                },
            );
        }
        assert_eq!(read_shape(&shared), Some(shape));
        let read = read_performance(&shared, 2, &shape);
        assert_eq!(read.pitch, 1.0);
        assert_eq!(read.amplitude, 0.25);
        assert_eq!(read.bias, 0.125);
        assert_eq!(read.modulation, shape.modulation);
        assert_eq!(read.operators, shape.operators);
        // Another version, or a payload short of its frames, is refused.
        let mut other = shared;
        write_u32(&mut other, 0, WIRE_VERSION + 1);
        assert!(read_shape(&other).is_none());
        assert!(read_shape(&shared[..shared.len() - 1]).is_none());
    }

    #[test]
    fn commands_round_trip_with_their_patch() {
        let patch = factory_voice(7);
        let commands = [
            TimedCommand {
                frame: 3,
                command: Command::Start {
                    key: Key {
                        channel: 2,
                        note: 60,
                        velocity: 100,
                    },
                    setup: VoiceSetup {
                        transpose: -12,
                        envelope_time: 1.5,
                        velocity_depth: 0.5,
                    },
                    glide: Glide {
                        from: Some(48),
                        semitones_per_second: 30.0,
                    },
                    patch,
                },
            },
            TimedCommand {
                frame: 10,
                command: Command::Retune {
                    note: 62,
                    setup: VoiceSetup {
                        transpose: 5,
                        ..VoiceSetup::default()
                    },
                    glide: Glide::default(),
                },
            },
            TimedCommand {
                frame: 20,
                command: Command::Release,
            },
            TimedCommand {
                frame: 21,
                command: Command::Silence,
            },
        ];
        let mut buffer = [0_u8; DISPATCH_STRIDE];
        let length = write_dispatch(&mut buffer, &commands).expect("four commands fit");
        assert_eq!(
            length,
            DISPATCH_HEADER + START_BYTES + RETUNE_BYTES + 2 * COMMAND_HEADER
        );
        let mut decoded = [TimedCommand {
            frame: 0,
            command: Command::Release,
        }; COMMANDS_PER_BLOCK];
        assert_eq!(read_dispatch(&buffer[..length], &mut decoded), Some(4));
        match decoded[0].command {
            Command::Start {
                key,
                setup,
                glide,
                patch: read,
            } => {
                assert_eq!(decoded[0].frame, 3);
                assert_eq!((key.channel, key.note, key.velocity), (2, 60, 100));
                assert_eq!(setup.transpose, -12);
                assert_eq!(setup.envelope_time, 1.5);
                assert_eq!(setup.velocity_depth, 0.5);
                assert_eq!(glide.from, Some(48));
                assert_eq!(glide.semitones_per_second, 30.0);
                assert_eq!(read, patch);
            }
            _ => panic!("the start came back as something else"),
        }
        match decoded[1].command {
            Command::Retune { note, setup, glide } => {
                assert_eq!((decoded[1].frame, note), (10, 62));
                assert_eq!(setup.transpose, 5);
                assert_eq!(glide.from, None);
            }
            _ => panic!("the retune came back as something else"),
        }
        assert!(matches!(decoded[2].command, Command::Release));
        assert!(matches!(decoded[3].command, Command::Silence));
        // An empty payload is no commands; a truncated one is refused.
        assert_eq!(read_dispatch(&[], &mut decoded), Some(0));
        assert_eq!(read_dispatch(&buffer[..length - 1], &mut decoded), None);
    }

    #[test]
    fn six_fresh_notes_fit_one_voice_and_a_seventh_does_not() {
        let start = TimedCommand {
            frame: 0,
            command: Command::Start {
                key: Key::default(),
                setup: VoiceSetup::default(),
                glide: Glide::default(),
                patch: Voice::init(),
            },
        };
        let mut buffer = [0_u8; DISPATCH_STRIDE];
        assert!(write_dispatch(&mut buffer, &[start; 6]).is_some());
        assert!(write_dispatch(&mut buffer, &[start; 7]).is_none());
    }

    #[test]
    fn a_unit_plays_its_commands_at_their_frames() {
        let mut shared = [0_u8; shared_length(64)];
        write_shape(
            &mut shared,
            BlockShape {
                frames: 64,
                sample_rate: 48_000.0,
                modulation: crate::MODULATION_CYCLES,
                operators: crate::ALL_OPERATORS,
            },
        );
        for frame in 0..64 {
            write_performance(&mut shared, frame, &Performance::default());
        }
        let commands = [TimedCommand {
            frame: 16,
            command: Command::Start {
                key: Key {
                    channel: 0,
                    note: 69,
                    velocity: 100,
                },
                setup: VoiceSetup::default(),
                glide: Glide::default(),
                patch: factory_voice(0),
            },
        }];
        let mut payload = [0_u8; DISPATCH_STRIDE];
        let length = write_dispatch(&mut payload, &commands).unwrap();
        let mut unit = Unit::default();
        let mut output = [1.0_f32; 64];
        assert!(unit.render(&payload[..length], &shared, &mut output));
        assert!(
            output[..16].iter().all(|s| *s == 0.0),
            "silent before the key"
        );
        assert!(output[16..].iter().any(|s| *s != 0.0), "sounding after it");
        assert!(unit.voice().is_active());
        // A shared payload it cannot read leaves silence and the voice alone.
        let mut output = [1.0_f32; 64];
        assert!(!unit.render(&[], &shared[..8], &mut output));
        assert!(output.iter().all(|s| *s == 0.0));
        assert!(unit.voice().is_active());
    }
}
