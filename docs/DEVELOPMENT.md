# Development

## Toolchain

Rust 1.98.0 is pinned. The DSP and the voice model have no third-party
dependencies at all; the laboratory uses `serde_json` for its reports, and the
plugin uses the public RackForge SDK from a sibling `rackforge` checkout through
an explicit Cargo path. A local path dependency is not a reproducible
distribution pin: before an external release, replace it with a published
version or an exact Git revision and regenerate `Cargo.lock`.

On this Windows GNU setup, put `C:/msys64/ucrt64/bin` on the shell's PATH so
Rust finds the linker. No machine-wide change is needed.

```powershell
$env:Path = 'C:/msys64/ucrt64/bin;' + $env:Path
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
cargo build --locked --release --workspace
cargo build --locked --release --target wasm32-unknown-unknown -p rf7-plugin
```

## Render and inspect

```text
cargo run --release -p rf7-lab -- render --output renders/tines.wav --program 1
cargo run --release -p rf7-lab -- demo --output renders/demo.wav
cargo run --release -p rf7-lab -- inspect renders/demo.wav
cargo run --release -p rf7-lab -- stress
cargo run --release -p rf7-lab -- cartridge cartridges/mine.syx
```

`--help` lists every render option. Output files use create-new semantics:
choose a new name to rerun, because nothing here overwrites. Each render also
writes a JSON report beside the WAV.

The WAV has no normalisation and no clipping, and its report gives the peak and
the RMS. Sixteen voices of a six-carrier patch sum past full scale; that shows
up in the report rather than being hidden by a limiter. Use host gain when
auditioning.

## Package

For the whole build, install and launch cycle use
`cargo run --locked --release -p rf7-lab -- audition`. See
[Desktop audition](AUDITION.md).

The standalone `package` command produces a versioned archive without launching
a host. It needs RackForge's own tools built first, in the sibling checkout:

```text
cargo build --locked --release -p rackforge-store -p rackforge-core
```

Then, from here:

```text
cargo run --release -p rf7-lab -- package
```

The laboratory copies the current WASM into the ignored `package/component.wasm`,
validates the metadata and smoke-tests it through the host, and creates the
archive only after both succeed. It never overwrites an existing archive.

## Layout

```text
crates/rf7-voice/    voice parameters, both byte layouts, System Exclusive
crates/rf7-dsp/      algorithms, envelopes, operators, LFO, engine
crates/rf7-plugin/   SDK adapter, MIDI validation, program catalog, state
tools/rf7-lab/       rendering, WAV, reports, packaging, audition
package/             RackForge manifest and metadata
docs/                design, model ledger and development notes
renders/             ignored generated WAV and JSON
cartridges/          ignored; your own System Exclusive files
dist/                ignored distributable and validation output
```

Unsafe Rust is forbidden across the workspace. The plugin's export macro
contains the SDK's own raw ABI implementation; every handwritten line here is
safe Rust. Event lists are validated before anything is mutated, and an invalid
block is silenced without a partial edit.
