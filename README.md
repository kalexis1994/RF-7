# RF-7

A six-operator FM instrument for RackForge, written in Rust.

The 32 algorithms, four-segment envelopes on the instrument's own quantised rate
scale, keyboard level and rate scaling, per-operator ratios with fine and
detune, fixed-frequency operators, feedback, a pitch envelope and one global LFO
with six waveforms. Sixteen voices. It reads the DX7 cartridges you already own
and compiles to a portable RackForge WASM plugin.

**This is an uncalibrated engine, not a measured DX7 recreation.** The structure
is taken from documented behaviour and asserted in tests. Several of the curves
that turn a panel number into a frequency or a slope are RF-7's own
approximations, and [the model ledger](docs/MODEL.md) says which is which,
one line per mapping. The largest open question is the modulation index, the one
constant that sets how bright the whole instrument is.

**No voice data ships here.** The eight factory voices were written for RF-7.
The voices a DX7 shipped with are Yamaha's; RF-7 plays a cartridge you supply.

## Quick start

To build, validate, install and open the current instrument in RackForge Desktop
on Windows:

```text
cargo run --locked --release -p rf7-lab -- audition
```

The [audition workflow](docs/AUDITION.md) keeps a dedicated test library,
retains audio and MIDI preferences and supports repeated builds of one version.

```text
cargo test --locked --workspace
cargo run --release -p rf7-lab -- demo --output renders/demo.wav
cargo run --release -p rf7-lab -- render --output renders/tines.wav --program 1
cargo run --release -p rf7-lab -- inspect renders/demo.wav
cargo run --release -p rf7-lab -- cartridge cartridges/mine.syx
cargo run --release -p rf7-lab -- stress
```

Requires Rust 1.98 and a sibling RackForge checkout for its public SDK. See
[Development](docs/DEVELOPMENT.md). Existing audio and report files are never
overwritten.

## What is here

- `rf7-voice`: the voice parameter model, both DX7 byte layouts, and the two
  System Exclusive containers with their checksum. No audio, no I/O.
- `rf7-dsp`: the 32 algorithms, the envelopes, the operators, the LFO and a
  sixteen-voice engine. No allocation, locks or I/O once constructed.
- `rf7-plugin`: the RackForge adapter, with MIDI 1.0 and 2.0, versioned state,
  and a program catalog built from the installed cartridge.
- `rf7-lab`: rendering, WAV, JSON reports, cartridge inspection, packaging and
  the Desktop audition workflow.

Tests cover the algorithm table's invariants, envelope ordering and release,
absolute pitch, cartridge round trips and refusals, voice stealing, and what the
plugin does with a malformed block.

## Cartridges

RF-7 accepts a 4104-byte cartridge dump or a 163-byte single-voice dump. A
cartridge's thirty-two voices become the plugin's thirty-two programs, named as
the cartridge names them. Bytes outside the documented ranges are clamped and
counted rather than silently accepted or used as a reason to refuse the file.
See [Cartridges](docs/CARTRIDGES.md).

## Read next

- [Model ledger](docs/MODEL.md): every mapping, and whether it is documented or
  an approximation waiting for a measurement.
- [Roadmap](docs/ROADMAP.md): what is done, and what would improve the sound
  most, in that order.
- [Cartridges](docs/CARTRIDGES.md): what RF-7 reads and how it is installed.
- [Sources](docs/SOURCES.md): what the structure was written from.
- [Development](docs/DEVELOPMENT.md): commands, integration and output formats.
- [Desktop audition](docs/AUDITION.md): build, install and launch a test version.

All project code, tools, tests and documentation are in English; executable
project code is Rust.
