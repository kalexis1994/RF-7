//! Build, validate, install and launch the Windows test instrument.
//! All mutable host data is confined to the project's owned audition library.

use super::package::{build_to, host_root, run as checked, workspace_root};
use serde_json::{Value, json};
use std::{
    error::Error,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const ID: &str = "org.rackforge.rf7";
const INSTANCE: &str = "desktop.org.rackforge.rf7";
const PROGRAM: &str = "program-01";
const MARKER: &str = "RF-7 audition library v1\n";

pub fn run(args: &[String]) -> Result<(), Box<dyn Error>> {
    let prepare_only = match args {
        [] => false,
        [arg] if arg == "--prepare-only" => true,
        _ => return Err("usage: audition [--prepare-only]".into()),
    };
    if !cfg!(windows) && !prepare_only {
        return Err(
            "automatic Desktop launch currently supports Windows; use --prepare-only elsewhere"
                .into(),
        );
    }
    let root = workspace_root()?;
    let host = host_root()?;
    let desktop = if prepare_only {
        None
    } else {
        Some(find_desktop(&host)?)
    };
    let area = root.join("dist/audition");
    fs::create_dir_all(&area)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(area.join("workflow.lock"))?;
    lock.try_lock()
        .map_err(|_| "another audition workflow is running")?;
    ensure_desktop_closed()?;
    let library = area.join("RackForgeData");
    own_library(&library)?;
    ensure_no_newer_version(&library)?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let run_dir = area.join(format!(
        "{}-{stamp}-{}",
        env!("CARGO_PKG_VERSION"),
        std::process::id()
    ));
    fs::create_dir(&run_dir)?;

    println!(
        "Building RF-7 {} for audition...",
        env!("CARGO_PKG_VERSION")
    );
    let mut build = cargo(&root);
    checked(build.args([
        "build",
        "--locked",
        "--release",
        "--target",
        "wasm32-unknown-unknown",
        "-p",
        "rf7-plugin",
    ]))?;
    let mut host_build = cargo(&host);
    checked(host_build.args([
        "build",
        "--locked",
        "--release",
        "-p",
        "rackforge-store",
        "-p",
        "rackforge-core",
    ]))?;
    let archive = run_dir.join(format!("RF-7-{}.rfplugin", env!("CARGO_PKG_VERSION")));
    build_to(&archive)?;
    // Recheck after builds: another launch may have happened while compiling.
    ensure_desktop_closed()?;
    let store = host
        .join("target/release")
        .join(format!("rackforge-store{}", std::env::consts::EXE_SUFFIX));
    let store_root = library.join("plugin-store");
    // Same-version replacement is restricted to our marked development library.
    checked(
        Command::new(&store)
            .arg("install-local")
            .arg(&archive)
            .arg(&store_root)
            .arg("--replace"),
    )?;
    checked(Command::new(&store).arg("enable").arg(ID).arg(&store_root))?;
    let installed = store_root
        .join("packages")
        .join(ID)
        .join(env!("CARGO_PKG_VERSION"));
    verify_install(
        &installed,
        &root.join("target/wasm32-unknown-unknown/release/rf7_plugin.wasm"),
    )?;
    prepare_session(&library, &run_dir)?;
    copy_initial_audio_settings(&library)?;
    let logs = run_dir.join("rackforge.log");
    let mut receipt = json!({
        "schema_version": 1, "version": env!("CARGO_PKG_VERSION"), "plugin_id": ID,
        "archive": archive, "installed_package": installed, "rackforge_root": library,
        "desktop": desktop, "log": logs, "status": "installed", "pid": null
    });
    if let Some(desktop) = desktop {
        ensure_desktop_closed()?;
        let log = File::create(&logs)?;
        // Native GUI is explicitly requested; only CLI helpers suppress windows.
        let mut child = Command::new(&desktop)
            .arg("--rackforge-root")
            .arg(&library)
            .current_dir(desktop.parent().ok_or("desktop has no directory")?)
            .stdin(Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log)
            .spawn()?;
        receipt["pid"] = json!(child.id());
        receipt["status"] = json!("launch_requested");
        write_json(&run_dir.join("audition.json"), &receipt)?;
        for _ in 0..12 {
            thread::sleep(Duration::from_millis(250));
            if let Some(status) = child.try_wait()? {
                return Err(format!(
                    "RackForge exited during startup ({status}); see {}",
                    logs.display()
                )
                .into());
            }
        }
        println!(
            "RackForge started: PID {}. RF-7 {} installed and selected.",
            child.id(),
            env!("CARGO_PKG_VERSION")
        );
        println!("Startup log: {}", logs.display());
    } else {
        write_json(&run_dir.join("audition.json"), &receipt)?;
        println!(
            "Prepared RF-7 {} without launching Desktop.",
            env!("CARGO_PKG_VERSION")
        );
    }
    println!("Audition library: {}", library.display());
    println!("Receipt: {}", run_dir.join("audition.json").display());
    Ok(())
}

fn cargo(directory: &Path) -> Command {
    let mut command = Command::new("cargo");
    command.current_dir(directory);
    // Match the local GNU toolchain without changing machine-wide settings.
    if cfg!(windows) && Path::new("C:/msys64/ucrt64/bin").is_dir() {
        let mut paths = vec![PathBuf::from("C:/msys64/ucrt64/bin")];
        if let Some(path) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&path));
        }
        if let Ok(path) = std::env::join_paths(paths) {
            command.env("PATH", path);
        }
    }
    command
}

