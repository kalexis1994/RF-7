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
| Detune | Measured on a DX7, by the Dexed project | The firmware hands the EGS a sign and a magnitude (`TABLE_DETUNE_VALUE`) and the chip applies it, so no table says how far a step goes. The Dexed author measured a DX7 and fitted one step to `0.0209 / 7 × log2(f) × e^(−0.396 log2 f)` octaves at a key of `f` hertz: 2.6 cents at A0, 1.2 at middle C, half a cent at C7 — the beat between two detuned operators grows with pitch, but more slowly than the pitch. RF-7 applies that curve at the key sounded, the same for every operator of the note as the measurement was made, and gives a fixed operator the curve at its own frequency. It replaces a constant 1.7 cents a step. |
| Pitch bend range | Documented, but not from the voice | It lives in the DX7's function parameters, not in a patch, so RF-7 exposes it as a control. See below. |

## Level

The engine counts levels in *units*: 256 units to a doubling of amplitude, so
the 4064-unit range spans a little over 96 dB.

| Mapping | Verdict | Note |
| --- | --- | --- |
| Output level 0..99 to the 0..127 scale, with a table below 20 | **Documented — read from the ROM** | `TABLE_LOG` in the v1.8 firmware (at 0x1CFE of the user's own dump, 0x2361 of the YA2138 mask ROM) is exactly 127 minus this scale, entry for entry. The firmware stores it doubled, so one step above level 20 is one eighth of an octave: the 32 units at 256 per octave used here. |
| Keyboard level scaling: break point, two depths, four curves | Documented, from the firmware | `PATCH_ACTIVATE_OPERATOR_KBD_SCALING_LEVEL` builds a forty-three-entry curve per operator: the break point plus twenty and the key sounded — the note played plus the voice's transpose — are both looked up on the key-to-pitch table and shifted down two bits, which makes groups of three keys with the break point's own group neutral (break point 39, C3, is keys 58 to 60); the distance in groups indexes the firmware's linear or exponential curve table, both carried verbatim, and the value is multiplied by the depth scaled to 255 at 99, keeping the top byte and clamping at 127. The result is added to, or taken from, the operator's logarithmic level before the velocity term. The earlier implementation had the neutral group three keys low and a reconstructed exponential table that diverged above the fifteenth group. |
| Velocity sensitivity 0..7 | **Documented — read from the ROM** | Two firmware tables and one line of arithmetic, in sixteenths of an octave: `((sensitivity × 32 × SCALE[MIDI[v]]) >> 8) + (15 − 2 × sensitivity)`. Sensitivity 0 is the constant 15, the reference. Sensitivity 7 spans from fourteen sixteenths *above* it at full velocity to ninety-eight below at velocity 0 — about 42 dB. The firmware clamps the attenuation byte at 4, which leaves eleven sixteenths of headroom above the reference; RF-7 keeps exactly that, so the last three sixteenths at sensitivity 7 are clamped as the instrument clamps them. |
| Amplitude modulation sensitivity 0..3 | **Approximate** | Modelled as 0, ¼, ½ and all of a 24 dB dip. |
| Carriers are summed and divided by six | **Approximate, but constrained** | The instrument's operator sum reaches a fixed-width accumulator and a 12-bit converter, so six operators at full have to fit: dividing by the operator count is that constraint rather than a taste decision. Whether the hardware's own scaling is exactly this has not been measured. Without it a four-carrier patch — which is ordinary on a real cartridge, not extreme — is four times louder than a single-carrier one and clips on any chord. |

## Envelopes

| Mapping | Verdict | Note |
| --- | --- | --- |
| Four rates and four levels, third level held, fourth on release | Documented | An envelope whose fourth level is not silence keeps sounding after the key is up, and RF-7 lets it, exactly as the instrument does. |
| Rates quantise to 64 speeds, `(rate × 41) / 64` | Documented | Two neighbouring panel rates often give an identical envelope. That is the instrument, not a shortcut. |
| Seconds per rate | Documented, measured on the hardware | The MSFA project's measurements of a DX7 (its `Dx7Envelope` notes): at the slowest quantised rate the level moves one 0.0235 dB step every 4096 samples of the 49096 Hz clock — 0.28 dB a second — and four quantised steps double the speed, with the steps between linear, `(1 + (q mod 4) / 4) × 2^(q div 4)`. A decay is a straight line in decibels: 90 dB in 1.25 s at rate 50, 4 s at 40, 40 s at 20. RF-7's level unit is that step, so the table is used as measured. Before 0.4.0 the engine ran five times too fast on decays and three times too slow on attacks, which is why its pianos died and its strings smeared. |
| The attack's shape | Documented, measured on the hardware | A rising segment multiplies the decay's step by `2 + ⌊(4095 − level) / 256⌋`, so it is fast from the bottom and slows towards the top — roughly linear in decibels — and it starts 1700 steps (40 dB) above the operator's floor, skipping the inaudible bottom of every attack. Rate 50 reaches a decibel under full in 0.16 s. |
| Keyboard rate scaling `(sensitivity × clamp(note/3 − 7, 0, 31)) / 8` | Documented | |
| Four quantised steps double the speed | **Approximate** | The base is set so the fastest full sweep is about 1.2 ms and the slowest about 63 s. Both ends want measuring. |
| Rising segments slow as they approach the top | **Approximate** | An exponential approach to a ceiling 21% above unity, which reproduces the shape but not a measured curve. |
| Pitch envelope levels | Documented, from the firmware | `TABLE_PITCH_EG_LEVEL`, verbatim: a level to a byte whose top seven bits are added to the voice's pitch word, which counts 4096 to the octave, so one step of the table is 3/8 of a semitone. 50 is the centre, the table is one step a level from 18 to 85 and steepens at the ends, and 0 and 99 are four octaves down and up. A quadratic curve stood here before; it was a third of the real deviation through the middle. |
| Pitch envelope rates | Documented in part, from the firmware | `PITCH_EG_PROCESS` adds `TABLE_PITCH_EG_RATE[rate]` — the same table the portamento reads — to the pitch word on every second timer tick, and a segment ends when it reaches or crosses its level: a straight line in the logarithmic pitch domain at a speed that does not depend on the distance. Rate 99 moves 140 semitones a second, rate 50 twenty-two, rate 0 half of one. The tick is the same inferred 187 a second as the portamento's. Before this the pitch envelope borrowed the operators' rate curve. |

## Modulation

| Mapping | Verdict | Note |
| --- | --- | --- |
| **Modulation index at level 99: 2^(17/16) cycles, π·2^(33/16) = 13.12 rad** | **Documented — derived, three sources agree** | The OPS adds the 14-bit operator output onto the 12-bit sine index, so full scale is four cycles (Shirriff's die analysis). The firmware never sends full scale: its velocity term at sensitivity 0 is a constant 15 sixteenths of an octave on every operator (read from the ROM). 4 × 2^(−15/16) = 2^(17/16) cycles, which is the π·2^(33/16) the literature quotes for the instrument and within four per cent of the DDX7 paper's 4π. Until 0.1.6 this was 1.0, and the instrument was half as bright as the DX7. A recording of the calibration cartridge — see [Calibration](CALIBRATION.md) — would confirm it to the last per cent; RF-7 measures itself at exactly this value. |
| Feedback level 0..7 as powers of two, 7 being full | **Approximate** | Averaged over the last two samples, which is what stops a feedback operator oscillating at the Nyquist frequency. |
| LFO waveforms | Documented, from the firmware | Triangle, saw down, saw up, square, sine, sample and hold, each read from the phase word the way `LFO_GET_AMPLITUDE` reads it: the triangle starts at its bottom and peaks halfway, so a synced triangle vibrato begins a full depth below the note; the saws start at their centre and wrap halfway; the square is high first; the sine starts at zero; sample-and-hold draws on every wrap. |
| LFO speed | Documented in part, from the firmware | `PATCH_ACTIVATE_SCALE_LFO_SPEED`: the speed scaled to 0..=255 times 11 is added to a sixteen-bit phase word on every timer tick, and from a scaled 160 up the multiplier climbs by one every four steps, which bends the top of the dial upwards. At the inferred 374 ticks a second that is 0.063 Hz at 0, 0.126 at 1, 49.5 at 99; the literature's 47 Hz at 99 would make the tick 355, and that four per cent is the one open number in every timing here. |
| LFO delay and fade-in | Documented in part, from the firmware | `PATCH_ACTIVATE_SCALE_LFO_DELAY`: with `x = 99 − delay`, an increment of `(16 + x mod 16) << (2 + x / 16)` — a mantissa and an exponent, doubling every sixteen steps of the dial — is added to a sixteen-bit word every tick; when it overflows the fade counter climbs to 255 by the increment's top byte a tick, so the fade lasts about as long as the delay. Delay 99 waits 2.7 s and fades over 0.7; delay 0 still waits 36 ms. The same inferred tick. |
| Pitch modulation sensitivity 0..7 | Documented, from the firmware | `TABLE_PITCH_MOD_SENS` {0, 10, 20, 33, 55, 92, 153, 255}. `MOD_PITCH_LOAD_TO_EGS` multiplies it by the LFO's amplitude, then by the depth (with the fade and the controllers), and halves the result into a register that the pitch bend of range 12 fills to 0x3FFF at an octave: so sensitivity 7 at depth 99 is ±11.8 semitones, which is where RF-7's ±12 already sat. |

## Controls

The seventeen parameters RackForge draws are not voice data and never touch a
cartridge: they are offsets on top of whatever program is loaded. Every one of
them is neutral where it starts, so a cartridge plays exactly as programmed
until something is moved, and a test asserts that.

| Control | Verdict | Note |
| --- | --- | --- |
| Bend range, master tune, transpose | Documented, but not from the voice | The DX7 keeps all three among its function parameters, outside the cartridge, which is exactly where RF-7 keeps them. |
| Mod wheel range and target | Documented | The DX7 assigns each controller to pitch, amplitude or the envelope bias with its own range. RF-7 offers all three. |
| Aftertouch, breath controller and foot controller | Documented | The same layer, one reach and one destination each. The breath controller is controller 2 and the foot controller is controller 4, which is where the instrument read them. |
| Envelope bias | Documented, in part | On the instrument the bias reaches the operators through their amplitude modulation sensitivity, and so it does here: an operator whose sensitivity is zero does not answer, as it did not. At rest the sensitive operators sit the controller's whole reach below their programmed level and full travel gives the program back as written. The DX7's exact curve from sensitivity to level is not measured; the amplitude modulation's own depth per sensitivity step is used for both. |
| Channel volume and expression | Not the instrument's | Controllers 7 and 11 scale the output. The DX7 had a volume slider and no MIDI volume; a keyboard today sends both and expects them answered. |
| **Brightness** | **RF-7 addition** | Scales the modulation index of every operator at once. The DX7 had no such control: on the instrument you would reach for six output levels. It moves a whole cartridge at once where the editor moves one voice, and it is the fastest way to hear what the modulation index does — the number the ledger above says a recording would confirm. |
| **Envelope time** | **RF-7 addition** | Stretches every segment of every envelope by one factor. Read once, when a note starts. |
| **LFO rate** | **RF-7 addition** | A factor on the speed the program asks for, from a quarter to four times. The program's own speed curve is unchanged; this multiplies its result. |
| **LFO depth** | **RF-7 addition** | Vibrato added to the program's own depth, at the same point the modulation wheel adds its own, so a program with none can still be given some. |
| **LFO delay** | **RF-7 addition** | Seconds added to the program's delay, because a delay of zero cannot be scaled into existence. |
| The brass family's architecture | Measured against the cartridges, and Yamaha's own programming notes | Ensemble brass is algorithm 22: one modulator at 1:1 with full feedback — a sawtooth, in effect — behind three carriers at 1:1 detuned a few cents apart, beside a sub-octave pair; the modulator blooms in after the carriers and then holds, darkened away from middle C by its keyboard scaling. Solo brass is one carrier over a 1:1 feedback core, a 1:1 modulator that spits and dies, an inharmonic partial near 3.7:1 under velocity for the bite, and a fixed low-frequency flutter, with a scoop into the note and a vibrato that arrives after it. RF BRASS, RF HORNS and RF TRUMPET are built that way with RF-7's own numbers, tuned by measurement: brass reads 1319 Hz over the attack and 1333 over the body against BRASS 1's 1384 and 1570, trumpet 580 and 593 against BRASS 3's 141 and 554, and the ensemble's detuned carriers swing 8.8 dB over a held note against BRASS 1's 8.2. |
| The pianos' decay and brightness | Measured against a recorded grand | The three acoustic pianos are measured against the Salamander Grand Piano (a Yamaha C5, sixteen velocity layers) at A0, C2, C4 and C6, soft and loud, on the hardware's envelope clock. What the recording shows: a fast first fall that deepens with pitch — six decibels in the first second at A0, fourteen at C2, twenty-two at C4, thirty-five at C6 — then a tail of two to three decibels a second at every key; and a spectral centroid that roughly doubles from soft to loud (A0 294 → 633 Hz, C4 516 → 943, C6 831 → 1398). One operator's envelope cannot deepen its first fall with pitch, so RF PIANO and RF GRAND use two carriers: a *prompt* carrier that falls all the way, with rate scaling 5 so it falls faster up the keyboard, and an *aftersound* carrier with a slow tail, rate scaling 1 and level scaling that lowers it towards the top. RF PIANO's level profile now sits within two decibels of the recording's at every second of every register measured; its centroid reads 312 → 322 Hz at A0, 501 → 780 at C4 and 1106 → 1359 at C6. The bass swing with velocity is the one thing the index cannot give without folding: a modulator loud enough to double the bass centroid pushes the index into the region where more level makes the tone duller. |
| The reed | Measured against the cartridges | The instrument's saxophone is algorithm 18: one carrier under three half-ratio modulators, one of them the feedback loop — a reed's odd harmonics — and a partial near 5.8:1 at the top of the stack for the edge. Every level holds. RF SAX is built that way with its own numbers: 1337 Hz over the attack and 1369 over the body against SAX BC's 1303 and 1401, and a body that does not move once it has arrived, as the instrument's does not. |
| Which operators the envelope bias lifts | Documented, from the cartridges | The instrument's breath-controlled voices set the amplitude modulation sensitivity on the carrier and on the modulators the breath should brighten, and leave it off elsewhere. RF-7's sustained voices do the same — the carriers at 3 and the main modulator at 2 on the brass, the reed like the instrument's — so a breath controller or a pedal on the envelope bias has something to move out of the box, and the struck voices, which have no sensitivity, do not answer. |
| A sustained voice holds its body | Measured against the cartridges | The carriers of the eleven sustained voices stay within five points of their attack peak, which is where the instrument's own brass, pipes, oboe and bassoon sit. The modulators keep falling: on the instrument the timbre eases back after the attack while the loudness does not, and holding the modulators too doubles the body's brightness. |
| Voice mode | Documented, but not from the voice | Poly or mono with last-note priority, a function parameter on the instrument. A legato note does not restart its envelopes, which is what the hardware does; the operator levels keep the scaling of the key that began the phrase, because the hardware scales when a note starts rather than while it is held. |
| Portamento time | **Documented in part** | The rate is the firmware's: `PORTA_COMPUTE_RATE_VALUE` reads `TABLE_PITCH_EG_RATE[99 - time]`, and `PORTA_PROCESS` adds `((distance >> 10) + 1) × rate` to a pitch that counts 1024 units to the octave. So a glide is a straight line in the logarithmic pitch domain, one step faster for every whole octave still to cross, and RF-7 draws it the same way. What is inferred is the clock: the routine runs from the periodic timer and takes half the sixteen voices each time, which puts a voice's update at about 187 Hz on the instrument's 3140-count tick. That number sets the absolute speed of every glide and a recording would settle it. |
| **Velocity depth** | **RF-7 addition** | Scales the velocity offset the patch already asks for. At 0 the instrument stops answering to velocity entirely. |
| **Operator switches** | Documented as a panel action, not as a control | The DX7 can silence an operator from its front panel; it is not a voice parameter and not continuous. Muting one here changes nothing about how long a note lives. |

The three additions are marked because a reader should be able to tell at a
glance which controls would exist on the hardware and which are RF-7 making a
whole cartridge playable without opening every voice. Editing one voice is
[its own contract](EDITING.md).

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
