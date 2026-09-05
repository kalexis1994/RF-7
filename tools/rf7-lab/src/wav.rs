//! Mono IEEE float WAV, written and read by this laboratory alone.
//!
//! No normalisation and no clipping: a report that says the peak was 1.4 is
//! more useful than a file that was quietly brought back to 1.0.

use std::{
    error::Error,
    fs::{File, OpenOptions},
    io::{BufWriter, Read, Write},
    path::Path,
};

const HEADER_BYTES: u32 = 44;
const FORMAT_IEEE_FLOAT: u16 = 3;

pub struct Report {
    pub frames: usize,
    pub sample_rate: u32,
    pub peak: f32,
    pub rms: f32,
}

/// Write a new file. An existing path is an error, never an overwrite.
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
    Ok(measure(samples, sample_rate))
}

/// Read a file this laboratory wrote, checking every structural claim it makes.
pub fn read(path: &Path) -> Result<(Vec<f32>, Report), Box<dyn Error>> {
    let mut bytes = Vec::new();
    File::open(path)?.read_to_end(&mut bytes)?;
    if bytes.len() < HEADER_BYTES as usize {
        return Err("shorter than a WAV header".into());
    }
    if &bytes[..4] != b"RIFF" || &bytes[8..16] != b"WAVEfmt " || &bytes[36..40] != b"data" {
        return Err("not a laboratory WAV file".into());
    }
    if u16::from_le_bytes([bytes[20], bytes[21]]) != FORMAT_IEEE_FLOAT
        || u16::from_le_bytes([bytes[22], bytes[23]]) != 1
        || u16::from_le_bytes([bytes[34], bytes[35]]) != 32
    {
        return Err("expected 32-bit mono IEEE float".into());
    }
    let sample_rate = u32::from_le_bytes(bytes[24..28].try_into()?);
    let declared = u32::from_le_bytes(bytes[40..44].try_into()?) as usize;
    let payload = &bytes[HEADER_BYTES as usize..];
    if declared != payload.len() || !declared.is_multiple_of(4) {
        return Err("the declared data length does not match the file".into());
    }
    let samples: Vec<f32> = payload
        .as_chunks::<4>()
        .0
        .iter()
        .map(|chunk| f32::from_le_bytes(*chunk))
        .collect();
    if let Some(index) = samples.iter().position(|sample| !sample.is_finite()) {
        return Err(format!("sample {index} is not a finite number").into());
    }
    let report = measure(&samples, sample_rate);
    Ok((samples, report))
}

fn measure(samples: &[f32], sample_rate: u32) -> Report {
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
        peak,
        rms,
    }
}
