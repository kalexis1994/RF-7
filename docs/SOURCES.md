# Sources

RF-7 is written from published descriptions of how the instrument behaves. No
Yamaha voice data and no third-party synthesizer source code is copied into this
repository; where an existing implementation was consulted, it was to check a
fact about the hardware, and the fact was then re-expressed in RF-7's own terms.

## Structure and algorithms

- The Faust `dx7.lib` algorithm definitions, which state all 32 routings
  explicitly rather than as diagrams. Used to check the carrier sets, the
  modulation edges and which operator carries the feedback loop — including the
  two algorithms, 4 and 6, whose loop spans more than one operator.
- Independent descriptions of algorithms 1, 5 and 32, used as a cross-check that
  the table was not transcribed against the wrong numbering. Reverb Machine,
  *Exploring the Yamaha DX7*: <https://reverbmachine.com/blog/exploring-the-yamaha-dx7/>
- Ken Shirriff's die-level reverse engineering of the DX7's sound chips, for how
  the algorithm ROM and the operator pipeline actually work:
  <https://www.righto.com/2021/12/yamaha-dx7-chip-reverse-engineering.html> and
  <https://www.righto.com/2021/11/reverse-engineering-yamaha-dx7.html>

## Voice data and System Exclusive

- The DX7 System Exclusive specification: the 155-parameter unpacked voice, the
  128-byte packed voice, the 4104-byte bulk dump, the 163-byte voice dump and
  the seven-bit checksum. Collected at
  <https://github.com/probonopd/dx-specs>.

## Programming and behaviour

- Yamaha's own programming material, for what each parameter does and how
  operators are heard rather than only wired. Yamaha black boxes archive:
  <https://yamahablackboxes.com/articles/how-to-program-yamaha-dx7/>
- Yamaha Synth, *FM 101*: <https://yamahasynth.com/learn/synth-programming/fm101-part-three-the-magic-of-modulation/>

## What is not sourced

Everything marked **approximate** in [the model ledger](MODEL.md) has no source
behind it yet — it is RF-7's own guess at a curve whose shape is known and whose
numbers are not. Those are the entries a measurement would replace, and the
ledger is the list of what to measure.
