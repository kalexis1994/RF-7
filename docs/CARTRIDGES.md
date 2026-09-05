# Cartridges

RF-7 ships no voice data. The eight factory voices in
[`crates/rf7-voice/src/factory.rs`](../crates/rf7-voice/src/factory.rs) were
written for RF-7 and are starting points, not recreations of anything. The
voices a DX7 shipped with are Yamaha's; they are not in this repository and will
not be.

What RF-7 does instead is read a cartridge you already have.

## What it accepts

| File | Bytes | Contents |
| --- | ---: | --- |
| Cartridge bulk dump | 4104 | 32 packed voices |
| Single voice dump | 163 | one unpacked voice |

Both are ordinary DX7 System Exclusive files, usually named `.syx`. The
sub-status channel a dump was made on is ignored: RF-7 reads files, not a MIDI
cable, and the channel says nothing about the voices in it.

A single voice fills all thirty-two slots, so a patch exported on its own is
playable without building a cartridge around it.

## In the laboratory

```text
cargo run --release -p rf7-lab -- cartridge cartridges/mine.syx
cargo run --release -p rf7-lab -- render --output renders/slot-7.wav --cartridge cartridges/mine.syx --program 7
```

`cartridge` lists all thirty-two names with their algorithm and feedback, and
says how many bytes were out of range.

## In RackForge

The package declares one optional file resource, `cartridge`, whose private
location is `<data-root>/plugins/org.rackforge.rf7/cartridges/current.syx`.
Install a `.syx` through RackForge's own resource installation; RF-7 never
opens that path itself and receives only the bytes.

Once a cartridge is delivered, RF-7 publishes its thirty-two voices as the
plugin's thirty-two programs, `program-01` to `program-32`, named as the
cartridge names them. Until then the eight factory voices stand in.

## Bytes that are out of range

Cartridges in circulation routinely carry values the DX7 itself ignored — a
detune of 15, a waveform of 7, unwritten memory that is all `0x7f`. Refusing
those would lose the other thirty-one voices with them, so RF-7 clamps each one
into its documented range and counts it. The count travels with the cartridge
and is reported rather than hidden.

A damaged *frame*, however, is refused outright: a wrong manufacturer or format
word, a payload byte with bit 7 set, or a checksum that does not match. Those
mean the file is not what it claims to be, and guessing at it would be worse
than saying so.
