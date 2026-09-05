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
| Cartridge bulk dumps | any multiple of 4104 | 32 packed voices each |
| Single voice dump | 163 | one unpacked voice |
| Chip image | any multiple of 4096 | 32 packed voices per bank, no framing |

The first two are ordinary DX7 System Exclusive files, usually named `.syx`.
The sub-status channel a dump was made on is ignored: RF-7 reads files, not a
MIDI cable, and the channel says nothing about the voices in it. Collections
are commonly distributed as several dumps in one file, so several is what RF-7
reads; one damaged dump refuses the whole file rather than loading half of it.

The third is a cartridge ROM read straight off its chip: the same packed voices
with no System Exclusive header, no checksum and no terminator. There is no
room for a checksum in a chip image, so a bank is accepted on its shape alone —
the right length, and every byte seven-bit — and the corrections count below is
then the only quality signal there is.

A single voice is a library of one, not one voice repeated thirty-two times.

RF-7 offers up to **128 programs**. A file holding more is read and capped, and
the laboratory reports how many the file actually contained.

## Which bank comes first

A cartridge holds bank B in the lower half of its address space, so a chip
image *opens* with the voices the front panel numbers B1..B32. Read in file
order, program 1 of `voicerom1.bin` is PIANO 4, not BRASS 1.

RF-7 does not guess at this, because not every file of that length is a
cartridge ROM. The laboratory offers the choice:

```text
cargo run --release -p rf7-lab -- cartridge cartridges/voicerom1.bin
cargo run --release -p rf7-lab -- cartridge cartridges/voicerom1.bin --bank-order swapped
```

`swapped` reverses the 4096-byte banks, which gives the numbering printed on
the cartridge: program 11 is then E.PIANO 1. Inside RackForge there is no such
switch, so install the bank you want as its own 4096-byte file if the archive
provides one.

## In the laboratory

```text
cargo run --release -p rf7-lab -- cartridge cartridges/mine.syx
cargo run --release -p rf7-lab -- render --output renders/slot-7.wav --cartridge cartridges/mine.syx --program 7
```

`cartridge` lists every voice with its algorithm and feedback, marks the bank
boundaries, and says how many bytes were out of range.

## In RackForge

The package declares one optional file resource, `cartridge`, whose private
location is `<data-root>/plugins/org.rackforge.rf7/cartridges/current.syx`.
Install a `.syx` through RackForge's own resource installation; RF-7 never
opens that path itself and receives only the bytes.

Once a cartridge is delivered, RF-7 publishes its voices as the plugin's
programs, `program-001` upwards, named as the cartridge names them and grouped
into banks of thirty-two. Until then the eight factory voices stand in — eight
programs, not eight padded out to thirty-two, because a slot holding INIT VOICE
is a sine wave with a name.

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
