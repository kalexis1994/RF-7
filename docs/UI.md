# The PLAY surface

RF-7 draws its own front panel inside RackForge, wherever the host shows a
plugin — Desktop, the web shell on a Raspberry Pi, a phone. It is a
silkscreened chassis with knobs, membrane keys, lit glass and section groups,
in the same idiom as RackForge's other instruments, because a player should
recognise an instrument before reading a word of it. The host owns the frame
around it, the session, the audio and the files; the panel owns everything
inside the frame.

## What it is made of

The surface is a Rust program compiled to WebAssembly, in `crates/rf7-ui`,
with `package/web/play.html` and `style.css` written by hand. `wasm-bindgen`
generates the browser glue (`app.js`, `app_bg.wasm`) when the laboratory
packages the plugin; the glue is not committed. There is no JavaScript
framework and no JavaScript logic: what to draw, what to ask the host and
what a reply means are Rust, and the native test build exercises all of it.

| Module | Owns |
| --- | --- |
| `model` | The host's context read into typed state: the catalog, the draft with its fields by id, the parameters with their schema. Forgiving on the way in, strict on the way out. |
| `render` | State to HTML: the whole panel, page by page. Every control carries a `data-field`, `data-param`, `data-sound`, `data-tab` or `data-action` and nothing else; the browser layer routes on those alone. |
| `diagram` | The algorithm as SVG, laid out from the routing table: carriers along the output rail, each modulator above what it modulates, the feedback path as a loop. |
| `client` | One request in flight at a time; a dragged slider sends its latest value, not every value; a reply that takes more than five seconds drops the queue and waits for the next context. |
| `browser` | The only module that touches the DOM, compiled only for wasm. Three event listeners on the root, four pointer listeners for the knobs, one message listener, one timer. |

## How it talks to RackForge

The host's Web Plugin API, version 1, over `postMessage`. The surface sends
`ready`; the host answers with a `context` — the instance's sounds, the
draft under edit if one is open, the lighting it is rendering in — and sends
a fresh one whenever anything changes. The surface asks for the rest:

- `plugin.parameters` and `plugin.set_parameter` for the seventeen
  performance controls, drawn from the schema the host returns so a
  parameter added to the plugin appears without a change here;
- `plugin.select_sound` to play a program;
- `plugin.begin_program_edit`, `plugin.edit_program_field`,
  `plugin.set_program_name`, `plugin.save_program` and
  `plugin.cancel_program` for the editor;
- `plugin.set_surface_info` so RackForge's own performance bar names the
  draft while one is open.

Every edit goes to the host and comes back in the next context. The surface
shows the value it sent until the host has both answered and echoed it, so a
slider does not jump while its request is in flight and cannot disagree with
the engine afterwards.

## The panel

A head rail across the top, four section keys under it, and the working
surface below.

**The head rail** carries the RF-7 logotype, the lit display and the command
keys. The display shows the program number, its name, what the panel is doing
to it — PLAYING, EDITING, EDITING · UNSAVED — and one line of machine state.
The number is the program's own — the library slot, or U and its number for
a saved program — and an open program keeps the number it came from. The
keys are EDIT and NEW while a program plays; SAVE (its lamp lit while there
are unsaved changes), COMPARE and EXIT, with the name field, while one is
open. COMPARE plays the program as it opened for as long as it is down,
shows those values on the knobs, and takes no edits until it is released;
it does nothing until something has moved.

**VOICE** — the algorithm on lit glass, drawn from the routing table, with a
stepper, a selector for all thirty-two and the carrier list printed beneath;
every envelope on lit glass carries a point at the end of each segment,
which drags up and down for the level and sideways for the rate, the trace
following as it goes;
feedback, transpose, pitch-modulation sensitivity and oscillator key sync;
the LFO with its six waveform keys; and the pitch envelope drawn as a trace
around its centre line, because a pitch envelope is a deviation, not an
amount.

**OPERATORS** — six columns, one per operator, each headed by its number, the
role the algorithm gives it, its frequency as the panel would write it
(`×2.00`, `440 Hz`) and its output level. Under that its envelope on lit
glass, then the four rates over the four levels, the output group, the
frequency group with its FIXED key, and keyboard scaling with the two curve
key rows. A carrier is amber, from its top edge to the trace on its screen to
the cap of its knobs; a modulator is blue. Each header also carries COPY and
PASTE: COPY takes the operator as it shows, PASTE lays the copied one over
this operator — every field the program has, only where the value would
change — and its lamp is lit while there is something to paste. The copy
lives in the surface, so it can be pasted into another program, or after the
window has been closed and opened.

