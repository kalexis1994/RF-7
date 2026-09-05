# Model ledger

Every number RF-7 turns a panel parameter into, with a verdict on each: whether
it is a documented property of the instrument, or an RF-7 approximation still
waiting for a measurement. The verdict matters more than the number. A wrong
number that is labelled *approximate* costs an afternoon; a wrong number that is
labelled *documented* costs a week of looking somewhere else.

All of it lives in [`crates/rf7-dsp/src/tables.rs`](../crates/rf7-dsp/src/tables.rs)
and [`crates/rf7-dsp/src/algorithm.rs`](../crates/rf7-dsp/src/algorithm.rs).

## Structure

| Fact | Verdict | Note |
| --- | --- | --- |
| Six operators, sixteen voices | Documented | Not parameters. |
| 32 algorithms: carriers, routing, feedback operator | Documented | Cross-checked against the Faust `dx7.lib` algorithm definitions and against independent descriptions of algorithms 1, 5 and 32. Algorithms 4 and 6 are the two whose feedback loop spans more than one operator, and the tests assert exactly that. |
| Modulators always outnumber their destination | Documented | Asserted, and it is why one descending pass renders any algorithm. |
| A single global LFO for the whole instrument | Documented | Not one per voice. |
| Cartridge and voice-dump byte layouts, checksum | Documented | Round-tripped in tests; a real cartridge either opens or is refused with a reason. |

## Frequency

| Mapping | Verdict | Note |
| --- | --- | --- |
| Coarse 0 is the half ratio, 1..31 are whole ratios | Documented | |
| Fine adds hundredths of the coarse ratio | Documented | |
| Fixed mode is four decades from 1 Hz, `10^(coarse mod 4 + fine/100)` | Documented | |
| Transpose byte 24 is centre | Documented | |
| Detune step ≈ 1.7 cents | **Approximate** | Applied as a constant factor in the logarithmic domain. On the real instrument detune is a fixed increment on the phase accumulator, so it is a larger interval on low notes than on high ones. Not yet modelled. |
| Pitch bend range | Documented, but not from the voice | It lives in the DX7's function parameters, not in a patch, so RF-7 exposes it as a control. See below. |

## Level

The engine counts levels in *units*: 256 units to a doubling of amplitude, so
the 4064-unit range spans a little over 96 dB.