fn find_desktop(host: &Path) -> Result<PathBuf, Box<dyn Error>> {
    if let Some(path) = std::env::var_os("RF7_DESKTOP") {
        let path = PathBuf::from(path);
        if !path.is_file() {
            return Err("RF7_DESKTOP must name an existing RackForge executable".into());
        }
        return Ok(path.canonicalize()?);
    }
    let path = [
        host.join("dist/windows-x86_64/rackforge.exe"),
        host.join("target/release/rackforge-desktop.exe"),
    ]
    .into_iter()
    .filter(|path| path.is_file())
    .max_by_key(|path| fs::metadata(path).and_then(|meta| meta.modified()).ok())
    .ok_or("build RackForge Desktop first, or set RF7_DESKTOP to its executable")?;
    Ok(path.canonicalize()?)
}

fn ensure_desktop_closed() -> Result<(), Box<dyn Error>> {
    #[cfg(windows)]
    {
        for name in ["rackforge.exe", "rackforge-desktop.exe"] {
            let mut command = Command::new("tasklist.exe");
            command.args(["/FI", &format!("IMAGENAME eq {name}"), "/FO", "CSV", "/NH"]);
            super::package::hide_console(&mut command);
            let output = command.output()?;
            if !output.status.success() {
                return Err("could not inspect running RackForge processes".into());
            }
            if String::from_utf8_lossy(&output.stdout)
                .to_ascii_lowercase()
                .contains(&format!("\"{name}\""))
            {
                return Err("RackForge is already open. Close its window and rerun audition so the new plugin is loaded; no process was terminated".into());
            }
        }
    }
    Ok(())
}

fn own_library(library: &Path) -> Result<(), Box<dyn Error>> {
    if library.exists() {
        if fs::symlink_metadata(library)?.file_type().is_symlink() {
            return Err("audition library must not be a link".into());
        }
        if fs::read_to_string(library.join(".rf7-owned"))
            .ok()
            .as_deref()
            != Some(MARKER)
        {
            return Err("refusing to modify an unmarked audition library".into());
        }
    } else {
        fs::create_dir(library)?;
        fs::write(library.join(".rf7-owned"), MARKER)?;
    }
    Ok(())
}

fn stable_version(text: &str) -> Result<[u64; 3], Box<dyn Error>> {
    let parts: Vec<_> = text
        .split('.')
        .map(str::parse::<u64>)
        .collect::<Result<_, _>>()?;
    parts
        .try_into()
        .map_err(|_| "audition requires a stable major.minor.patch version".into())
}

fn ensure_no_newer_version(library: &Path) -> Result<(), Box<dyn Error>> {
    let packages = library.join("plugin-store/packages").join(ID);
    if !packages.exists() {
        return Ok(());
    }
    let current = stable_version(env!("CARGO_PKG_VERSION"))?;
    for entry in fs::read_dir(packages)? {
        let entry = entry?;
        if entry.file_type()?.is_dir()
            && stable_version(&entry.file_name().to_string_lossy())? > current
        {
            return Err("a newer RF-7 version is already in the audition library; refusing a misleading downgrade".into());
        }
    }
    Ok(())
}

fn verify_install(installed: &Path, component: &Path) -> Result<(), Box<dyn Error>> {
    let runtime: Value =
        serde_json::from_slice(&fs::read(installed.join("metadata/runtime.json"))?)?;
    if runtime["version"] != env!("CARGO_PKG_VERSION") {
        return Err("installed metadata version does not match the laboratory version".into());
    }
    if fs::read(installed.join("component.wasm"))? != fs::read(component)? {
        return Err("installed WASM differs from the freshly built component".into());
    }
    Ok(())
}

