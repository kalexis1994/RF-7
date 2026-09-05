# RF-7 working conventions

- Keep project code, documentation and CLI messages in English. Speak to the user in their preferred language.
- After producing a version for the user to test, run `cargo run --locked --release -p rf7-lab -- audition` from this workspace. This is the standard build, package, install and Desktop launch workflow.
- The audition command owns only `dist/audition/RackForgeData`; keep the user's regular RackForge library separate. Preserve test-library audio/MIDI settings between runs.
- If RackForge is already open, report that its window must be closed before rerunning. Do not force-terminate it or replace a package behind a running host.
- Use `audition --prepare-only` for noninteractive preparation. Never launch Desktop in CI. Do not label a prepare-only run as a successful GUI/audio test.
- No Yamaha voice data, ROM cartridge dumps or third-party synthesizer source code enters this repository. The instrument reads cartridges the user already owns; every voice shipped here is written for RF-7.
- Say which behaviour is measured against a documented DX7 fact and which is a stated approximation. `docs/MODEL.md` is the ledger; keep it truthful.
