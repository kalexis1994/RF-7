//! Measuring the modulation index: of a recording, and of RF-7 itself.

use crate::wav;
use rf7_analysis::{estimate_index, spectrum};
use rf7_dsp::Engine;
use rf7_voice::{
    Library, calibration_cartridge, calibration_levels, calibration_voice, encode_bulk_dump,
};
use serde_json::json;
use std::{error::Error, fs::OpenOptions, io::Write, path::PathBuf};

/// Middle C, which is the note every calibration recording should hold.
pub const CARRIER_HZ: f64 = 261.625_565;
/// The calibration voice's modulator sits four times above its carrier.
pub const RATIO: f64 = 4.0;
const ORDERS: i32 = 8;

pub const HELP: &str = "  rf7-lab measure-index --input PATH.wav [--carrier HZ] [--channel N]
                         [--start S] [--seconds S] [--orders N] [--output REPORT.json]
  rf7-lab calibrate [--output REPORT.json]
  rf7-lab export-calibration --output PATH.syx
Measure options:
  --carrier HZ      The note the recording holds (default 261.63, middle C)
  --modulator HZ    Default: four times the carrier, as the calibration voice
  --channel N       Which channel of the recording to read (default 0)
  --start S         Skip this much of the recording first (default 0.5)
  --seconds S       Analyse this much after the start (default 2)
  --orders N        Sidebands either side of the carrier to fit (default 8)
";

struct Measure {
    input: PathBuf,
    output: Option<PathBuf>,
    carrier: f64,
    modulator: Option<f64>,
    channel: u16,
    start: f64,
    seconds: f64,
    orders: i32,
}

fn parse_measure(arguments: &[String]) -> Result<Measure, Box<dyn Error>> {
    let mut options = Measure {
        input: PathBuf::new(),
        output: None,
        carrier: CARRIER_HZ,
        modulator: None,
        channel: 0,
        start: 0.5,
        seconds: 2.0,
        orders: ORDERS,
    };
    let mut index = 0;
    let mut have_input = false;
    while index < arguments.len() {
        let flag = arguments[index].as_str();
        let value = arguments
            .get(index + 1)
            .ok_or_else(|| format!("{flag} needs a value"))?;
        match flag {
            "--input" => {
                options.input = PathBuf::from(value);
                have_input = true;
            }
            "--output" => options.output = Some(PathBuf::from(value)),
            "--carrier" => options.carrier = value.parse()?,
            "--modulator" => options.modulator = Some(value.parse()?),
            "--channel" => options.channel = value.parse()?,
            "--start" => options.start = value.parse()?,
            "--seconds" => options.seconds = value.parse()?,
            "--orders" => options.orders = value.parse()?,
            other => return Err(format!("unknown option: {other}").into()),
        }
        index += 2;
    }
    if !have_input {
        return Err("--input is required".into());
    }
    let positive = |value: f64| value.is_finite() && value > 0.0;
    if !positive(options.carrier) || options.start < 0.0 || !positive(options.seconds) {
        return Err("carrier must be positive, start non-negative, seconds positive".into());
    }
    if !(1..=16).contains(&options.orders) {
        return Err("--orders is 1..16".into());
    }
    Ok(options)
}

