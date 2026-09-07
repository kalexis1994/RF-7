# Cartridges

RF-7 ships no voice data. The thirty-eight factory voices in
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
| ZIP | up to eight megabytes | any number of the above |

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

## Archives

Cartridges are published as ZIPs, and what is in one is rarely only the
cartridge: the same bank appears as a `.syx`, as a `.mid` wrapping the same
dump, and in the formats two old editors used, all four in a folder, sometimes
with a note beside them. A reader that went by content would find every bank
three times over, so RF-7 goes by name.

It takes the `.syx` files if the archive holds any, the `.bin` and `.dx7` chip
images if it does not, and anything at all if it holds neither — so a cartridge
saved under a name nobody agreed on still installs. Whatever it takes it reads
in the order the names sort in, which is what puts `ROM1A` in front of `ROM1B`,
and it skips a second copy of a file it has already taken. An entry that turns
out not to be a cartridge is passed over rather than failing the archive.

The reading is RF-7's own, in
[`crates/rf7-voice/src/zip.rs`](../crates/rf7-voice/src/zip.rs) and
[`inflate.rs`](../crates/rf7-voice/src/inflate.rs) — this crate has no
dependencies, and a decompressor that runs on a user's files is not a place to
add one. Every entry's checksum is verified before its bytes are used, and
each is decompressed into a buffer of RF-7's own — half a megabyte, which is
a hundred and twenty eight banks in one file, and past which a file is passed
over rather than read in half. Four megabytes decompressed is all one archive
gets, however large it is or claims to be, so what an installation costs does
not depend on what it was handed. Encryption, the extensions for very large
files, and compression methods other than *stored* and *deflate* are refused
rather than guessed at.

RF-7 holds **eight cartridges at once**, in eight bays, and offers up to **320
programs** — the factory bank, then every bay in order, then the programs you
saved. A file holding more than fits is read and capped, and the laboratory
reports how many the file actually contained.

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

RF-7's **SETUP** surface is the rack: eight bays, each drawn as the cartridge
in it or as an empty one. Open the plugin from RackForge's Plugins section and
press a bay. *Install…* opens the host's own file explorer, and what you
choose — a dump, a chip image, or the ZIP a collection came in — is copied
into that bay; the host prepares a replacement instance away from the audio
callback and swaps it at a block boundary. *Take it out* empties the bay.
Files the host has been pointed at before are listed inside a bay under
*Already chosen*, so the same cartridge goes into another bay without the
explorer.

What is in a bay is not remembered by the surface: the plugin publishes one
**bank per bay**, so the programs themselves say which cartridge they came
from, and the surface draws the label from them. The one thing it does keep is
the file's name, because the host never tells a plugin what a file was
called.

**The factory bank never leaves.** It is the head of the library — programs 1
upwards, in a bank of its own at the top of the list — and a cartridge is
added to the instrument rather than put in front of it, which is what the
internal memory of the instrument this one is shaped after did. Each bay's
voices are one stretch behind it, so filling, replacing or emptying a bay
leaves the factory's program numbers, and every other bay's, exactly where
they were.

Going the other way — RF-7's programs, or its factory bank, as a cartridge —
happens with every save: the host writes `exports/rf7-programs-N.syx` and,
while the factory bank plays, `exports/rf7-factory-1.syx` and `-2.syx` under
`plugins/org.rackforge.rf7/` in its data folder. SETUP says so. They are
ordinary thirty-two-voice bulk dumps, so they load into a DX7, into RF-7
itself, or into anything that reads the format.

Setting the library up is not something to reach for while playing, which is
why it is a surface of its own: RackForge keeps program selection and voice
editing in PLAY, and libraries, resources and diagnostics in CONFIG.

Underneath, the package declares eight optional file resources — `cartridge`,
then `cartridge-2` through `cartridge-8` — whose private locations are
`<data-root>/plugins/org.rackforge.rf7/cartridges/`. The first keeps the name
RF-7 has always declared, so a cartridge installed before there were bays is
still in bay one. The explorer,
the permission and the copy are all the host's: RF-7 is handed the bytes and
never sees a path, and the surface asks for the file by the resource's name
rather than by any location of its own.

RF-7 publishes the factory voices as `program-001` upwards, in the bank named
*Factory bank*, and a bay's cartridge behind them, named as the cartridge
names them, in a bank per bay.

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
