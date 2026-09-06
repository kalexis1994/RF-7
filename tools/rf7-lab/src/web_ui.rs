//! Build the Rust PLAY surface and its generated browser bindings.
//!
//! The surface is a Rust crate compiled to WebAssembly; `wasm-bindgen`
//! writes the JavaScript glue and the `.wasm` beside the hand-written
//! `play.html` and `style.css`. The glue is generated output and is not
//! committed; this rebuilds it before every package.

use std::{error::Error, fs::OpenOptions, io::Write, path::Path, process::Command};

pub const GENERATED: [&str; 2] = ["web/app.js", "web/app_bg.wasm"];
pub const STATIC: [&str; 3] = ["web/play.html", "web/config.html", "web/style.css"];
const BINDGEN_VERSION: &str = "wasm-bindgen 0.2.127";

/// Every surface file the installed package holds must be the one just
/// built, byte for byte.
pub fn verify_install(installed: &Path, source: &Path) -> Result<(), Box<dyn Error>> {
    for asset in GENERATED.iter().chain(STATIC.iter()) {
        if std::fs::read(installed.join(asset))? != std::fs::read(source.join(asset))? {
            return Err(format!(
                "installed UI asset differs from the freshly built surface: {asset}"
            )
            .into());
        }
    }
    Ok(())
}

pub fn build() -> Result<(), Box<dyn Error>> {
    let root = super::package::workspace_root()?;
    let mut version = Command::new("wasm-bindgen");
    version.arg("--version");
    super::package::hide_console(&mut version);
    let output = version
        .output()
        .map_err(|_| "install wasm-bindgen-cli 0.2.127 to build the Rust UI")?;
    if !output.status.success() || String::from_utf8_lossy(&output.stdout).trim() != BINDGEN_VERSION
    {
        return Err("the Rust UI requires wasm-bindgen-cli exactly 0.2.127".into());
    }
    super::package::run(Command::new("cargo").current_dir(&root).args([
        "build",
        "--locked",
        "--release",
        "--target",
        "wasm32-unknown-unknown",
        "-p",
        "rf7-ui",
    ]))?;
    let web = root.join("package/web");
    super::package::run(
        Command::new("wasm-bindgen")
            .arg(root.join("target/wasm32-unknown-unknown/release/rf7_ui.wasm"))
            .arg("--out-dir")
            .arg(&web)
            .args(["--out-name", "app", "--target", "web", "--no-typescript"]),
    )?;
    // The bootstrap is appended to the generated glue; every decision the
    // surface makes is in Rust.
    let mut script = OpenOptions::new().append(true).open(web.join("app.js"))?;
    script.write_all(b"\n// Generated Rust UI bootstrap.\n__wbg_init();\n")?;
    for asset in STATIC {
        if !root.join("package").join(asset).is_file() {
            return Err(format!("the surface is missing {asset}").into());
        }
    }
    println!("Built the RF-7 PLAY surface.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_or_stale_ui_assets_reject_installation() {
        let root = std::env::temp_dir().join(format!(
            "rf7-ui-install-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source = root.join("source");
        let installed = root.join("installed");
        std::fs::create_dir_all(source.join("web")).unwrap();
        std::fs::create_dir_all(installed.join("web")).unwrap();
        for asset in GENERATED.iter().chain(STATIC.iter()) {
            std::fs::write(source.join(asset), asset).unwrap();
            assert!(verify_install(&installed, &source).is_err());
            std::fs::write(installed.join(asset), asset).unwrap();
        }
        assert!(verify_install(&installed, &source).is_ok());
        std::fs::write(installed.join("web/app_bg.wasm"), b"stale").unwrap();
        assert!(verify_install(&installed, &source).is_err());
        let _ = std::fs::remove_dir_all(root);
    }
}
