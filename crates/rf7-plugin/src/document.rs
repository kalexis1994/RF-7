//! The program document: a voice as RackForge stores it, and the envelopes
//! the host exchanges around it.
//!
//! The payload is the voice written out in words — algorithm 1..32, six named
//! operators, curves and waveforms as names — because a document a user can
//! find on disk should be one they can read. It round-trips exactly; every
//! field is clamped on the way in, so a document edited by hand cannot put an
//! out-of-range value into the engine.
//!
//! The host-side envelopes are mirrored here with the fields the host
//! validates and nothing more: it denies unknown fields, so an extra one would
//! be a rejected save.

use rf7_voice::{
    Cartridge, Curve, LfoWaveform, NAME_LENGTH, OPERATORS, Operator, VOICES_PER_CARTRIDGE, Voice,
    encode_bulk_dump, encode_voice_dump, printable_name,
};
use serde::{Deserialize, Serialize};

pub const PROGRAM_SCHEMA_VERSION: u32 = 1;
pub const PROGRAM_EDIT_SCHEMA_VERSION: u32 = 1;
pub const PROGRAM_EDITOR_SCHEMA_VERSION: u32 = 1;
pub const PAYLOAD_VERSION: u32 = 1;
pub const PLUGIN_ID: &str = "org.rackforge.rf7";
const PAYLOAD_FORMAT: &str = "rf7-voice";

/// The voice, in words.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoicePayload {
    pub format: String,
    pub version: u32,
    pub name: String,
    /// 1..=32, as the panel numbers them.
    pub algorithm: u8,
    pub feedback: u8,
    pub oscillator_sync: bool,
    /// -24..=24 semitones; the cartridge byte minus 24.
    pub transpose: i8,
    pub pitch_mod_sensitivity: u8,
    pub pitch_eg: EnvelopePayload,
    pub lfo: LfoPayload,
    pub operators: Vec<OperatorPayload>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvelopePayload {
    pub rates: [u8; 4],
    pub levels: [u8; 4],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LfoPayload {
    pub speed: u8,
    pub delay: u8,
    pub pitch_mod_depth: u8,
    pub amp_mod_depth: u8,
    pub sync: bool,
    pub waveform: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperatorPayload {
    pub envelope: EnvelopePayload,
    pub output_level: u8,
    pub fixed_frequency: bool,
    pub coarse: u8,
    pub fine: u8,
    /// -7..=7; the cartridge byte minus 7.
    pub detune: i8,
    pub break_point: u8,
    pub left_depth: u8,
    pub right_depth: u8,
    pub left_curve: String,
    pub right_curve: String,
    pub rate_scaling: u8,
    pub amp_mod_sensitivity: u8,
    pub velocity_sensitivity: u8,
}

pub const CURVE_NAMES: [&str; 4] = ["neg-lin", "neg-exp", "pos-exp", "pos-lin"];
pub const WAVEFORM_NAMES: [&str; 6] = [
    "triangle",
    "saw-down",
    "saw-up",
    "square",
    "sine",
    "sample-hold",
];

pub fn curve_name(curve: Curve) -> &'static str {
    match curve {
        Curve::NegativeLinear => CURVE_NAMES[0],
        Curve::NegativeExponential => CURVE_NAMES[1],
        Curve::PositiveExponential => CURVE_NAMES[2],
        Curve::PositiveLinear => CURVE_NAMES[3],
    }
}

pub fn curve_index(name: &str) -> Option<u8> {
    CURVE_NAMES
        .iter()
        .position(|candidate| *candidate == name)
        .map(|index| index as u8)
}

pub fn waveform_name(waveform: LfoWaveform) -> &'static str {
    match waveform {
        LfoWaveform::Triangle => WAVEFORM_NAMES[0],
        LfoWaveform::SawDown => WAVEFORM_NAMES[1],
        LfoWaveform::SawUp => WAVEFORM_NAMES[2],
        LfoWaveform::Square => WAVEFORM_NAMES[3],
        LfoWaveform::Sine => WAVEFORM_NAMES[4],
        LfoWaveform::SampleAndHold => WAVEFORM_NAMES[5],
    }
}

pub fn waveform_index(name: &str) -> Option<u8> {
    WAVEFORM_NAMES
        .iter()
        .position(|candidate| *candidate == name)
        .map(|index| index as u8)
}