**PERFORM** — the twenty-six public parameters, grouped as the plugin's own
schema groups them, so a parameter added to the plugin appears here without a
change to the panel. The performance page is laid out as the instrument's
function layer: KEYBOARD for bend range, tuning, transpose, the voice mode
and portamento, and CONTROLLERS as one row per controller — mod wheel,
aftertouch, breath, foot — with its reach and its destination side by side.
These are the same controls the LITTLE surface and MIDI links see.

**PROGRAMS** — the library's banks and then YOUR PROGRAMS, as pads. One press
plays a program. While a program is open for editing the pads are dark: the
host holds the audition for the draft.

Knobs turn by dragging up and down — a hundred and eighty pixels is the whole
range, the throw the other RackForge instruments use. Shift makes the same
travel a tenth of the range, and switching Shift mid-drag continues from where
the knob is rather than jumping. The wheel turns a knob a notch, a step with
Shift. A double-click sends a knob back to where it started: a parameter to
the schema's default, a voice field to the value it had when the program was
opened. Focused, a knob answers the arrow, page and home keys. Each operator
card carries its own ON key, the same switch the PERFORM page and a controller
see, so an operator can be muted without leaving the editor. A choice is a row of membrane keys, except
the algorithm, which has thirty-two positions and so keeps a selector.

The panel has one appearance. It does not repaint itself for the room, any
more than a painted chassis would; the host's lighting hint is read and left
alone.

## On a narrow panel

The panel is the same instrument at every width. A phone gets the same four
sections, the same keys and the same knobs a Desktop window gets; what a
narrow frame changes is how many of them sit on a line and how large a knob
is printed, never which controls the panel has.

- The head rail keeps its keys. Below the width they need in a line the
  commands take the line under the display rather than running off the end
  of the chassis, because a player cannot press what is not there.
- The section keys keep their names and drop the gloss under them; if four
  names still do not fit, the row scrolls rather than losing the fourth.
- The knob is one dimension the panel prints at 62, 54 or 48 pixels, and the
  cap, its shadow and the pointer are struck as fractions of it.
- An operator's header keeps its number, its role, its frequency and its
  level on one line, and takes the line under them for ON, COPY and PASTE.
- Every measure is the width of the panel, of the group, or of the card the
  control is mounted on — never the window's. A group that is a third of a
  wide page is a narrow group and lays itself out as one.
- Where the host is driven by touch, the keys are struck at the size a
  finger can hit and the points on an envelope are drawn large enough to
  take hold of.

The floor is a panel 320 pixels wide. At that width every control is still
on the panel, still legible and still reachable; narrower than that the
working surface scrolls, and the rail and the section keys do not.

## The library

**PROGRAMS** is two columns. Down the left is every bank there is, one per
filled bay and named after its cartridge, then the programs you saved; the
list scrolls on its own, and the bank the program that is playing came out of
is marked. Down the right are that bank's programs, numbered from one as the
cartridge numbers them. Choosing a bank on the left changes what is on the
right and nothing else — the instrument keeps playing what it was playing.

Eight cartridges is more programs than one list can be read down, which is why
the bank is a column of its own rather than a heading in a long page.

## The setup surface

RF-7 declares a second surface, CONFIG, which RackForge opens from its
Plugins section. It holds what is done once rather than while playing:

- **Cartridge bays** — the rack: eight bays, each drawn as the cartridge in
  it, the way the DX7's own looked. A dark slab with a turquoise *VOICE ROM*
  band along the spine and a label on its face: the bay's number, a hairline,
  and rows A and B naming the first and last voice of each bank of thirty-two,
  with the file's name in the corner. A filled bay is lit; an empty one sits
  back and says so. If you have saved programs, the rack ends with them, on
  the silver *DATA RAM* the instrument would have written them to.

  Pressing a bay opens what it holds — its voices, numbered in the two columns
  its label would print them — with *Install…* or *Replace…*, *Take it out*,
  and the files RackForge has been granted before, each one press away from
  going into this bay without the explorer.
- **Instrument** — the plugin version, how many programs there are and how
  many you saved, whether the host is answering, and where the exports go.

The surface never sees a path, and it cannot invent one: it asks for the
resource by the name the manifest declares, and the host decides what that
means. While the host is working the commands are disabled and the panel says
what it is waiting for; a cancelled explorer is reported as a choice not
made, not as a failure. The requests that wait on a person — the explorer, an
installation — are exempt from the five-second timeout the other requests
keep, because a player thinking about which file to pick is not a host that
has stopped answering.

## What it refuses to do

- It never assumes a value. Before the first context it draws nothing
  editable; after a lost reply it disables what it cannot vouch for.
- It never edits a program the host did not open. Selecting a sound while a
  draft is open does nothing; the host would refuse it anyway.
- It never writes a name the host would reject: one to sixty-four printable
  ASCII characters, trimmed.
- It never loses a field. A control the layout has no place for — one a
  later plugin adds — is rendered under *More* from the host's typed tree.