| Mapping | Verdict | Note |
| --- | --- | --- |
| Output level 0..99 to the 0..127 scale, with a table below 20 | **Documented — read from the ROM** | `TABLE_LOG` in the v1.8 firmware (at 0x1CFE of the user's own dump, 0x2361 of the YA2138 mask ROM) is exactly 127 minus this scale, entry for entry. The firmware stores it doubled, so one step above level 20 is one eighth of an octave: the 32 units at 256 per octave used here. |
| Keyboard level scaling: break point, two depths, four curves | Documented | Break point anchored 17 semitones up the MIDI scale, groups of three semitones, the published exponential table. The anchor in particular deserves a measurement. |
| Velocity sensitivity 0..7 | **Documented — read from the ROM** | Two firmware tables and one line of arithmetic, in sixteenths of an octave: `((sensitivity × 32 × SCALE[MIDI[v]]) >> 8) + (15 − 2 × sensitivity)`. Sensitivity 0 is the constant 15, the reference. Sensitivity 7 spans from fourteen sixteenths *above* it at full velocity to ninety-eight below at velocity 0 — about 42 dB. The firmware clamps the attenuation byte at 4, which leaves eleven sixteenths of headroom above the reference; RF-7 keeps exactly that, so the last three sixteenths at sensitivity 7 are clamped as the instrument clamps them. |
| Amplitude modulation sensitivity 0..3 | **Approximate** | Modelled as 0, ¼, ½ and all of a 24 dB dip. |
| Carriers are summed and divided by six | **Approximate, but constrained** | The instrument's operator sum reaches a fixed-width accumulator and a 12-bit converter, so six operators at full have to fit: dividing by the operator count is that constraint rather than a taste decision. Whether the hardware's own scaling is exactly this has not been measured. Without it a four-carrier patch — which is ordinary on a real cartridge, not extreme — is four times louder than a single-carrier one and clips on any chord. |

## Envelopes

| Mapping | Verdict | Note |
| --- | --- | --- |
| Four rates and four levels, third level held, fourth on release | Documented | An envelope whose fourth level is not silence keeps sounding after the key is up, and RF-7 lets it, exactly as the instrument does. |
| Rates quantise to 64 speeds, `(rate × 41) / 64` | Documented | Two neighbouring panel rates often give an identical envelope. That is the instrument, not a shortcut. |
| Keyboard rate scaling `(sensitivity × clamp(note/3 − 7, 0, 31)) / 8` | Documented | |
| Four quantised steps double the speed | **Approximate** | The base is set so the fastest full sweep is about 1.2 ms and the slowest about 63 s. Both ends want measuring. |
| Rising segments slow as they approach the top | **Approximate** | An exponential approach to a ceiling 21% above unity, which reproduces the shape but not a measured curve. |
| Pitch envelope, level 50 as centre | **Approximate** | A quadratic curve reaching four octaves at the extremes: gentle near centre, steep at the ends. The real table is not this. |

## Modulation

| Mapping | Verdict | Note |
| --- | --- | --- |
| **Modulation index at level 99: 2^(17/16) cycles, π·2^(33/16) = 13.12 rad** | **Documented — derived, three sources agree** | The OPS adds the 14-bit operator output onto the 12-bit sine index, so full scale is four cycles (Shirriff's die analysis). The firmware never sends full scale: its velocity term at sensitivity 0 is a constant 15 sixteenths of an octave on every operator (read from the ROM). 4 × 2^(−15/16) = 2^(17/16) cycles, which is the π·2^(33/16) the literature quotes for the instrument and within four per cent of the DDX7 paper's 4π. Until 0.1.6 this was 1.0, and the instrument was half as bright as the DX7. A recording of the calibration cartridge — see [Calibration](CALIBRATION.md) — would confirm it to the last per cent; RF-7 measures itself at exactly this value. |
| Feedback level 0..7 as powers of two, 7 being full | **Approximate** | Averaged over the last two samples, which is what stops a feedback operator oscillating at the Nyquist frequency. |
| LFO waveforms | Documented | Triangle, saw down, saw up, square, sine, sample and hold. The synced triangle starts at zero. |
| LFO speed 0..99 to about 0.06..47 Hz | **Approximate** | Exponential between the two documented endpoints. The instrument's own curve is not exponential throughout. |
| LFO delay to 0..4 s, then a fade | **Approximate** | |
| Pitch modulation sensitivity 0..7 | **Approximate** | A published-shaped table scaled to ±12 semitones at full depth. |

## Controls

The seventeen parameters RackForge draws are not voice data and never touch a
cartridge: they are offsets on top of whatever program is loaded. Every one of
them is neutral where it starts, so a cartridge plays exactly as programmed
until something is moved, and a test asserts that.

| Control | Verdict | Note |
| --- | --- | --- |
| Bend range, master tune, transpose | Documented, but not from the voice | The DX7 keeps all three among its function parameters, outside the cartridge, which is exactly where RF-7 keeps them. |
| Mod wheel range and target | Documented, in part | The DX7 assigns each controller to pitch, amplitude or the envelope bias with its own range. RF-7 offers the first two destinations and not the third. |
| Aftertouch range and target | Documented, in part | Same layer, same two destinations. |
| **Brightness** | **RF-7 addition** | Scales the modulation index of every operator at once. The DX7 had no such control: on the instrument you would reach for six output levels. It exists because a cartridge cannot be edited yet, and it is the fastest way to hear what the modulation index does — the number the ledger above says is unmeasured. |
| **Envelope time** | **RF-7 addition** | Stretches every segment of every envelope by one factor. Read once, when a note starts. |
| **Velocity depth** | **RF-7 addition** | Scales the velocity offset the patch already asks for. At 0 the instrument stops answering to velocity entirely. |
| **Operator switches** | Documented as a panel action, not as a control | The DX7 can silence an operator from its front panel; it is not a voice parameter and not continuous. Muting one here changes nothing about how long a note lives. |

The three additions are marked because a reader should be able to tell at a
glance which controls would exist on the hardware and which are RF-7 making a
fixed cartridge usable. None of them is a substitute for editing a voice, which
is a separate milestone.

## The sine

A 4096-point table with linear interpolation, error below 10⁻⁵. The real OPS chip
takes a logarithmic sine and an exponential output table, and the quantisation
that introduces is audible. RF-7 does not model it yet; the operator kernel is
one function so it can be replaced without touching anything above it.

## What is deliberately not modelled

- The DAC. The DX7 output stage and its converter are part of the sound.
- Envelope level quantisation in the fixed-point domain the hardware uses.
- The internal 49096 Hz sample rate. RF-7 runs at the host's rate.
- Portamento, glissando and the function-parameter layer generally.