impl VoicePayload {
    pub fn from_voice(voice: &Voice) -> Self {
        Self {
            format: PAYLOAD_FORMAT.to_owned(),
            version: PAYLOAD_VERSION,
            name: printable_name(&voice.name).to_owned(),
            algorithm: voice.algorithm + 1,
            feedback: voice.feedback,
            oscillator_sync: voice.oscillator_sync,
            transpose: i8::try_from(i16::from(voice.transpose) - 24).unwrap_or(0),
            pitch_mod_sensitivity: voice.pitch_mod_sensitivity,
            pitch_eg: EnvelopePayload {
                rates: voice.pitch_eg_rate,
                levels: voice.pitch_eg_level,
            },
            lfo: LfoPayload {
                speed: voice.lfo.speed,
                delay: voice.lfo.delay,
                pitch_mod_depth: voice.lfo.pitch_mod_depth,
                amp_mod_depth: voice.lfo.amp_mod_depth,
                sync: voice.lfo.sync,
                waveform: waveform_name(voice.lfo.shape()).to_owned(),
            },
            operators: voice
                .operators
                .iter()
                .map(|operator| OperatorPayload {
                    envelope: EnvelopePayload {
                        rates: operator.eg_rate,
                        levels: operator.eg_level,
                    },
                    output_level: operator.output_level,
                    fixed_frequency: operator.fixed_frequency,
                    coarse: operator.coarse,
                    fine: operator.fine,
                    detune: i8::try_from(i16::from(operator.detune) - 7).unwrap_or(0),
                    break_point: operator.break_point,
                    left_depth: operator.left_depth,
                    right_depth: operator.right_depth,
                    left_curve: curve_name(operator.left()).to_owned(),
                    right_curve: curve_name(operator.right()).to_owned(),
                    rate_scaling: operator.rate_scaling,
                    amp_mod_sensitivity: operator.amp_mod_sensitivity,
                    velocity_sensitivity: operator.velocity_sensitivity,
                })
                .collect(),
        }
    }

    /// Back to a voice, clamping everything: a document a person edited by
    /// hand can hold anything, and the engine must not.
    pub fn to_voice(&self) -> Option<Voice> {
        if self.format != PAYLOAD_FORMAT
            || self.version != PAYLOAD_VERSION
            || self.operators.len() != OPERATORS
        {
            return None;
        }
        let mut voice = Voice::init();
        voice.name = pad_name(&self.name);
        voice.algorithm = self.algorithm.saturating_sub(1);
        voice.feedback = self.feedback;
        voice.oscillator_sync = self.oscillator_sync;
        voice.transpose = (i16::from(self.transpose.clamp(-24, 24)) + 24) as u8;
        voice.pitch_mod_sensitivity = self.pitch_mod_sensitivity;
        voice.pitch_eg_rate = self.pitch_eg.rates;
        voice.pitch_eg_level = self.pitch_eg.levels;
        voice.lfo.speed = self.lfo.speed;
        voice.lfo.delay = self.lfo.delay;
        voice.lfo.pitch_mod_depth = self.lfo.pitch_mod_depth;
        voice.lfo.amp_mod_depth = self.lfo.amp_mod_depth;
        voice.lfo.sync = self.lfo.sync;
        voice.lfo.waveform = waveform_index(&self.lfo.waveform)?;
        for (operator, payload) in voice.operators.iter_mut().zip(&self.operators) {
            *operator = Operator {
                eg_rate: payload.envelope.rates,
                eg_level: payload.envelope.levels,
                break_point: payload.break_point,
                left_depth: payload.left_depth,
                right_depth: payload.right_depth,
                left_curve: curve_index(&payload.left_curve)?,
                right_curve: curve_index(&payload.right_curve)?,
                rate_scaling: payload.rate_scaling,
                amp_mod_sensitivity: payload.amp_mod_sensitivity,
                velocity_sensitivity: payload.velocity_sensitivity,
                output_level: payload.output_level,
                fixed_frequency: payload.fixed_frequency,
                coarse: payload.coarse,
                fine: payload.fine,
                detune: (i16::from(payload.detune.clamp(-7, 7)) + 7) as u8,
            };
        }
        voice.clamp();
        Some(voice)
    }
}