pub fn measure(arguments: &[String]) -> Result<(), Box<dyn Error>> {
    let options = parse_measure(arguments)?;
    let (samples, report) = wav::read_channel(&options.input, options.channel)?;
    let rate = f64::from(report.sample_rate);
    let first = (options.start * rate) as usize;
    let last = ((options.start + options.seconds) * rate) as usize;
    if first >= samples.len() {
        return Err("--start is past the end of the recording".into());
    }
    let window = &samples[first..last.min(samples.len())];
    let modulator = options.modulator.unwrap_or(options.carrier * RATIO);
    let estimate = estimate_index(
        &spectrum(window, rate),
        options.carrier,
        modulator,
        options.orders,
    )?;
    println!("{}", options.input.display());
    println!(
        "  {} Hz, {} channel(s), read channel {} from {:.2} s for {:.2} s",
        report.sample_rate,
        report.channels,
        options.channel,
        options.start,
        window.len() as f64 / rate
    );
    println!(
        "  modulation index: {:.4} radians ({:.4} cycles), residual {:.4}",
        estimate.beta,
        estimate.beta / core::f64::consts::TAU,
        estimate.residual
    );
    if estimate.residual > 0.05 {
        println!(
            "  note: the residual is high; the recording may not be a clean two-operator tone"
        );
    }
    println!("  order   expected Hz   found Hz   measured   model");
    for line in &estimate.lines {
        println!(
            "  {:>5}   {:>11.1}   {:>8.1}   {:>8.4}   {:>5.4}",
            line.order, line.expected_hz, line.found_hz, line.measured, line.model
        );
    }
    if let Some(output) = &options.output {
        write_report(
            output,
            &json!({
                "schema_version": 1,
                "command": "measure-index",
                "input": options.input,
                "channel": options.channel,
                "sample_rate": report.sample_rate,
                "carrier_hz": options.carrier,
                "modulator_hz": modulator,
                "orders": options.orders,
                "start": options.start,
                "seconds": window.len() as f64 / rate,
                "beta_radians": estimate.beta,
                "beta_cycles": estimate.beta / core::f64::consts::TAU,
                "residual": estimate.residual,
                "lines": estimate.lines.iter().map(|line| json!({
                    "order": line.order, "expected_hz": line.expected_hz,
                    "found_hz": line.found_hz, "measured": line.measured, "model": line.model,
                })).collect::<Vec<_>>(),
            }),
        )?;
    }
    Ok(())
}

/// Measure RF-7's own calibration voice at every level the cartridge holds.
///
/// This is the instrument measuring itself: it says what index RF-7 produces
/// at each modulator output level today, which is what a real recording will
/// be compared against.
pub fn calibrate(arguments: &[String]) -> Result<(), Box<dyn Error>> {
    let output = match arguments {
        [] => None,
        [flag, path] if flag == "--output" => Some(PathBuf::from(path)),
        _ => return Err("usage: calibrate [--output REPORT.json]".into()),
    };
    let rate = 48_000.0;
    println!("RF-7 calibration voice, middle C, modulator at 4:1");
    println!("  level   index (rad)   cycles   residual");
    let mut rows = Vec::new();
    for level in calibration_levels() {
        let mut engine = Engine::new(rate as f32)?;
        engine.set_gain(1.0);
        engine.load_library(Library::from_voices(&[calibration_voice(level)]));
        engine.note_on(0, 60, 127);
        for _ in 0..24_000 {
            engine.next_sample();
        }
        let samples: Vec<f32> = (0..96_000).map(|_| engine.next_sample()).collect();
        let estimate = estimate_index(
            &spectrum(&samples, rate),
            CARRIER_HZ,
            CARRIER_HZ * RATIO,
            ORDERS,
        )?;
        println!(
            "  {:>5}   {:>11.4}   {:>6.4}   {:>8.4}",
            level,
            estimate.beta,
            estimate.beta / core::f64::consts::TAU,
            estimate.residual
        );
        rows.push(json!({
            "level": level, "beta_radians": estimate.beta,
            "beta_cycles": estimate.beta / core::f64::consts::TAU, "residual": estimate.residual,
        }));
    }
    if let Some(output) = output {
        write_report(
            &output,
            &json!({
                "schema_version": 1, "command": "calibrate", "sample_rate": rate,
                "carrier_hz": CARRIER_HZ, "modulator_hz": CARRIER_HZ * RATIO, "orders": ORDERS,
                "levels": rows,
            }),
        )?;
    }
    Ok(())
}

/// The calibration cartridge as a System Exclusive bulk dump, ready to send to
/// a DX7. The only voices RF-7 ever writes to disk are its own.
pub fn export(arguments: &[String]) -> Result<(), Box<dyn Error>> {
    let output = match arguments {
        [flag, path] if flag == "--output" => PathBuf::from(path),
        _ => return Err("usage: export-calibration --output PATH.syx".into()),
    };
    let dump = encode_bulk_dump(&calibration_cartridge(), 0);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)?;
    file.write_all(&dump)?;
    file.sync_all()?;
    println!(
        "Wrote {} ({} bytes): 32 calibration voices, modulator levels 99 down to 6",
        output.display(),
        dump.len()
    );
    println!("Record each voice holding middle C for ten seconds, dry, from the line output.");
    Ok(())
}

fn write_report(path: &PathBuf, value: &serde_json::Value) -> Result<(), Box<dyn Error>> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(&serde_json::to_vec_pretty(value)?)?;
    file.sync_all()?;
    Ok(())
}
