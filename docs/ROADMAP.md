# Roadmap

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

1. **Measure the modulation index.** One number sets how bright the whole
   instrument is, and it is currently a guess. Everything else in an FM patch is
   a ratio hanging off it, so this is worth more than the rest of this list put
   together. A recording of one operator modulating another at known output
   levels, compared against RF-7 under the same patch, settles it.
2. **The velocity table.** RF-7's curve is plausible and not measured. The
   instrument's is a table.
3. **The envelope rate scale.** The quantisation is documented; the seconds each
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
- Portable individual-program editing, so the six operators can be edited inside
  RackForge rather than only selected.
- A branded schema 3 package, once there is artwork.
- `parallel_render_v1`: sixteen voices split cleanly across audio workers, and
  this instrument is exactly the shape that contract was written for.
- A `.syx` export path, so a voice edited in RackForge can go back to hardware.
