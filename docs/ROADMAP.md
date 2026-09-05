# Roadmap

## 0.3.0 — done

- RF-7's own front panel: a PLAY surface drawn inside RackForge on every
  host that shows plugins, in the idiom of the other RackForge instruments —
  a silkscreened chassis with knobs, membrane keys and lit glass. Four
  sections: the voice with its algorithm chart, the six operators as a
  programmer's columns, the public parameters, and the program library as
  pads. A Rust program compiled to WebAssembly, with the host's Web Plugin
  API as its only dependency and no JavaScript logic. See [The PLAY
  surface](UI.md).
- A branded package: manifest schema 3 with icon, banner and splash, so
  RackForge shows RF-7 as itself rather than under its generic identity.
- Every catalog entry is editable: a library voice opens in the editor as a
  copy, from RF-7's window and from a controller's display alike.
- RF-7 answers RackForge's standard control vocabulary: parameter schema 2
  with six published roles, so a controller's output, cutoff and LFO knobs
  reach the instrument without the player mapping anything. Three of them
  are new controls — a performance layer over the program's LFO — because a
  role pointed at nothing would be worse than a role left unclaimed.

## 0.2.0 — done

- Every parameter of a voice is edited inside RackForge, on any surface the
  host draws, with live preview: the plugin speaks the host's portable
  program-editing contract and publishes the voice as pages of fields. A
  library voice opens as a copy; a saved program reopens in place; the
  cartridge is never changed. See [Editing](EDITING.md).
- Saved programs join the catalog in a bank of their own, marked editable,
  survive a cartridge change, and are remembered by the session (state
  version 3, written only when one is selected, so older sessions still
  open in older RF-7s).
- Beside every saved program the plugin leaves the voice as a single-voice
  System Exclusive dump: the `.syx` export path, for free, from the
  contract's artifacts.
- The document format is the voice in words, so a file found on disk can be
  read without RF-7, and a hand-edited one is clamped rather than trusted.
- The editing tests run the host's own validators over everything the plugin
  emits, so the envelopes cannot drift from what the host accepts.

## 0.1.7 — done

- The factory bank retouched against the instrument's own cartridges rather
  than by ear. `rf7-lab report` measures how a cartridge's designers set their
  operators — Yamaha's modulators sit at a median of 80 and a quarter of their
  voices run one at 99; RF-7's sat at 70 and none did — and the bank was moved
  onto that distribution. `rf7-lab brightness` then compared each RF-7 voice
  with its ROM1 counterpart by spectral centroid over the attack and the body,
  which found the struck voices decaying three times too fast and the electric
  pianos getting their body from the wrong modulator. Both fixed.
- The design was too dark, not too bright: doubling the modulation index had
  not caught up with how deep the instrument's own patches go.

## 0.1.6 — done

- The modulation index is derived, not guessed: 2^(17/16) cycles at level 99,
  from the OPS datapath and a constant in the firmware, matching the
  literature. RF-7 was half as bright as the instrument before this.
- The velocity curve is the firmware's: two tables and one line of arithmetic,
  located in the user's own ROM dumps, with the instrument's own headroom.
- The output level table is confirmed byte for byte in the ROM.

## 0.1.5 — done

- `rf7-analysis`: an FFT, the Bessel functions and an estimator that reads the
  modulation index off a two-operator recording of any level, by fitting the
  sideband pattern. Recovers a known index to within one per cent; refuses a
  colliding ratio rather than averaging it.
- The calibration voice and cartridge — OP2 at 4:1 into OP1, everything else
  flat or silent, at thirty-two modulator levels — and `export-calibration`,
  the first thing RF-7 writes to a `.syx`. Its own voices only.
- `calibrate` measures RF-7 against itself; `measure-index` measures a
  recording. The recording is the step that needs a real DX7 for five minutes.
- The laboratory reads ordinary WAV files now: PCM at 16, 24 and 32 bits or
  float, any channel count. The recordings that matter come from other
  people's interfaces.

## 0.1.4 — done

