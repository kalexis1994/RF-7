//! WAV in and out.
//!
//! Written: mono IEEE float, with no normalisation and no clipping — a report
//! that says the peak was 1.4 is more useful than a file quietly brought back
//! to 1.0. Read: whatever a recorder produces, because the recordings that
//! matter most here come from someone else's DX7 and someone else's
//! interface: PCM at 16, 24 or 32 bits or IEEE float, any channel count.

use std::{
    error::Error,
    fs::{File, OpenOptions},
    io::{BufWriter, Read, Write},
    path::Path,
};

const HEADER_BYTES: u32 = 44;
const FORMAT_PCM: u16 = 1;
const FORMAT_IEEE_FLOAT: u16 = 3;
const FORMAT_EXTENSIBLE: u16 = 0xfffe;

pub struct Report {
    pub frames: usize,
    pub sample_rate: u32,
    pub channels: u16,
    pub peak: f32,
    pub rms: f32,
}

/// Write a new mono float file. An existing path is an error, never an
/// overwrite.
pub fn write(path: &Path, samples: &[f32], sample_rate: u32) -> Result<Report, Box<dyn Error>> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    let file = OpenOptions::new().write(true).create_new(true).open(path)?;
    let mut out = BufWriter::new(file);
    let data = u32::try_from(samples.len() * 4)?;
    out.write_all(b"RIFF")?;
    out.write_all(&(HEADER_BYTES - 8 + data).to_le_bytes())?;
    out.write_all(b"WAVEfmt ")?;
    out.write_all(&16u32.to_le_bytes())?;
    out.write_all(&FORMAT_IEEE_FLOAT.to_le_bytes())?;
    out.write_all(&1u16.to_le_bytes())?;
    out.write_all(&sample_rate.to_le_bytes())?;
    out.write_all(&(sample_rate * 4).to_le_bytes())?;
    out.write_all(&4u16.to_le_bytes())?;
    out.write_all(&32u16.to_le_bytes())?;
    out.write_all(b"data")?;
    out.write_all(&data.to_le_bytes())?;
    for sample in samples {
        out.write_all(&sample.to_le_bytes())?;
    }
    out.flush()?;
    out.into_inner()?.sync_all()?;
    Ok(measure(samples, sample_rate, 1))
}

/// Read one channel of any ordinary WAV file, as floats in -1.0..=1.0.
pub fn read_channel(path: &Path, channel: u16) -> Result<(Vec<f32>, Report), Box<dyn Error>> {
    let mut bytes = Vec::new();
    File::open(path)?.read_to_end(&mut bytes)?;
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("not a WAV file".into());
    }
    let mut format = None;
    let mut data = None;
    let mut offset = 12;
    while offset + 8 <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        let size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into()?) as usize;
        let body = offset + 8;
        let end = body.saturating_add(size).min(bytes.len());
        match id {
            b"fmt " => format = Some(parse_format(&bytes[body..end])?),
            b"data" => data = Some(&bytes[body..end]),
            _ => {}
        }
        // Chunks are word-aligned; an odd size carries a pad byte.
        offset = body + size + (size & 1);
    }
    let Some(format) = format else {
        return Err("no fmt chunk".into());
    };
    let Some(data) = data else {
        return Err("no data chunk".into());
    };
    if channel >= format.channels {
        return Err(format!(
            "channel {channel} requested from a {}-channel file",
            format.channels
        )
        .into());
    }
    let width = usize::from(format.bits / 8);
    let frame = width * usize::from(format.channels);
    let frames = data.len() / frame;
    let mut samples = Vec::with_capacity(frames);
    for index in 0..frames {
        let at = index * frame + usize::from(channel) * width;
        let raw = &data[at..at + width];
        let value = match (format.tag, format.bits) {
            (FORMAT_IEEE_FLOAT, 32) => f32::from_le_bytes(raw.try_into()?),
            (FORMAT_PCM, 16) => f32::from(i16::from_le_bytes(raw.try_into()?)) / 32_768.0,
            (FORMAT_PCM, 24) => {
                let value = i32::from_le_bytes([0, raw[0], raw[1], raw[2]]) >> 8;
                value as f32 / 8_388_608.0
            }
            (FORMAT_PCM, 32) => i32::from_le_bytes(raw.try_into()?) as f32 / 2_147_483_648.0,
            (tag, bits) => {
                return Err(format!("unsupported WAV format {tag} at {bits} bits").into());
            }
        };
        samples.push(value);
    }
    if let Some(index) = samples.iter().position(|sample| !sample.is_finite()) {
        return Err(format!("sample {index} is not a finite number").into());
    }
    let report = measure(&samples, format.sample_rate, format.channels);
    Ok((samples, report))
}

/// Read a file this laboratory wrote: mono, and checked to be so.
pub fn read(path: &Path) -> Result<(Vec<f32>, Report), Box<dyn Error>> {
    let (samples, report) = read_channel(path, 0)?;
    if report.channels != 1 {
        return Err(format!(
            "expected a mono laboratory file, found {} channels",
            report.channels
        )
        .into());
    }
    Ok((samples, report))
}

struct Format {
    tag: u16,
    channels: u16,
    sample_rate: u32,
    bits: u16,
}

fn parse_format(chunk: &[u8]) -> Result<Format, Box<dyn Error>> {
    if chunk.len() < 16 {
        return Err("fmt chunk too short".into());
    }
    let mut tag = u16::from_le_bytes([chunk[0], chunk[1]]);
    let channels = u16::from_le_bytes([chunk[2], chunk[3]]);
    let sample_rate = u32::from_le_bytes(chunk[4..8].try_into()?);
    let bits = u16::from_le_bytes([chunk[14], chunk[15]]);
    if tag == FORMAT_EXTENSIBLE {
        // The real format is the first word of the sub-format GUID.
        if chunk.len() < 26 {
            return Err("extensible fmt chunk too short".into());
        }
        tag = u16::from_le_bytes([chunk[24], chunk[25]]);
    }
    if channels == 0 || sample_rate == 0 || !matches!(bits, 16 | 24 | 32) {
        return Err(format!("unsupported WAV: {channels} channels, {bits} bits").into());
    }
    Ok(Format {
        tag,
        channels,
        sample_rate,
        bits,
    })
}

fn measure(samples: &[f32], sample_rate: u32, channels: u16) -> Report {
    let peak = samples.iter().fold(0.0f32, |peak, s| peak.max(s.abs()));
    let energy: f64 = samples.iter().map(|s| f64::from(*s) * f64::from(*s)).sum();
    let rms = if samples.is_empty() {
        0.0
    } else {
        (energy / samples.len() as f64).sqrt() as f32
    };
    Report {
        frames: samples.len(),
        sample_rate,
        channels,
        peak,
        rms,
    }
}