/// Ten bytes of printable ASCII, space-padded, anything else replaced.
pub fn pad_name(name: &str) -> [u8; NAME_LENGTH] {
    let mut padded = [b' '; NAME_LENGTH];
    for (slot, byte) in padded.iter_mut().zip(name.bytes()) {
        *slot = if (0x20..0x7f).contains(&byte) {
            byte
        } else {
            b' '
        };
    }
    padded
}

// ---- the host's envelopes, mirrored ------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProgramDocument {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub plugin_id: String,
    pub plugin_version: String,
    pub plugin_state_version: u32,
    pub payload_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    pub payload: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProgramEditRequest {
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProgramArtifact {
    pub storage_path: String,
    pub media_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PreparedProgram {
    pub schema_version: u32,
    pub storage_path: String,
    pub preview_sound_id: String,
    pub document: ProgramDocument,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<ProgramArtifact>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum EditorValue {
    Inherited,
    Boolean(bool),
    Integer(i64),
    Choice(String),
    SoundId(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProgramFieldEditRequest {
    pub schema_version: u32,
    pub document: ProgramDocument,
    pub field_id: String,
    pub value: EditorValue,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EditorChoice {
    pub value: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EditorFieldKind {
    Toggle,
    Number {
        minimum: i64,
        maximum: i64,
        step: i64,
        decimals: u8,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        unit: Option<String>,
        allow_inherited: bool,
    },
    Choice {
        options: Vec<EditorChoice>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EditorField {
    pub id: String,
    pub label: String,
    pub detail: String,
    pub value: EditorValue,
    pub kind: EditorFieldKind,
    pub live_preview: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EditorPage {
    pub id: String,
    pub label: String,
    pub detail: String,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pages: Vec<EditorPage>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<EditorField>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EditorView {
    pub schema_version: u32,
    pub title: String,
    pub pages: Vec<EditorPage>,
}

/// A voice as a complete document, named after itself.
pub fn document(id: &str, voice: &Voice) -> ProgramDocument {
    let name = printable_name(&voice.name);
    document_with_id_and_name(id, if name.is_empty() { "RF EDIT" } else { name }, voice)
}

/// A voice as a complete document under a name the user chose, which may be
/// longer than the ten bytes the voice itself carries.
pub fn document_with_id_and_name(id: &str, name: &str, voice: &Voice) -> ProgramDocument {
    let payload = VoicePayload::from_voice(voice);
    let name = name.trim();
    let name = if name.is_empty() {
        "RF EDIT".to_owned()
    } else {
        name.to_owned()
    };
    ProgramDocument {
        schema_version: PROGRAM_SCHEMA_VERSION,
        id: id.to_owned(),
        name,
        plugin_id: PLUGIN_ID.to_owned(),
        plugin_version: env!("CARGO_PKG_VERSION").to_owned(),
        plugin_state_version: crate::STATE_VERSION,
        payload_version: PAYLOAD_VERSION,
        category: Some("FM".to_owned()),
        tags: Vec::new(),
        payload: serde_json::to_value(payload).expect("a payload of plain fields serialises"),
    }
}

/// The voice inside a document, if it is one of ours.
pub fn voice_of(document: &ProgramDocument) -> Option<Voice> {
    if document.plugin_id != PLUGIN_ID || document.payload_version != PAYLOAD_VERSION {
        return None;
    }
    let payload: VoicePayload = serde_json::from_value(document.payload.clone()).ok()?;
    payload.to_voice()
}

/// The media type of a DX7 System Exclusive file.
const SYSEX_MEDIA_TYPE: &str = "application/x-yamaha-dx7-sysex";

/// A library as the bulk dumps a DX7 accepts: thirty-two voices to a bank,
/// the last bank padded with INIT VOICE, each written as
/// `exports/<stem>-<n>.syx`. This is how RF-7 exports: the host keeps every
/// artifact of a save in the plugin's own folder, so the banks are written
/// afresh beside every saved program and are there on disk to be taken.
pub fn export_artifacts(stem: &str, voices: &[Voice]) -> Vec<ProgramArtifact> {
    voices
        .chunks(VOICES_PER_CARTRIDGE)
        .enumerate()
        .map(|(index, chunk)| {
            let mut bank = [Voice::init(); VOICES_PER_CARTRIDGE];
            bank[..chunk.len()].copy_from_slice(chunk);
            ProgramArtifact {
                storage_path: format!("exports/{stem}-{}.syx", index + 1),
                media_type: SYSEX_MEDIA_TYPE.to_owned(),
                bytes: encode_bulk_dump(&Cartridge::from_voices(bank), 0).to_vec(),
            }
        })
        .collect()
}

/// What the host commits: the document, where it goes, what to preview, and
/// the voice as a single-voice System Exclusive dump beside it — the same
/// bytes a DX7 would accept, so a program edited here can go to hardware.
pub fn prepared(
    document: ProgramDocument,
    preview_sound_id: &str,
    voice: &Voice,
) -> PreparedProgram {
    let stem = document.id.replace('.', "-");
    PreparedProgram {
        schema_version: PROGRAM_EDIT_SCHEMA_VERSION,
        storage_path: format!("programs/{stem}.json"),
        preview_sound_id: preview_sound_id.to_owned(),
        artifacts: vec![ProgramArtifact {
            storage_path: format!("programs/{stem}.syx"),
            media_type: SYSEX_MEDIA_TYPE.to_owned(),
            bytes: encode_voice_dump(voice, 0).to_vec(),
        }],
        document,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rf7_voice::{FACTORY_VOICES, factory_voice};

    #[test]
    fn every_factory_voice_survives_the_document_round_trip() {
        for index in 0..FACTORY_VOICES {
            let voice = factory_voice(index);
            let payload = VoicePayload::from_voice(&voice);
            let json = serde_json::to_string(&payload).unwrap();
            let back: VoicePayload = serde_json::from_str(&json).unwrap();
            assert_eq!(back.to_voice(), Some(voice), "voice {index}");
        }
    }

    #[test]
    fn the_payload_reads_like_a_patch_sheet() {
        let json = serde_json::to_string(&VoicePayload::from_voice(&factory_voice(0))).unwrap();
        assert!(json.contains("\"algorithm\":5"), "{json}");
        assert!(json.contains("\"name\":\"RF TINES\""));
        assert!(json.contains("\"left_curve\":\"neg-lin\""));
        assert!(json.contains("\"waveform\":\"triangle\""));
        assert!(json.contains("\"transpose\":0"));
    }

    #[test]
    fn a_hand_edited_payload_is_clamped_not_trusted() {
        let mut payload = VoicePayload::from_voice(&factory_voice(0));
        payload.operators[0].output_level = 200;
        payload.feedback = 9;
        payload.transpose = 90;
        payload.operators[1].detune = -50;
        let voice = payload.to_voice().expect("still a voice");
        assert_eq!(voice.operators[0].output_level, 99);
        assert_eq!(voice.feedback, 7);
        assert_eq!(voice.transpose, 48);
        assert_eq!(voice.operators[1].detune, 0);
        payload.lfo.waveform = "trapezoid".into();
        assert_eq!(
            payload.to_voice(),
            None,
            "an unknown name is not guessed at"
        );
    }

    #[test]
    fn a_document_carries_the_voice_and_its_dump() {
        let voice = factory_voice(3);
        let prepared = prepared(document("user.rf7-001", &voice), "program-004", &voice);
        assert_eq!(prepared.document.name, "RF BRASS");
        assert_eq!(prepared.document.plugin_id, PLUGIN_ID);
        assert_eq!(prepared.storage_path, "programs/user-rf7-001.json");
        assert_eq!(prepared.artifacts.len(), 1);
        assert_eq!(
            prepared.artifacts[0].storage_path,
            "programs/user-rf7-001.syx"
        );
        assert_eq!(prepared.artifacts[0].bytes.len(), 163);
        assert_eq!(voice_of(&prepared.document), Some(voice));
        // Serialised and back, as the host will do it.
        let json = serde_json::to_string(&prepared).unwrap();
        let back: PreparedProgram = serde_json::from_str(&json).unwrap();
        assert_eq!(back, prepared);
    }

    #[test]
    fn another_plugins_document_is_not_ours() {
        let mut other = document("user.x", &factory_voice(0));
        other.plugin_id = "org.example.other".into();
        assert_eq!(voice_of(&other), None);
        let mut future = document("user.y", &factory_voice(0));
        future.payload_version = 99;
        assert_eq!(voice_of(&future), None);
    }
}