- A full factory cartridge: thirty-two voices written for RF-7 across most of
  the algorithms, replacing the eight. Designed by construction and measured
  offline, not by ear; every one sounds, none clips, none is a transcription.
  They stand on the unmeasured modulation index and will be retouched when it
  is settled.

## 0.1.3 — done

- Chip images are read: a cartridge ROM dumped from its own chip has no System
  Exclusive framing and no checksum, and is now accepted on its shape alone.
- Several bulk dumps in one file are read end to end, which is how collections
  are distributed.
- Up to 128 programs instead of exactly 32, grouped into banks of thirty-two.
  A single voice dump is a library of one, and the factory library is eight
  voices rather than eight padded out with INIT VOICE.
- `--bank-order swapped` in the laboratory, because a cartridge ROM holds bank
  B in the lower half of its address space and so opens on B1.
- Program identifiers are three digits: `program-001`, not `program-01`. This
  breaks saved program selections, which is why it happened at 0.1.x.

## 0.1.2 — done

- Carriers are summed and divided by the operator count, so no single note can
  leave a voice above full scale. Real cartridge patches run four or six
  carriers near maximum, which the previous model turned into a chord peaking
  at 2.47; this was found by playing a real ROM through it rather than by
  reasoning about it.
- The default gain is now chosen against those patches rather than against this
  project's quieter factory voices.

## 0.1.1 — done

- Seventeen parameters in four pages, checked against the package schema by a
  test so the two descriptions cannot drift apart.
- The DX7's function-parameter layer: bend range, master tune, transpose, and a
  range and destination each for the wheel and for aftertouch.
- Aftertouch handled at both MIDI widths; before this it was dropped.
- Brightness, envelope time, velocity depth and the six operator switches.
- State version 2 carries every parameter, and still opens a version 1 state.

## 0.1.0 — done

- The voice data model, both DX7 byte layouts, and both System Exclusive
  containers, with the checksum and a full round trip.
- The 32 algorithms, asserted rather than drawn.
- Four-segment envelopes with the quantised rate scale, keyboard level scaling
  and keyboard rate scaling.
- Six operators with ratio, fine, detune and fixed frequency; feedback averaged
  over two samples; a pitch envelope and one global LFO with six waveforms.
- Sixteen voices, sustain pedal, bend, program change, and voice stealing that
  reports rather than hides.
- A RackForge plugin that publishes the loaded cartridge's thirty-two voices as
  its programs, plus versioned state and MIDI 1.0/2.0.
- The laboratory: render, demo, stress, inspect, cartridge, package, audition.

## Next, in the order that would improve the sound most

1. **Confirm the modulation index with a recording.** It is derived and
   three sources agree; a recording of the calibration cartridge on a real
   DX7 would close the last per cent and check the level curve end to end.
   See [Calibration](CALIBRATION.md).
2. **The envelope rate scale.** The quantisation is documented; the seconds each
   quantised rate takes are not, at either end.
4. **The pitch envelope curve**, which is currently a quadratic standing in for
   a table.
5. **A fixed-point operator kernel**: the logarithmic sine and the exponential
   output table the OPS chip uses. The quantisation that introduces is audible,
   and the kernel is already one function so nothing above it moves.
6. **Detune as a constant phase increment** rather than a constant interval,
   which is what makes it wider at the bottom of the keyboard.

## Then

- Portamento and a mono/legato voice mode. Both are real DX7 controls and both
  need genuine work in the allocator: last-note priority, a glide in the
  logarithmic frequency domain, and a decision about whether a legato note
  retriggers its envelopes, which on the instrument it does not.
- A ZIP import container, so a folder of `.syx` files can be installed in one
  step instead of one cartridge at a time.
- `parallel_render_v1`: sixteen voices split cleanly across audio workers, and
  this instrument is exactly the shape that contract was written for.
- A whole-library `.syx` export — the thirty-two-voice bulk dump — for the
  saved programs together; each one already leaves its own single-voice dump.
- A voice name field in the editor, when the host's generic editor grows a
  text kind; today the name is the program's name in the host.
