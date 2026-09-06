//! Package and smoke-test through RackForge's own Rust tools.

use std::{error::Error, fs, path::Path, process::Command};

pub fn build() -> Result<(), Box<dyn Error>> {
    let root = workspace_root()?;
    build_to(
        &root
            .join("dist")
            .join(format!("RF-7-{}.rfplugin", env!("CARGO_PKG_VERSION"))),
    )
}

pub(crate) fn workspace_root() -> Result<std::path::PathBuf, Box<dyn Error>> {
    Ok(Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or("cannot resolve the workspace root")?
        .to_path_buf())
}

pub(crate) fn host_root() -> Result<std::path::PathBuf, Box<dyn Error>> {
    Ok(workspace_root()?
        .parent()
        .ok_or("cannot resolve the sibling RackForge checkout")?
        .join("rackforge"))
}

pub(crate) fn build_to(output: &Path) -> Result<(), Box<dyn Error>> {
    let root = workspace_root()?;
    // The component is built here rather than taken from wherever the target
    // directory happens to be, so a package can never be validated against
    // the WASM of an earlier edit.
    run(Command::new("cargo").current_dir(&root).args([
        "build",
        "--locked",
        "--release",
        "--target",
        "wasm32-unknown-unknown",
        "-p",
        "rf7-plugin",
    ]))?;
    let host = host_root()?;
    let tools = host.join("target/release");
    let store = tools.join(format!("rackforge-store{}", std::env::consts::EXE_SUFFIX));
    let core = tools.join(format!("rackforge-core{}", std::env::consts::EXE_SUFFIX));
    let component = root.join("target/wasm32-unknown-unknown/release/rf7_plugin.wasm");
    let package = root.join("package");
    let dist = root.join("dist");
    if output.exists() {
        return Err(format!("refusing to overwrite {}", output.display()).into());
    }
    for file in [&store, &core, &component] {
        if !file.is_file() {
            return Err(format!("build the required artifact first: {}", file.display()).into());
        }
    }
    fs::create_dir_all(&dist)?;
    super::web_ui::build()?;
    fs::copy(&component, package.join("component.wasm"))?;
    run(Command::new(&core).arg("inspect").arg(&package))?;
    run(Command::new(&core)
        .arg("smoke")
        .arg(&package)
        .arg("--preset")
        .arg("program-001")
        .arg("--data-root")
        .arg(dist.join("smoke-data")))?;
    run(Command::new(&store)
        .arg("pack-wasm")
        .arg(&package)
        .arg(&component)
        .arg(output))?;
    println!("Validated package: {}", output.display());
    Ok(())
}

pub(crate) fn run(command: &mut Command) -> Result<(), Box<dyn Error>> {
    hide_console(command);
    let status = command.status()?;
    if !status.success() {
        return Err(format!("RackForge validation failed: {status}").into());
    }
    Ok(())
}

pub(crate) fn hide_console(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW for CLI helpers.
    }
    #[cfg(not(windows))]
    let _ = command;
}
