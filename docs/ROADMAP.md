# Roadmap

## 0.4.0 — done

- Portamento and a mono voice mode, both from the instrument's own
  behaviour. Mono has last-note priority and holds one voice, so a line
  never stacks the release tails of the notes it has left behind; a legato
  note takes that voice without starting its envelopes again, which is what
  the hardware does. The glide reads the firmware's own rate table and its
  `((distance >> 10) + 1) × rate` step, so it is a straight line in the
  logarithmic pitch domain that gains a step of speed for every octave still
  to cross. Portamento works in poly as well, gliding from the note played
  before, and controller 65 switches it. The one inferred number is the
  update rate the hardware glides at; [the ledger](MODEL.md) says so.
- The sustained voices hold their note. Every one of them had been written
  to attack to 99 and then settle a long way under it, which is heard as a
  crescendo that breaks and drops away — the brass fell 8 dB from its peak to
  its body, the horns 15, the saxophone 12. The instrument's own cartridges
  do not do that: BRASS 1 falls 3, 1, 1, 1 and 7 points from its peak to the
  level it holds, PIPES 1 falls 1, 9, 9, 2, 6, 0, OBOE 10, 0, 19, 4, 0, 0.
  The carriers of the eight sustained voices now hold within five points of
  their peak; the modulators keep the fall they had, because that fall is the
  timbre easing back and it is what put the body's brightness beside the
  instrument's. Brass now falls 3.6 dB, horns 5.0, saxophone 4.3, and the
  body's spectral centroid is 1759 Hz against BRASS 1's 1570 — where holding
  the modulators too had put it at 3584. A test renders every voice and
  asserts which ones hold and which ones decay.
- The instrument's two other controllers. A breath controller and a foot
  controller each have a reach and a destination of their own, and every
  controller now reaches the third destination the DX7 offered: the envelope
  bias, which holds the sensitive operators under their level until the
  player brings them up — what a breath controller is for, and what a swell
  pedal does on the brass. Channel volume and expression scale the output, so
  a keyboard's volume slider and pedal do what they do everywhere else. A
  program change past the library's last slot reaches the saved programs in
  catalog order, so a controller can call up a program written in the editor.
- COMPARE on the head rail: the program as it opened plays for as long as
  the key is down, its values on the knobs, and the edit is untouched
  underneath. It uses the host's own transient previews and its restore, so
  nothing is written to the draft to hear the difference. The display shows
  the number an open program came from instead of ED, and a saved program
  as U and its number. SETUP names the installed cartridge — the host says
  only that one is installed, so the surface remembers which grant it put
  there — and lights that pad.
- The envelope clock is the hardware's. RF-7's envelopes had run on a guessed
  scale: measured against the MSFA project's hardware measurements, decays
  were five times too fast and attacks three times too slow, so a piano died
  in a second and a string section smeared. The engine now uses the measured
  curve — one level step every 4096 samples at rate 0, four quantised steps
  to double, the attack's `2 + (full − level) / 256` factor and its 40 dB
  jump — and its seconds per rate match the hardware's table to a few per
  cent. Every voice in the bank was measured again against its reference
  afterwards.
- The pitch envelope on the firmware's own tables. `TABLE_PITCH_EG_LEVEL`
  carried verbatim: a level is a byte whose top seven bits join a pitch word
  of 4096 to the octave, so a step is 3/8 of a semitone, the middle of the
  table is one step a level and the ends steepen to four octaves. The rates
  read `TABLE_PITCH_EG_RATE` — the portamento's table — added to that word
  every second timer tick, a straight line in log pitch at a speed that does
  not depend on the distance: 140 semitones a second at 99, half of one at
  0. The quadratic curve that stood in for the levels was a third of the
  real deviation through the middle, and the rates had borrowed the
  operators' curve. The factory voices' scoops were re-read onto the table
  so each keeps the deviation it was written for.
- Keyboard level scaling exactly as the firmware builds it. The routine
  that constructs each operator's forty-three-entry curve was read: the
  break point plus twenty and the sounding key (note plus the voice's
  transpose) go through the key-to-pitch table and lose two bits, so the
  break point's own group of three keys is neutral — three keys higher
  than RF-7 had it — and the distance indexes the firmware's linear and
  exponential curve tables, now carried verbatim (the reconstructed
  exponential table had diverged above the fifteenth group), multiplied by
  the depth scaled to 255 at 99 and clamped at 127. Tests pin the groups,
  the two curves, the depth scale and the clamp to the firmware's numbers.
