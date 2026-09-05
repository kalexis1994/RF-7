//! Offline laboratory. All rendering, WAV writing and reporting runs in Rust.
//! No audio device is opened and no existing file is overwritten.

mod audition;
mod package;
mod wav;

use rf7_dsp::{Engine, POLYPHONY, SAMPLE_RATE_MAX, SAMPLE_RATE_MIN};
use rf7_voice::{
    BULK_DUMP_LENGTH, Cartridge, VOICE_DUMP_LENGTH, VOICES_PER_CARTRIDGE, decode_bulk_dump,
    decode_voice_dump, printable_name,
};
use serde_json::json;
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

const HELP: &str = "RF-7 six-operator FM laboratory 0.1.1
Usage:
  rf7-lab render --output PATH.wav [options]
  rf7-lab demo --output PATH.wav [--cartridge PATH.syx]
  rf7-lab stress [--sample-rate HZ]
  rf7-lab inspect PATH.wav
  rf7-lab cartridge PATH.syx
  rf7-lab package
  rf7-lab audition [--prepare-only]
Render options:
  --program N       Cartridge slot 1..32 (default 1)
  --note N          MIDI 0..127 (default 60, middle C)
  --velocity N      MIDI 1..127 (default 100)
  --sample-rate HZ  8000..192000 (default 48000)
  --seconds S       Duration 0.05..60 (default 4)
  --hold S          Key hold, shorter than the duration (default 2)
  --cartridge PATH  A DX7 cartridge or single-voice System Exclusive file
WAV is mono IEEE float, without normalisation or clipping. Every render also
writes a JSON report beside it. No voice data ships with RF-7: without
--cartridge the eight RackForge-written factory voices are used.
Demo: each factory voice in turn, one chord each.
Stress: sixteen voices, 128-frame blocks, three seconds, with timings.
";

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if let Err(error) = dispatch(&arguments) {
        eprintln!("rf7-lab: {error}");
        std::process::exit(1);
    }
}

fn dispatch(arguments: &[String]) -> Result<(), Box<dyn Error>> {
    let Some((command, rest)) = arguments.split_first() else {
        print!("{HELP}");
        return Ok(());
    };
    match command.as_str() {
        "render" => render(&Options::parse(rest, true)?),
        "demo" => demo(&Options::parse(rest, true)?),
        "stress" => stress(&Options::parse(rest, false)?),
        "inspect" => inspect(&single_path(rest)?),
        "cartridge" => describe_cartridge(&single_path(rest)?),
        "package" => package::build(),
        "audition" => audition::run(rest),
        "help" | "--help" | "-h" => {
            print!("{HELP}");
            Ok(())
        }
        other => Err(format!("unknown command: {other}\n\n{HELP}").into()),
    }
}

fn single_path(arguments: &[String]) -> Result<PathBuf, Box<dyn Error>> {
    match arguments {
        [path] => Ok(PathBuf::from(path)),
        _ => Err("expected exactly one path".into()),
    }
}

struct Options {
    output: Option<PathBuf>,
    cartridge: Option<PathBuf>,
    program: usize,
    note: u8,
    velocity: u8,
    sample_rate: f32,
    seconds: f64,
    hold: f64,
}

