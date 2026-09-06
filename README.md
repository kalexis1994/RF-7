# RF-7

A six-operator FM instrument for RackForge, written in Rust.

The 32 algorithms, four-segment envelopes on the instrument's own quantised rate
scale, keyboard level and rate scaling, per-operator ratios with fine and
detune, fixed-frequency operators, feedback, a pitch envelope and one global LFO
with six waveforms. Sixteen voices. It reads the DX7 cartridges you already own
and compiles to a portable RackForge WASM plugin.

**What is modelled is written down, and where it came from.** The structure
is taken from documented behaviour and asserted in tests; the tables that
turn a panel number into a level, a rate, a pitch or a speed are read from
the instrument's own firmware — its envelope, keyboard scaling, pitch
envelope and LFO arithmetic, its rate and level tables — or from published
measurements of the hardware, and the factory voices are measured against
the cartridges and against a recorded grand. [The model ledger](docs/MODEL.md)
says which is which, one line per mapping, and names the two numbers that
only a recording of a DX7 would settle: the timer's tick, and how deep one
step of amplitude modulation sensitivity goes.

**No voice data ships here.** The thirty-eight factory voices were written for RF-7.
The voices a DX7 shipped with are Yamaha's; RF-7 plays a cartridge you supply.

## Controls

Twenty-six parameters, in five pages. They are offsets on the loaded program,
not a second copy of it, and every one of them is neutral at its default:

- **Output** — output gain.
- **Performance** — bend range, master tune, transpose, a range and
  destination each for the modulation wheel, aftertouch, the breath controller
  and the foot controller, the voice mode and the portamento time. This is the
  DX7's own function-parameter layer, which never lived in a cartridge. Each
  controller reaches pitch, amplitude, both, or the envelope bias — the
  destination a breath controller is for, where the note sits below its
  programmed level until the player brings it up.
- **Voice** — brightness, envelope time and velocity depth. These the DX7 did
  not have; they are one-knob ways to move a whole cartridge at once, next to
  the editor that moves one voice. Brightness scales the modulation index of
  every operator together, which is also the quickest way to hear the one
  constant the model ledger says a recording would confirm.
- **LFO** — rate, depth and delay over the program's own: a factor on its
  speed, vibrato added to its depth, seconds added to its delay. A program
  with no vibrato of its own answers to these.
- **Operators** — six switches, one per operator.

In **mono** the instrument plays one note at a time with last-note priority:
a key played over another takes the voice without starting its envelopes
again, and releasing it hands the voice back to whichever key is still down.
**Portamento** glides between notes on the instrument's own 0–99 dial, in
poly as well as mono, and controller 65 switches a glide that is set off and
on. Controllers 7 and 11 — channel volume and expression — scale the output,
and a program change past the library's last slot reaches the programs saved
from the editor, in catalog order.

RF-7 publishes six of RackForge's standard control roles, so a controller's
knobs find them without the player mapping anything: `plugin.output.level`
and `synth.amplifier.level` on the output gain, `synth.filter.cutoff` on
brightness — an FM instrument has no filter, and brightness is what that knob
is for — and the three `synth.lfo.*` roles on the LFO layer. The roles RF-7
cannot honour are left unclaimed rather than pointed at something that only
resembles them.

## The window

RF-7 draws its own front panel inside RackForge — a silkscreened chassis with
knobs, membrane keys and lit glass, in the idiom of the other RackForge
instruments. Four sections: the voice with its algorithm chart, the six
operators as a programmer's columns, the public parameters, and the program
library as pads. It is a Rust program compiled to WebAssembly, with no
JavaScript logic. See [The PLAY surface](docs/UI.md).

## Editing

Every parameter of a voice — the algorithm, the six operators, their
envelopes, scaling and frequencies, the pitch envelope and the LFO — is edited
inside RackForge, in RF-7's own window or on a controller's display, with
live preview. A library voice opens as a copy and the cartridge is never
touched; a saved program reopens in place. Beside each saved program the
plugin leaves the voice as a single-voice System Exclusive dump, so it can go
to hardware. See [Editing](docs/EDITING.md).

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
cargo run --release -p rf7-lab -- calibrate
cargo run --release -p rf7-lab -- export-calibration --output renders/rf7-calibration.syx
```

Requires Rust 1.98, `wasm-bindgen-cli` 0.2.127 for the surface, and a sibling
RackForge checkout for its public SDK. See [Development](docs/DEVELOPMENT.md). Existing audio and report files are never
overwritten.

## What is here

- `rf7-voice`: the voice parameter model, both DX7 byte layouts, and the two
  System Exclusive containers with their checksum. No audio, no I/O.
- `rf7-dsp`: the 32 algorithms, the envelopes, the operators, the LFO and a
  sixteen-voice engine. No allocation, locks or I/O once constructed.
- `rf7-analysis`: an FFT, Bessel functions and the modulation-index estimator,
  with no dependencies. Reads the index off a two-operator recording of any
  level.
- `rf7-plugin`: the RackForge adapter, with MIDI 1.0 and 2.0, versioned state,
  a program catalog built from the installed cartridge and the saved programs,
  and the declarative voice editor the host draws.
- `rf7-ui`: the PLAY surface, Rust compiled to WebAssembly: the host's
  context read into state, state drawn as HTML, the algorithm drawn as SVG.
- `rf7-lab`: rendering, WAV, JSON reports, cartridge inspection, packaging and
  the Desktop audition workflow.

Tests cover the algorithm table's invariants, envelope ordering and release,
absolute pitch, cartridge round trips and refusals, voice stealing, what the
plugin does with a malformed block, and the editing contract end to end with
the host's own validators reading every envelope the plugin emits.

## Cartridges

Install one from RF-7's SETUP surface, which RackForge opens from its Plugins
section: the host's own explorer chooses the file and the host installs it.
RF-7 reads System Exclusive dumps, several of them in one file, a single voice
on its own, and cartridge ROM images taken straight off the chip with no
framing or checksum at all. Their voices become the plugin's programs — up to
128, grouped into banks of thirty-two and named as the cartridge names them.
Bytes outside the documented ranges are clamped and counted rather than
silently accepted or used as a reason to refuse the file.
See [Cartridges](docs/CARTRIDGES.md).

## Licence

RF-7 is distributed under the GNU General Public License, version 3 only
(`GPL-3.0-only`); see [LICENSE](LICENSE) and [NOTICE.md](NOTICE.md). The
package RackForge bundles carries both. RF-7 and RackForge are separate
components: the plugin talks to the host through its public WebAssembly ABI
and contains no host code.

## Read next

- [Model ledger](docs/MODEL.md): every mapping, and whether it is documented or
  an approximation waiting for a measurement.
- [Roadmap](docs/ROADMAP.md): what is done, and what would improve the sound
  most, in that order.
- [Cartridges](docs/CARTRIDGES.md): what RF-7 reads and how it is installed.
- [The PLAY surface](docs/UI.md): RF-7's own window, what it is made of and
  how it talks to the host.
- [Editing](docs/EDITING.md): the voice editor inside RackForge, and what a
  saved program leaves on disk.
- [Calibration](docs/CALIBRATION.md): how the modulation index gets measured,
  and what a real DX7 has to record for it.
- [Sources](docs/SOURCES.md): what the structure was written from.
- [Development](docs/DEVELOPMENT.md): commands, integration and output formats.
- [Desktop audition](docs/AUDITION.md): build, install and launch a test version.

All project code, tools, tests and documentation are in English; executable
project code is Rust.
