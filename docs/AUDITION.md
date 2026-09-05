# Desktop audition

```text
cargo run --locked --release -p rf7-lab -- audition
```

One command builds the WASM, builds RackForge's packaging tools in the sibling
checkout, validates and packs the archive, installs it, selects RF-7 in a
session, and launches Desktop on a library that belongs to this project alone.

## What it owns, and what it leaves alone

Everything mutable is inside `dist/audition/RackForgeData`, marked with a
`.rf7-owned` file. An unmarked directory there is refused rather than modified,
so pointing this at a real RackForge library does nothing at all.

Your regular library is never touched. On the first run only, audio and MIDI
preferences are copied from `%LOCALAPPDATA%/RackForge/config/audio.toml` so the
test library opens on a working device. After that the test library keeps its
own settings, and the program you last selected.

## Rules it will not break

- If RackForge is already open, the run stops and says so. No process is ever
  terminated, and no package is replaced behind a running host.
- Same-version replacement is allowed, but only inside the marked library.
- A library already holding a newer RF-7 is refused rather than downgraded.
- After installing, the archive's WASM is compared byte for byte against the one
  just built, and the installed metadata version against this workspace's.

## Elsewhere, and in CI

```text
cargo run --locked --release -p rf7-lab -- audition --prepare-only
```

Prepares everything and launches nothing. This is the only form for CI, and a
prepare-only run is never a successful audio test — it has not made a sound.

## Receipts

Each run writes `dist/audition/<version>-<stamp>-<pid>/`, holding the archive,
`audition.json` with every path and the process id, the session file as it was
before the run, and `rackforge.log` from the launched host. When a run fails,
that log is the first place to look.

Set `RF7_DESKTOP` to point at a particular RackForge executable; otherwise the
newest of the sibling checkout's release builds is used.