impl Options {
    fn parse(arguments: &[String], wants_output: bool) -> Result<Self, Box<dyn Error>> {
        let mut options = Self {
            output: None,
            cartridge: None,
            program: 0,
            note: 60,
            velocity: 100,
            sample_rate: 48_000.0,
            seconds: 4.0,
            hold: 2.0,
        };
        let mut seen = std::collections::BTreeSet::new();
        let mut index = 0;
        while index < arguments.len() {
            let flag = arguments[index].as_str();
            if !seen.insert(flag.to_owned()) {
                return Err(format!("duplicate option: {flag}").into());
            }
            let value = arguments
                .get(index + 1)
                .ok_or_else(|| format!("{flag} needs a value"))?;
            match flag {
                "--output" => options.output = Some(PathBuf::from(value)),
                "--cartridge" => options.cartridge = Some(PathBuf::from(value)),
                "--program" => {
                    let slot: usize = value.parse()?;
                    if !(1..=VOICES_PER_CARTRIDGE).contains(&slot) {
                        return Err("--program is a cartridge slot 1..32".into());
                    }
                    options.program = slot - 1;
                }
                "--note" => {
                    let note: u8 = value.parse()?;
                    if note > 127 {
                        return Err("--note is 0..127".into());
                    }
                    options.note = note;
                }
                "--velocity" => {
                    let velocity: u8 = value.parse()?;
                    if !(1..=127).contains(&velocity) {
                        return Err("--velocity is 1..127".into());
                    }
                    options.velocity = velocity;
                }
                "--sample-rate" => {
                    let rate: f32 = value.parse()?;
                    if !(SAMPLE_RATE_MIN..=SAMPLE_RATE_MAX).contains(&rate) {
                        return Err("--sample-rate is 8000..192000".into());
                    }
                    options.sample_rate = rate;
                }
                "--seconds" => options.seconds = value.parse()?,
                "--hold" => options.hold = value.parse()?,
                other => return Err(format!("unknown option: {other}").into()),
            }
            index += 2;
        }
        if !(0.05..=60.0).contains(&options.seconds) {
            return Err("--seconds is 0.05..60".into());
        }
        if options.hold <= 0.0 || options.hold >= options.seconds {
            return Err("--hold must be greater than zero and shorter than --seconds".into());
        }
        if wants_output && options.output.is_none() {
            return Err("--output is required".into());
        }
        Ok(options)
    }
}

fn engine(options: &Options) -> Result<Engine, Box<dyn Error>> {
    let mut engine = Engine::new(options.sample_rate)?;
    if let Some(path) = &options.cartridge {
        engine.load_cartridge(read_cartridge(path)?);
    }
    if !engine.select_program(options.program) {
        return Err("no such program".into());
    }
    Ok(engine)
}

/// Read either container. Nothing here ever writes a cartridge back to disk.
fn read_cartridge(path: &Path) -> Result<Cartridge, Box<dyn Error>> {
    let bytes = fs::read(path)?;
    match bytes.len() {
        BULK_DUMP_LENGTH => Ok(decode_bulk_dump(&bytes)?),
        VOICE_DUMP_LENGTH => {
            let decoded = decode_voice_dump(&bytes)?;
            Ok(Cartridge::from_voices([decoded.voice; VOICES_PER_CARTRIDGE]))
        }
        length => Err(format!(
            "{} is {length} bytes; expected a {BULK_DUMP_LENGTH}-byte cartridge or a {VOICE_DUMP_LENGTH}-byte voice dump",
            path.display()
        )
        .into()),
    }
}

fn render(options: &Options) -> Result<(), Box<dyn Error>> {
    let output = options.output.as_deref().expect("checked while parsing");
    let mut engine = engine(options)?;
    let name = engine.program_name(options.program).to_owned();
    let total = (options.seconds * f64::from(options.sample_rate)) as usize;
    let release = (options.hold * f64::from(options.sample_rate)) as usize;
    let mut samples = Vec::with_capacity(total);
    engine.note_on(0, options.note, options.velocity);
    let started = Instant::now();
    for frame in 0..total {
        if frame == release {
            engine.note_off(0, options.note);
        }
        samples.push(engine.next_sample());
    }
    let elapsed = started.elapsed();
    let measured = wav::write(output, &samples, options.sample_rate as u32)?;
    let report = json!({
        "schema_version": 1,
        "command": "render",
        "program": options.program + 1,
        "program_name": name,
        "note": options.note,
        "velocity": options.velocity,
        "sample_rate": options.sample_rate,
        "seconds": options.seconds,
        "hold": options.hold,
        "cartridge": options.cartridge,
        "frames": measured.frames,
        "peak": measured.peak,
        "rms": measured.rms,
        "stolen_notes": engine.stolen_notes(),
        "render_seconds": elapsed.as_secs_f64(),
    });
    write_report(&report_path(output), &report)?;
    println!(
        "Rendered {} ({name}): peak {:.4}, rms {:.4}",
        output.display(),
        measured.peak,
        measured.rms
    );
    Ok(())
}

