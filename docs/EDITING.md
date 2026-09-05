# Editing a voice

Every parameter of a voice can be edited inside RackForge, from any surface
the host draws: the Desktop window, the web shell, and the two-line display of
a controller. There is no command to run and no file to write by hand. RF-7
speaks the host's portable program-editing contract, and the host draws the
editor.

## Where it starts

RF-7's own window opens the editor with *Edit* on the program that is
playing, or *New program*. A controller's display opens it from the plugin's
programs page, where every catalog entry is offered. The plugin answers
three requests:

- With no program named, the editor opens on the initial voice — one carrier
  at full level, everything else silent — under the name `RF NEW`.
- On a program you saved earlier, the editor opens on **that program**. Saving
  replaces it in place, and it keeps its position in the catalog.
- On one of the library's voices — a factory voice or one from an installed
  cartridge — the editor opens on a **copy**, saved under its own identifier
  when you save it and not before. The cartridge is never changed.

## What the editor shows

Four pages at the top, shaped so that no list is longer than nine entries,
because the smallest surface is two lines high:

| Page | Fields |
| --- | --- |
| Voice | Algorithm (1–32, each with its routing as a hint), feedback, transpose, oscillator sync, pitch modulation sensitivity |
| LFO | Speed, delay, pitch depth, amplitude depth, waveform, key sync |
| Pitch envelope | Four rates, four levels |
| Operators | One page per operator, each with four groups |

Each operator's four groups: **Frequency** (fixed mode, coarse, fine, detune),
**Level** (output level, velocity sensitivity, amplitude modulation
sensitivity), **Envelope** (four rates, four levels) and **Keyboard scaling**
(break point, the two depths and curves, rate scaling). The operator's page says
whether the algorithm makes it a carrier or a modulator, so the output level
field reads correctly: on a modulator it is the modulation depth.

Every field previews live. Change a value and the next note plays it; the host
restores whatever was selected before when the editor closes.

Values are shown as the panel shows them: transpose from −24 to +24 rather than
0 to 48, detune from −7 to +7, algorithms numbered from 1. A value outside a
field's range is refused by the plugin rather than clamped, so the editor and
the engine cannot disagree about what was set.

## What is saved

The host owns the files. Each saved program is a JSON document under the
host's data folder, and the plugin writes the voice into it in words —
`"algorithm": 5`, `"left_curve": "neg-lin"`, `"waveform": "triangle"` — so a
file found on disk can be read without RF-7. A document edited by hand is
clamped on the way back in; an unknown curve or waveform name is refused, not
guessed at.

Beside every document the plugin leaves the same voice as a **single-voice
System Exclusive dump**, `programs/<id>.syx`, 163 bytes, with the checksum. It
is the format a DX7 accepts over MIDI, so a program made here can go to
hardware or to any other instrument that reads the format. The name in the
dump is the first ten characters of the name you gave the program.

Saved programs appear in the catalog after the library, in a bank of their
own, marked editable so the host offers to reopen them. Installing a different
cartridge later replaces the library and leaves them where they are. A session
that had one selected reopens on it, provided the host has installed it; if
the program was removed meanwhile, the library slot the session also remembers
plays instead.

## Limits

- Up to 128 saved programs, as many as the library itself may hold.
- Names are the host's; the voice carries ten bytes of it.
- A program's identifier is assigned by the plugin — `user.rf7-001` onwards,
  the first free number — and is not shown as something to edit.
- The host's generic editor has no free-text field, so the voice name is set
  where the host names sounds, not inside the editor pages.