- The pianos measured against a real one. With the envelope clock right,
  RF PIANO, RF GRAND and RF UPRIGHT were read beside the Salamander grand
  at four registers and two dynamics, and rebuilt around what the recording
  shows: a first fall that deepens with pitch, from six decibels at A0 to
  thirty-five at C6, over a tail of two to three decibels a second
  everywhere. That needs two carriers — a prompt sound that falls faster up
  the keyboard and an aftersound with a slow tail that is lowered towards
  the top — and RF PIANO's level profile now sits within two decibels of the
  recording's at every second measured, with the brightness of a soft and a
  loud note beside the recording's in the mids and the treble.
- Strings and pianos rebuilt on the cartridges' own architectures, and a
  second bank of six. RF STRINGS is the ensemble — a 1:1 pair with the
  feedback loop, a stack ending in 3:1 and 14:1 for the rosin, the second
  carrier darkened towards the top; RF CELLOS the same an octave down with
  doubled ratios and an 8:1 edge; RF VIOLINS quicker, with a deeper vibrato
  that waits and the bow's scoop; RF ORCHSTR the whole section with a long
  swell. RF PIANO is the instrument's own piano: three 1:1 carriers under
  held modulators, a 1.58:1 knock that dies in a tenth of a second, a
  two-stage decay, rate scaling that lets the treble die while the bass
  rings; RF GRAND the bright grand with a 7:1 hammer and a half-ratio thump;
  RF UPRIGHT the darker one an octave down; RF EP BELL a brighter electric
  piano with a longer bell. Each measured within a few per cent of its
  reference's brightness and decay profile.
- PERFORM laid out as the instrument's function layer: KEYBOARD for the
  bend range, tuning, transpose, voice mode and portamento, and CONTROLLERS
  as one row per controller with its reach and its destination side by side,
  instead of thirteen controls in one plate.
- The saxophone rebuilt on the instrument's own reed — algorithm 18, three
  half-ratio modulators under one carrier, one of them the feedback loop,
  a partial near 5.8:1 for the edge — after it measured as a bright spit
  over a dull body (1029 Hz over 320 against SAX BC's 1303 over 1401). It
  now reads 1337 over 1369 and holds. The sustained voices carry the
  envelope-bias sensitivity the instrument's breath-controlled voices
  carry, so the new breath and foot controllers swell them out of the box;
  a test blows each of them.
- The leads hold too. LEAD, SQUARE and SUB had the same fall as the brass —
  12, 8 and 14 dB from the peak to the body — and the instrument's own
  SYN-LEAD 2, 3 and 4 hold their note within a decibel. Their carriers now
  stay within five points of their peak, the modulators keep their fall, and
  the lead's four carriers sit a step apart instead of six, so the chorus
  swings 6 dB over a held note instead of 8. The hold test covers them.
- The brass family rebuilt on the instrument's own architecture. Holding
  the old voices' bodies had not made them brass: every operator sat at 1:1
  attacking together and falling together, which measured as a bright spit
  over a dull body — the trumpet read 1238 Hz over the attack and 329 over
  the body, where the instrument's solo brass reads 141 over 554. The
  cartridges and Yamaha's programming notes agree on the recipe: ensemble
  brass is algorithm 22 with a 1:1 feedback modulator blooming in behind
  three detuned 1:1 carriers and a sub-octave pair; solo brass is one carrier
  over a feedback core, an attack that spits and dies, an inharmonic bite
  under velocity and a fixed flutter, with a pitch scoop and a delayed
  vibrato. RF BRASS, RF HORNS and RF TRUMPET are written that way with their
  own numbers, tuned until their attack and body brightness sat beside BRASS
  1, 7 and 3, and the ensemble's chorus swing was measured against the
  instrument's before its carriers were staggered to match it.
- The knobs answer the way a player expects: Shift for a tenth of the
  travel, rebased mid-drag so the key can be pressed at any moment; the wheel
  for a notch, a step with Shift; a double-click to send a parameter to its
  default or a voice field to where the program opened. Each operator card
  carries its own ON key — the same switch the PERFORM page and a controller
  see — so muting one no longer means leaving the editor. EDIT stays on the
  rail with its lamp lit while a program is open, instead of vanishing.
- A SETUP surface, so a cartridge can be installed from inside RackForge.
  Until now the plugin declared the resource but offered no way to fill it:
  the only way in was to put the file in the host's data folder by hand. The
  host owns the explorer, the permission and the copy; the surface names the
  resource and reads back what is installed.
- The laboratory builds the WASM component itself before packaging, so a
  package can no longer be validated against the component of an earlier
  edit — which had just happened once, silently.

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

- A ZIP import container, so a folder of `.syx` files can be installed in one
  step instead of one cartridge at a time.
- `parallel_render_v1`: sixteen voices split cleanly across audio workers, and
  this instrument is exactly the shape that contract was written for.
- A whole-library `.syx` export — the thirty-two-voice bulk dump — for the
  saved programs together; each one already leaves its own single-voice dump.
- A voice name field in the editor, when the host's generic editor grows a
  text kind; today the name is the program's name in the host.