fn demo(options: &Options) -> Result<(), Box<dyn Error>> {
    let output = options.output.as_deref().expect("checked while parsing");
    let mut engine = engine(options)?;
    let rate = options.sample_rate;
    let chord = [57u8, 60, 64, 67];
    let per_program = (2.5 * f64::from(rate)) as usize;
    let hold = (1.6 * f64::from(rate)) as usize;
    let mut samples = Vec::new();
    let mut played = Vec::new();
    for program in 0..8 {
        if !engine.select_program(program) {
            break;
        }
        played.push(json!({"slot": program + 1, "name": engine.program_name(program)}));
        for note in chord {
            engine.note_on(0, note, 100);
        }
        for frame in 0..per_program {
            if frame == hold {
                for note in chord {
                    engine.note_off(0, note);
                }
            }
            samples.push(engine.next_sample());
        }
    }
    let measured = wav::write(output, &samples, rate as u32)?;
    let report = json!({
        "schema_version": 1,
        "command": "demo",
        "sample_rate": rate,
        "cartridge": options.cartridge,
        "programs": played,
        "frames": measured.frames,
        "peak": measured.peak,
        "rms": measured.rms,
        "stolen_notes": engine.stolen_notes(),
    });
    write_report(&report_path(output), &report)?;
    println!(
        "Rendered {}: {} programs, peak {:.4}",
        output.display(),
        played.len(),
        measured.peak
    );
    Ok(())
}

fn stress(options: &Options) -> Result<(), Box<dyn Error>> {
    const BLOCK: usize = 128;
    let mut engine = engine(options)?;
    let rate = options.sample_rate;
    let blocks = (f64::from(rate) * 3.0 / BLOCK as f64) as usize;
    // Sixteen keys at once is the instrument's own limit, and the only load
    // worth timing: anything past it is voice stealing, not work.
    for slot in 0..POLYPHONY {
        engine.note_on(0, 36 + slot as u8 * 3, 100);
    }
    let mut peak = 0.0f32;
    let started = Instant::now();
    for _ in 0..blocks {
        for _ in 0..BLOCK {
            peak = peak.max(engine.next_sample().abs());
        }
    }
    let elapsed = started.elapsed().as_secs_f64();
    let audio_seconds = (blocks * BLOCK) as f64 / f64::from(rate);
    println!("RF-7 stress at {rate} Hz, {POLYPHONY} voices, {BLOCK}-frame blocks");
    println!("  rendered {audio_seconds:.2} s of audio in {elapsed:.3} s");
    println!(
        "  real-time factor {:.1}x",
        audio_seconds / elapsed.max(1e-9)
    );
    println!("  peak {peak:.4}, active voices {}", engine.active_voices());
    println!("  stolen notes {}", engine.stolen_notes());
    Ok(())
}

fn inspect(path: &Path) -> Result<(), Box<dyn Error>> {
    let (samples, report) = wav::read(path)?;
    println!("{}", path.display());
    println!("  {} frames at {} Hz", report.frames, report.sample_rate);
    println!(
        "  {:.3} seconds, peak {:.4}, rms {:.4}",
        report.frames as f64 / f64::from(report.sample_rate.max(1)),
        report.peak,
        report.rms
    );
    println!("  every one of {} samples is finite", samples.len());
    if report.peak > 1.0 {
        println!("  note: the peak is above full scale; nothing was normalised");
    }
    Ok(())
}

fn describe_cartridge(path: &Path) -> Result<(), Box<dyn Error>> {
    let cartridge = read_cartridge(path)?;
    println!("{}", path.display());
    let corrections = cartridge.corrections();
    if corrections.is_clean() {
        println!("  every byte was already in range");
    } else {
        println!(
            "  {} bytes were out of range and were clamped",
            corrections.0
        );
    }
    for slot in 0..VOICES_PER_CARTRIDGE {
        let voice = cartridge.voice(slot).expect("a cartridge has 32 voices");
        let clamped = cartridge.voice_corrections(slot);
        println!(
            "  {:>2}  {:<10}  algorithm {:>2}  feedback {}{}",
            slot + 1,
            printable_name(&voice.name),
            voice.algorithm + 1,
            voice.feedback,
            if clamped.is_clean() {
                String::new()
            } else {
                format!("  ({} clamped)", clamped.0)
            }
        );
    }
    Ok(())
}

fn report_path(output: &Path) -> PathBuf {
    output.with_extension("json")
}

fn write_report(path: &Path, value: &serde_json::Value) -> Result<(), Box<dyn Error>> {
    use std::{fs::OpenOptions, io::Write};
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(&serde_json::to_vec_pretty(value)?)?;
    file.sync_all()?;
    Ok(())
}
