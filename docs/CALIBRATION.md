# Calibration

The one number RF-7 cannot take from a cartridge is how deep the modulation
is: the phase deviation a modulator at full output level produces in the
operator it feeds. Every FM parameter is a ratio hanging from that scale, so a
wrong value here makes every patch wrong in the same direction, and every patch
tuned by ear against it would have to be tuned again once it moved. The model
ledger marks it *approximate*; this document is how it stops being that.

## Why a recording of unknown level is enough

Two operators, one modulating the other, produce a spectrum that is known in
closed form. With a carrier at *f꜀* and a modulator at *fₘ*, the lines sit at
*f꜀ ± n·fₘ* and the amplitude of the *n*-th is **Jₙ(β)**, the Bessel function
of the modulation index β. The *ratios* between lines depend on β and on
nothing else — not the recording level, not the interface, not the gain
staging. So β can be read off a recording nobody normalised.

The estimator in `rf7-analysis` reads the lines at every order from −8 to +8,
normalises them to unit power, and finds the β whose normalised Bessel pattern
fits best. It reports a residual with the answer: below a few hundredths, the
recording was a clean two-operator tone and the number can be trusted.

## The calibration voice

OP1 is the carrier at 1:1 and level 99. OP2 modulates it at **4:1**, so every
sideband lands on its own odd harmonic — 1, 5, 9, 13 above and 3, 7, 11 folded
back from below — and none collides with another. A 1:1 ratio would fold order
−2 onto the carrier itself, which is why it is refused. Envelopes are flat,
velocity and keyboard scaling are off, the LFO does nothing, the other four
operators are silent.

The calibration cartridge holds that voice at thirty-two modulator levels, 99
down to 6 in steps of three, so one session on a real instrument measures the
whole level curve — which is also marked *approximate* — and not only its top.

## Getting the recording

```text
cargo run --release -p rf7-lab -- export-calibration --output renders/rf7-calibration.syx
```

Send the cartridge to a DX7. For each voice, hold **middle C** for ten
seconds and record the line output dry: no reverb, no compressor, no
normalisation afterwards. Any sample rate and bit depth an ordinary recorder
produces will do; RF-7 reads 16, 24 and 32-bit PCM and float, at any channel
count. Name the files by level.

Nobody involved needs to own the instrument. Someone who does needs five
minutes.

## Measuring

```text
cargo run --release -p rf7-lab -- measure-index --input recordings/level-99.wav --output renders/level-99.json
cargo run --release -p rf7-lab -- calibrate --output renders/rf7-calibrate.json
```

`measure-index` gives β for a recording, in radians and in cycles, with the
line-by-line fit. `calibrate` is RF-7 measuring itself: β at every level of the
cartridge, rendered offline through the same estimator. The two tables side by
side are the calibration. Where they differ at level 99 is the modulation
index; where they differ in shape is the level curve.

Today `calibrate` at level 99 reports 2π radians — one cycle — because that is
what `MODULATION_CYCLES` is set to. That is RF-7's claim, not a measurement of
the DX7.

## What is asserted

- The Bessel functions against tabulated values, and Parseval's sum to one.
- A synthetic phase-modulated sine at indexes from 0.3 to 14 is recovered to
  within one per cent, at any recording level.
- A colliding ratio, a line above Nyquist, and silence are refused with a
  reason, not averaged into a number.
- RF-7's own calibration voice at level 99 measures 2π, and at 90, 80 and 70 it
  follows the level tables. If the engine and its tables ever disagree, that
  test is the one that says so.