fn checkpoint(previous: Option<Value>) -> Result<Value, Box<dyn Error>> {
    let mut value = previous.unwrap_or_else(
        || json!({"schema_version":1,"session_id":"live.main","master_level":1000,"master_pan":0}),
    );
    if !value.is_object()
        || value["session_id"] != "live.main"
        || !matches!(value["schema_version"].as_u64(), Some(1..=4))
    {
        return Err("unsupported audition session checkpoint; original file preserved".into());
    }
    value["active_mode"] = json!("play");
    value["active_instance_id"] = json!(INSTANCE);
    // Only select the first program on the first run; later sessions retain
    // whichever cartridge voice was left selected.
    if value.get("selected_sounds").is_none() {
        value["selected_sounds"] = json!({});
    }
    let sounds = value["selected_sounds"]
        .as_object_mut()
        .ok_or("invalid selected_sounds checkpoint")?;
    sounds.entry(INSTANCE).or_insert(json!(PROGRAM));
    Ok(value)
}

fn prepare_session(library: &Path, run_dir: &Path) -> Result<(), Box<dyn Error>> {
    let path = library.join("data/sessions/live.main.json");
    let previous = if path.exists() {
        Some(serde_json::from_slice(&fs::read(&path)?)?)
    } else {
        None
    };
    let value = checkpoint(previous)?;
    let staging = run_dir.join("session-next.json");
    write_json(&staging, &value)?;
    fs::create_dir_all(path.parent().ok_or("session has no parent")?)?;
    let backup = run_dir.join("session-before.json");
    let had_previous = path.exists();
    if had_previous {
        fs::rename(&path, &backup)?;
    }
    if let Err(error) = fs::rename(staging, &path) {
        if had_previous {
            let _ = fs::rename(&backup, &path);
        }
        return Err(error.into());
    }
    Ok(())
}

fn copy_initial_audio_settings(library: &Path) -> Result<(), Box<dyn Error>> {
    let destination = library.join("config/audio.toml");
    if destination.exists() {
        return Ok(());
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let source = PathBuf::from(local).join("RackForge/config/audio.toml");
        if source.is_file() {
            fs::create_dir_all(destination.parent().ok_or("audio config has no parent")?)?;
            fs::copy(&source, destination)?;
            println!(
                "Copied initial audio/MIDI preferences from {}",
                source.display()
            );
        }
    }
    Ok(())
}

fn write_json(path: &Path, value: &Value) -> Result<(), Box<dyn Error>> {
    let bytes = serde_json::to_vec_pretty(value)?;
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_checkpoint_selects_rf7_and_preserves_user_controls() {
        let first = checkpoint(None).unwrap();
        assert_eq!(first["active_instance_id"], INSTANCE);
        assert_eq!(first["selected_sounds"][INSTANCE], PROGRAM);
        let next = checkpoint(Some(json!({"schema_version":4,"session_id":"live.main",
            "master_level":450,"master_pan":-50,"active_mode":"live",
            "selected_sounds":{INSTANCE:"program-17"},
            "parameter_links":[],"live":{"custom":"preserve"}})))
        .unwrap();
        assert_eq!(next["master_level"], 450);
        assert_eq!(next["selected_sounds"][INSTANCE], "program-17");
        assert_eq!(next["live"]["custom"], "preserve");
        assert!(checkpoint(Some(json!({"schema_version":99,"session_id":"live.main"}))).is_err());
    }

    #[test]
    fn library_ownership_and_installed_bytes_are_checked() {
        let path = std::env::temp_dir().join(format!(
            "rf7-audition-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&path).unwrap();
        let library = path.join("library");
        fs::create_dir(&library).unwrap();
        fs::write(library.join("user-file"), "keep").unwrap();
        assert!(own_library(&library).is_err());
        assert_eq!(
            fs::read_to_string(library.join("user-file")).unwrap(),
            "keep"
        );
        let owned = path.join("owned");
        own_library(&owned).unwrap();
        own_library(&owned).unwrap();
        let installed = path.join("installed");
        fs::create_dir_all(installed.join("metadata")).unwrap();
        fs::write(
            installed.join("metadata/runtime.json"),
            json!({"version": env!("CARGO_PKG_VERSION")}).to_string(),
        )
        .unwrap();
        fs::write(installed.join("component.wasm"), b"old").unwrap();
        fs::write(path.join("current.wasm"), b"new").unwrap();
        assert!(verify_install(&installed, &path.join("current.wasm")).is_err());
        fs::write(installed.join("component.wasm"), b"new").unwrap();
        verify_install(&installed, &path.join("current.wasm")).unwrap();
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn version_order_does_not_use_lexicographic_sorting() {
        assert!(stable_version("0.10.0").unwrap() > stable_version("0.9.9").unwrap());
        assert!(stable_version("0.1").is_err());
        assert!(stable_version("0.1.0-beta").is_err());
    }
}
