//! State to HTML: the RF-7 front panel.
//!
//! The panel is drawn the way RackForge's other instruments are drawn — a
//! silkscreened chassis with knobs, membrane switches, lit displays and
//! section groups — because a player should recognise an instrument before
//! reading a word of it. What is behind the paint is still the host's typed
//! state: every control carries `data-field` for a draft field, `data-param`
//! for a plugin parameter, `data-sound` for a program, `data-tab` for a page
//! or `data-action` for a command, and the browser layer routes on those
//! alone.

use crate::diagram;
use crate::model::{Draft, Field, FieldKind, Parameter, ParameterKind, State, frequency_text};
use std::fmt::Write;

pub fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

/// The pages of the panel, in the order the tabs sit.
pub const PAGES: [(&str, &str, &str); 4] = [
    ("voice", "VOICE", "Algorithm · LFO · pitch"),
    ("operators", "OPERATORS", "Six operators"),
    ("perform", "PERFORM", "Bend · wheels · level"),
    ("programs", "PROGRAMS", "Library and yours"),
];

/// The page a state is showing, always one that exists.
pub fn page_id(state: &State) -> &str {
    PAGES
        .iter()
        .map(|(id, _, _)| *id)
        .find(|id| *id == state.page)
        .unwrap_or("programs")
}

// ---- the head of the chassis -------------------------------------------

pub fn header(state: &State) -> String {
    let mut out = String::from(
        "<div class=\"identity\"><span class=\"mark\">RF&#8209;7</span><span class=\"sub\">SIX&#8209;OPERATOR FM</span></div>",
    );
    out.push_str(&display(state));
    if state.surface == "config" {
        // The library is set up here, not played, so the rail carries no
        // program keys.
        out.push_str("<div class=\"commands\"><span class=\"rail-note\">SETUP</span></div>");
        return out;
    }
    // Four keys, always in the same place, the way a panel has them: the
    // lamps say what the instrument is doing. EDIT lights while a program is
    // open; SAVE lights while there is something unsaved.
    let editing = state.draft.is_some();
    let dirty = state.draft.as_ref().is_some_and(|d| d.dirty);
    let editable = state.selected_sound().is_some_and(|s| s.editable);
    out.push_str("<div class=\"commands\">");
    if let Some(draft) = &state.draft {
        let _ = write!(
            out,
            "<input class=\"name-input\" type=\"text\" maxlength=\"64\" data-action=\"name\" value=\"{}\" aria-label=\"Program name\" spellcheck=\"false\">",
            esc(&draft.name)
        );
    }
    let key = |out: &mut String, action: &str, label: &str, lit: bool, enabled: bool| {
        let _ = write!(
            out,
            "<button type=\"button\" class=\"key wide{}\" data-action=\"{action}\" aria-pressed=\"{lit}\"{}><span class=\"led\"></span>{label}</button>",
            if lit { " lit" } else { "" },
            if enabled { "" } else { " disabled" }
        );
    };
    key(
        &mut out,
        "edit",
        "EDIT",
        editing,
        state.connected && (editing || editable),
    );
    key(&mut out, "new", "NEW", false, state.connected && !editing);
    key(&mut out, "save", "SAVE", dirty, editing);
    // COMPARE plays the program as it opened while it is down. There is
    // nothing to compare until something has moved.
    key(
        &mut out,
        "compare",
        "COMPARE",
        state.comparing,
        editing && (state.comparing || !state.moved_fields().is_empty()),
    );
    key(&mut out, "cancel", "EXIT", false, editing);
    out.push_str("</div>");
    out
}

/// The lit display: what plays, and what the panel is doing to it.
fn display(state: &State) -> String {
    let (number, name) = match (&state.draft, state.selected_sound()) {
        // An open program keeps the number it came from; a new one has none.
        (Some(draft), _) => (
            draft
                .original_program_id
                .as_deref()
                .map_or_else(|| "--".to_owned(), program_number),
            draft.name.clone(),
        ),
        (None, Some(sound)) => (program_number(&sound.id), sound.name.clone()),
        (None, None) => ("--".to_owned(), "NO PROGRAM".to_owned()),
    };
    let mode = match &state.draft {
        Some(_) if state.comparing => "COMPARING · AS OPENED",
        Some(draft) if draft.dirty => "EDITING · UNSAVED",
        Some(_) => "EDITING",
        None => "PLAYING",
    };
    let status = if state.connected {
        if state.status.is_empty() {
            "READY".to_owned()
        } else {
            state.status.to_uppercase()
        }
    } else if state.status.is_empty() {
        "LINKING TO RACKFORGE".to_owned()
    } else {
        state.status.to_uppercase()
    };
    format!(
        "<div class=\"lcd\" data-ready=\"{}\"><span class=\"mode\">{mode}</span><span class=\"code\">{}</span><span class=\"name\">{}</span><span class=\"line\">{}</span></div>",
        state.connected,
        esc(&number),
        esc(&name),
        esc(&status)
    )
}

/// `program-004` reads as 04 on the display; a saved program reads as U and
/// its own number, `custom.user.rf7-003` as U03.
fn program_number(id: &str) -> String {
    if let Some(slot) = id
        .strip_prefix("program-")
        .and_then(|digits| digits.parse::<usize>().ok())
    {
        return format!("{slot:02}");
    }
    if let Some(number) = id.strip_prefix("custom.").and_then(|rest| {
        let digits = rest.trim_end_matches(|c: char| !c.is_ascii_digit()).len();
        rest[..digits]
            .rsplit(|c: char| !c.is_ascii_digit())
            .next()?
            .parse::<usize>()
            .ok()
    }) {
        return format!("U{number:02}");
    }
    "US".to_owned()
}

pub fn tabs(state: &State) -> String {
    let current = page_id(state);
    let mut out = String::new();
    for (id, title, detail) in PAGES {
        let active = if id == current { " active" } else { "" };
        let _ = write!(
            out,
            "<button type=\"button\" class=\"tab{active}\" data-tab=\"{id}\" aria-selected=\"{}\"><strong>{title}</strong><span>{detail}</span></button>",
            id == current
        );
    }
    out
}

// ---- panel parts -------------------------------------------------------

fn group(id: &str, colour: &str, title: &str, body: &str) -> String {
    format!(
        "<section class=\"group group-{id}\"><span class=\"stripe {colour}\"></span><h2>{}</h2><div class=\"controls\">{body}</div></section>",
        esc(title)
    )
}

/// Eleven ticks over 270°, the same scale the other instruments print.
fn knob_ticks() -> String {
    let mut ticks = String::new();
    for index in 0..=10 {
        let angle = -135.0 + f64::from(index) * 27.0;
        let class = if matches!(index, 0 | 5 | 10) {
            " major"
        } else {
            ""
        };
        let _ = write!(
            ticks,
            "<line class=\"tick{class}\" x1=\"50\" y1=\"4\" x2=\"50\" y2=\"13\" transform=\"rotate({angle} 50 50)\"></line>"
        );
    }
    ticks
}

fn knob_angle(value: f64, minimum: f64, maximum: f64) -> f64 {
    if maximum <= minimum || !value.is_finite() {
        return -135.0;
    }
    let fraction = ((value - minimum) / (maximum - minimum)).clamp(0.0, 1.0);
    -135.0 + fraction * 270.0
}

/// One knob. `binding` is the whole attribute the browser routes on, so the
/// same body serves a draft field and a plugin parameter.
#[allow(clippy::too_many_arguments)]
fn knob_html(
    binding: &str,
    label: &str,
    detail: &str,
    value: f64,
    minimum: f64,
    maximum: f64,
    step: f64,
    display: &str,
    default: f64,
) -> String {
    let angle = knob_angle(value, minimum, maximum);
    format!(
        "<div class=\"ctl knob-ctl\" title=\"{}\"><span class=\"l\">{}</span><div class=\"knob\" data-knob style=\"--knob-turn:{angle:.2}deg\"><svg class=\"scale\" viewBox=\"0 0 100 100\" aria-hidden=\"true\">{}</svg><span class=\"shadow\"></span><span class=\"cap\"><span class=\"top\"></span><span class=\"marker\"></span></span><input class=\"knob-input\" type=\"range\" {binding} min=\"{minimum}\" max=\"{maximum}\" step=\"{step}\" value=\"{value}\" data-default=\"{default}\" aria-label=\"{}\"></div><output>{}</output></div>",
        esc(detail),
        esc(label),
        knob_ticks(),
        esc(label),
        esc(display)
    )
}

fn switch_html(binding: &str, label: &str, detail: &str, on: bool) -> String {
    format!(
        "<button type=\"button\" class=\"sw\" role=\"switch\" aria-checked=\"{on}\" {binding} data-toggle title=\"{}\"><span class=\"led\"></span><span class=\"l\">{}</span></button>",
        esc(detail),
        esc(label)
    )
}

fn segment_html(binding: &str, label: &str, options: &[(String, String)], current: &str) -> String {
    let mut out = format!(
        "<div class=\"ctl seg-ctl\"><span class=\"l\">{}</span><div class=\"seg\" role=\"group\" aria-label=\"{}\">",
        esc(label),
        esc(label)
    );
    for (value, text) in options {
        let _ = write!(
            out,
            "<button type=\"button\" {binding} data-value=\"{}\" aria-pressed=\"{}\">{}</button>",
            esc(value),
            value == current,
            esc(text)
        );
    }
    out.push_str("</div></div>");
    out
}

// ---- draft fields ------------------------------------------------------

fn field_knob(state: &State, draft: &Draft, id: &str, label: &str) -> String {
    let Some(field) = draft.field(id) else {
        return String::new();
    };
    let FieldKind::Number {
        minimum,
        maximum,
        unit,
    } = &field.kind
    else {
        return String::new();
    };
    let value = state
        .field_value(id)
        .and_then(|v| v.as_i64())
        .unwrap_or(*minimum);
    let display = format!("{value}{}", unit.as_deref().unwrap_or(""));
    // A double-click sends the knob back to where the draft opened.
    let opened = state
        .baseline
        .get(id)
        .and_then(|v| v.as_i64())
        .unwrap_or(value);
    knob_html(
        &format!("data-field=\"{}\"", esc(id)),
        label,
        &field.detail,
        value as f64,
        *minimum as f64,
        *maximum as f64,
        1.0,
        &display,
        opened as f64,
    )
}

fn field_switch(state: &State, draft: &Draft, id: &str, label: &str) -> String {
    let Some(field) = draft.field(id) else {
        return String::new();
    };
    let on = state
        .field_value(id)
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    switch_html(
        &format!("data-field=\"{}\"", esc(id)),
        label,
        &field.detail,
        on,
    )
}

/// A choice field as a row of membrane keys.
fn field_segment(state: &State, draft: &Draft, id: &str, label: &str) -> String {
    let Some(field) = draft.field(id) else {
        return String::new();
    };
    let FieldKind::Choice { options } = &field.kind else {
        return String::new();
    };
    let current = state.field_value(id);
    let current = current.as_ref().and_then(|v| v.as_choice()).unwrap_or("");
    let options: Vec<(String, String)> = options
        .iter()
        .map(|option| (option.value.clone(), short_label(&option.label)))
        .collect();
    segment_html(
        &format!("data-field=\"{}\"", esc(id)),
        label,
        &options,
        current,
    )
}

/// "Sample and hold" does not fit a key cap; "S/H" does.
fn short_label(label: &str) -> String {
    match label {
        "Sample and hold" => "S/H",
        "Saw down" => "SAW-",
        "Saw up" => "SAW+",
        "Triangle" => "TRI",
        "Square" => "SQR",
        "Sine" => "SIN",
        "Amplitude" => "AMP",
        other => other,
    }
    .to_owned()
}

/// A field the panel has no printed place for, drawn by its kind so nothing
/// a later plugin publishes is lost.
fn field_generic(state: &State, draft: &Draft, field: &Field) -> String {
    match field.kind {
        FieldKind::Toggle => field_switch(state, draft, &field.id, &field.label),
        FieldKind::Number { .. } => field_knob(state, draft, &field.id, &field.label),
        FieldKind::Choice { .. } => field_segment(state, draft, &field.id, &field.label),
    }
}

fn integer(state: &State, id: &str) -> i64 {
    state.field_value(id).and_then(|v| v.as_i64()).unwrap_or(0)
}

// ---- lit displays ------------------------------------------------------

/// An envelope as a shape on lit glass: four rates and four levels, drawn
/// from L4 up through L1, L2, L3, a hold, and back down to L4.
pub fn envelope_svg(rates: [i64; 4], levels: [i64; 4], pitch: bool) -> String {
    let duration = |rate: i64| 4.0 + ((99 - rate.clamp(0, 99)) as f64).powf(1.6) / 12.0;
    let widths = [
        duration(rates[0]),
        duration(rates[1]),
        duration(rates[2]),
        60.0,
        duration(rates[3]),
    ];
    let total: f64 = widths.iter().sum();
    let width = 240.0;
    let height = 64.0;
    let y = |level: i64| height - 6.0 - (level.clamp(0, 99) as f64 / 99.0) * (height - 12.0);
    let mut x = 4.0;
    let mut points = vec![(x, y(levels[3]))];
    for (segment, level) in [levels[0], levels[1], levels[2], levels[2], levels[3]]
        .iter()
        .enumerate()
    {
        x += widths[segment] / total * (width - 8.0);
        points.push((x, y(*level)));
    }
    let path: Vec<String> = points
        .iter()
        .map(|(x, y)| format!("{x:.1},{y:.1}"))
        .collect();
    let hold = points[3].0;
    let mut out = format!(
        "<svg class=\"env\" viewBox=\"0 0 {width:.0} {height:.0}\" preserveAspectRatio=\"none\" aria-hidden=\"true\"><g class=\"grid\">"
    );
    for step in 1..4 {
        let gy = height / 4.0 * step as f64;
        let _ = write!(
            out,
            "<line x1=\"0\" y1=\"{gy:.1}\" x2=\"{width:.0}\" y2=\"{gy:.1}\"/>"
        );
    }
    out.push_str("</g>");
    if pitch {
        let mid = y(50);
        let _ = write!(
            out,
            "<line class=\"mid\" x1=\"4\" y1=\"{mid:.1}\" x2=\"{:.1}\" y2=\"{mid:.1}\"/>",
            width - 4.0
        );
    }
    let _ = write!(
        out,
        "<line class=\"hold\" x1=\"{hold:.1}\" y1=\"4\" x2=\"{hold:.1}\" y2=\"{:.1}\"/>",
        height - 4.0
    );
    // An operator's envelope is an amount, so it is filled from the floor. A
    // pitch envelope is a deviation from the centre line, so it is a trace.
    if !pitch {
        let _ = write!(
            out,
            "<polygon class=\"fill\" points=\"4,{:.1} {} {:.1},{:.1}\"/>",
            height - 6.0,
            path.join(" "),
            points[5].0,
            height - 6.0
        );
    }
    let _ = write!(
        out,
        "<polyline class=\"line\" points=\"{}\"/>",
        path.join(" ")
    );
    out.push_str("</svg>");
    out
}

// ---- pages -------------------------------------------------------------

pub fn page(state: &State) -> String {
    if state.surface == "config" {
        return page_config(state);
    }
    match page_id(state) {
        "voice" => match &state.draft {
            Some(draft) => page_voice(state, draft),
            None => no_program(state),
        },
        "operators" => match &state.draft {
            Some(draft) => page_operators(state, draft),
            None => no_program(state),
        },
        "perform" => page_perform(state),
        _ => page_programs(state),
    }
}

/// What RF-7 reads, and the one resource it declares.
pub const CARTRIDGE: &str = "cartridge";
/// The extensions the host's explorer offers first. A chip image has no
/// framing and often no extension of its own, so `bin` is here too.
pub const CARTRIDGE_EXTENSIONS: [&str; 3] = ["syx", "bin", "dx7"];

/// The setup surface: what the instrument is playing from, and how to change
/// it. Everything here is the host's — RF-7 never sees a path.
fn page_config(state: &State) -> String {
    let installed = state.resource_installed(CARTRIDGE);
    let programs = state
        .instance
        .as_ref()
        .map_or(0, |instance| instance.sounds.len());
    let busy = !state.busy.is_empty();
    let mut cartridge = String::new();
    // The host says only that a cartridge is installed; the name is the
    // file SETUP put there last, while the host still lists it.
    let source = if !installed {
        "FACTORY BANK".to_owned()
    } else {
        state
            .installed_cartridge_name()
            .map_or_else(|| "YOUR CARTRIDGE".to_owned(), esc)
    };
    let _ = write!(
        cartridge,
        "<dl class=\"facts\"><dt>SOURCE</dt><dd>{source}</dd><dt>PROGRAMS</dt><dd>{programs}</dd></dl>",
    );
    let _ = write!(
        cartridge,
        "<div class=\"row\"><button type=\"button\" class=\"key wide\" data-action=\"choose-cartridge\"{}>{}</button><button type=\"button\" class=\"key wide\" data-action=\"clear-cartridge\"{}>REMOVE</button></div>",
        if busy || !state.connected {
            " disabled"
        } else {
            ""
        },
        if installed {
            "REPLACE…"
        } else {
            "INSTALL…"
        },
        if busy || !installed { " disabled" } else { "" }
    );
    // One line, because the buttons say the rest.
    cartridge.push_str(
        "<p class=\"small\">Bulk dumps, single voices or a raw chip image. Up to 128.</p>",
    );

    let mut out = String::from("<div class=\"page page-config\">");
    out.push_str(&group("cartridge", "blue", "VOICE CARTRIDGE", &cartridge));

    let grants: Vec<&crate::model::Grant> = state
        .grants
        .iter()
        .filter(|grant| grant.resource == CARTRIDGE)
        .collect();
    if !grants.is_empty() {
        let mut body = String::from("<div class=\"pads\">");
        for grant in grants {
            let lit = installed && state.installed_grant.as_deref() == Some(grant.id.as_str());
            let _ = write!(
                body,
                "<button type=\"button\" class=\"pad\" data-grant=\"{}\" aria-pressed=\"{lit}\"{}><span class=\"n\">{}</span><span class=\"name\">{}</span></button>",
                esc(&grant.id),
                if busy { " disabled" } else { "" },
                if lit { "●" } else { "↺" },
                esc(&grant.name)
            );
        }
        body.push_str("</div>");
        out.push_str(&group("grants", "amber", "ALREADY CHOSEN", &body));
    }

    let mut about = String::new();
    let _ = write!(
        about,
        "<dl class=\"facts\"><dt>PLUGIN</dt><dd>RF-7 {}</dd><dt>SAVED PROGRAMS</dt><dd>{}</dd><dt>HOST</dt><dd>{}</dd></dl>",
        env!("CARGO_PKG_VERSION"),
        state.instance.as_ref().map_or(0, |instance| instance
            .sounds
            .iter()
            .filter(|sound| sound.id.starts_with("custom."))
            .count()),
        if state.connected {
            "ANSWERING"
        } else {
            "SILENT"
        }
    );
    out.push_str(&group("about", "red", "INSTRUMENT", &about));

    if busy {
        let _ = write!(out, "<p class=\"note busy\">{}</p>", esc(&state.busy));
    } else if !state.notice.is_empty() {
        let _ = write!(out, "<p class=\"note\">{}</p>", esc(&state.notice));
    }
    out.push_str("</div>");
    out
}

fn no_program(state: &State) -> String {
    let line = match state.selected_sound() {
        Some(sound) if sound.editable => format!(
            "<strong>{}</strong> is playing. Press EDIT to open it — a library voice opens as a copy, so the cartridge stays as it is — or NEW to start from the initial voice.",
            esc(&sound.name)
        ),
        Some(sound) => format!("<strong>{}</strong> is playing.", esc(&sound.name)),
        None => "Choose a program on the PROGRAMS page.".to_owned(),
    };
    format!(
        "<section class=\"plate\"><h2>NO PROGRAM OPEN</h2><p>{line}</p><p class=\"small\">Every change previews on the next note. Saving adds the program to YOUR PROGRAMS and leaves a DX7 System Exclusive dump beside it.</p></section>"
    )
}

fn page_voice(state: &State, draft: &Draft) -> String {
    let algorithm = state
        .field_value("algorithm")
        .and_then(|v| v.as_choice().and_then(|a| a.parse::<usize>().ok()))
        .unwrap_or(1)
        .clamp(1, 32);
    let routing = &rf7_dsp::ALGORITHMS[algorithm - 1];
    let carriers: Vec<String> = (0..6)
        .filter(|op| routing.is_carrier(*op))
        .map(|op| format!("{}", op + 1))
        .collect();
    let mut chart = format!(
        "<div class=\"chart\"><div class=\"glass\">{}</div><div class=\"stepper\"><button type=\"button\" data-action=\"alg-prev\" aria-label=\"Previous algorithm\">&#8249;</button><span class=\"num\">{algorithm:02}</span><button type=\"button\" data-action=\"alg-next\" aria-label=\"Next algorithm\">&#8250;</button></div><p class=\"caption\">CARRIERS {}</p></div>",
        diagram::svg(algorithm),
        carriers.join(" · ")
    );
    chart.push_str("<div class=\"alg-fields\">");
    chart.push_str(&field_select(state, draft, "algorithm", "ALGORITHM"));
    chart.push_str(&field_knob(state, draft, "feedback", "FEEDBACK"));
    chart.push_str("</div>");

    let mut voice = String::new();
    voice.push_str(&field_knob(state, draft, "transpose", "TRANSPOSE"));
    voice.push_str(&field_knob(state, draft, "pms", "P.MOD SENS"));
    voice.push_str(&field_switch(state, draft, "sync", "OSC KEY SYNC"));

    let mut lfo = String::new();
    lfo.push_str(&field_segment(state, draft, "lfo.wave", "WAVE"));
    lfo.push_str(&field_knob(state, draft, "lfo.speed", "SPEED"));
    lfo.push_str(&field_knob(state, draft, "lfo.delay", "DELAY"));
    lfo.push_str(&field_knob(state, draft, "lfo.pmd", "PITCH DEPTH"));
    lfo.push_str(&field_knob(state, draft, "lfo.amd", "AMP DEPTH"));
    lfo.push_str(&field_switch(state, draft, "lfo.sync", "KEY SYNC"));

    let rates = [1, 2, 3, 4].map(|s| integer(state, &format!("peg.r{s}")));
    let levels = [1, 2, 3, 4].map(|s| integer(state, &format!("peg.l{s}")));
    let mut peg = format!(
        "<div class=\"screen wide\">{}</div>",
        envelope_svg(rates, levels, true)
    );
    for s in 1..=4 {
        peg.push_str(&field_knob(
            state,
            draft,
            &format!("peg.r{s}"),
            &format!("RATE {s}"),
        ));
    }
    for s in 1..=4 {
        peg.push_str(&field_knob(
            state,
            draft,
            &format!("peg.l{s}"),
            &format!("LEVEL {s}"),
        ));
    }

    let mut out = String::from("<div class=\"page page-voice\">");
    out.push_str(&group("algorithm", "blue", "ALGORITHM", &chart));
    out.push_str(&group("global", "blue", "VOICE", &voice));
    out.push_str(&group("lfo", "amber", "LFO", &lfo));
    out.push_str(&group("peg", "amber", "PITCH ENVELOPE", &peg));
    out.push_str(&extras(state, draft));
    out.push_str("</div>");
    out
}

/// The algorithm has thirty-two positions: too many for keys, so it keeps a
/// panel selector beside the chart.
fn field_select(state: &State, draft: &Draft, id: &str, label: &str) -> String {
    let Some(field) = draft.field(id) else {
        return String::new();
    };
    let FieldKind::Choice { options } = &field.kind else {
        return String::new();
    };
    let current = state.field_value(id);
    let current = current.as_ref().and_then(|v| v.as_choice()).unwrap_or("");
    let mut out = format!(
        "<div class=\"ctl select-ctl\"><span class=\"l\">{}</span><select data-field=\"{}\" aria-label=\"{}\">",
        esc(label),
        esc(id),
        esc(label)
    );
    for option in options {
        let _ = write!(
            out,
            "<option value=\"{}\"{}>{}{}</option>",
            esc(&option.value),
            if option.value == current {
                " selected"
            } else {
                ""
            },
            esc(&option.label),
            option
                .detail
                .as_deref()
                .map(|detail| format!(" — {}", esc(detail)))
                .unwrap_or_default()
        );
    }
    out.push_str("</select></div>");
    out
}

fn page_operators(state: &State, draft: &Draft) -> String {
    let algorithm = state
        .field_value("algorithm")
        .and_then(|v| v.as_choice().and_then(|a| a.parse::<usize>().ok()))
        .unwrap_or(1)
        .clamp(1, 32);
    let routing = &rf7_dsp::ALGORITHMS[algorithm - 1];
    let mut out = String::from("<div class=\"page page-operators\">");
    for n in 1..=6usize {
        out.push_str(&operator(state, draft, n, routing.is_carrier(n - 1)));
    }
    out.push_str(&extras(state, draft));
    out.push_str("</div>");
    out
}

fn operator(state: &State, draft: &Draft, n: usize, carrier: bool) -> String {
    let p = format!("op{n}");
    let f = |suffix: &str| format!("{p}.{suffix}");
    let fixed = state
        .field_value(&f("fixed"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let frequency = frequency_text(
        fixed,
        integer(state, &f("coarse")),
        integer(state, &f("fine")),
        integer(state, &f("detune")),
    );
    let rates = [1, 2, 3, 4].map(|s| integer(state, &f(&format!("eg.r{s}"))));
    let levels = [1, 2, 3, 4].map(|s| integer(state, &f(&format!("eg.l{s}"))));
    let role = if carrier { "CARRIER" } else { "MODULATOR" };
    let class = if carrier {
        "op carrier"
    } else {
        "op modulator"
    };
    // The operator's switch is one of the plugin's public parameters, so a
    // key here is the same key the PERFORM page has and a controller sees.
    let switch = state
        .parameters
        .as_ref()
        .and_then(|parameters| {
            parameters
                .parameters
                .iter()
                .find(|p| p.id == format!("operator_{n}"))
        })
        .map(|p| {
            let on = state.parameter_value(p.index).unwrap_or(p.value) >= 0.5;
            format!(
                "<button type=\"button\" class=\"sw mini\" role=\"switch\" aria-checked=\"{on}\" data-param=\"{}\" data-toggle title=\"{}\"><span class=\"led\"></span><span class=\"l\">ON</span></button>",
                p.index,
                esc(&p.name)
            )
        })
        .unwrap_or_default();
    let mut out = format!(
        "<section class=\"{class}\" data-op=\"{n}\"><header><span class=\"badge\">OP{n}</span><span class=\"role\">{role}</span><span class=\"freq\">{}</span><span class=\"out\">{}</span>{switch}</header><div class=\"screen\">{}</div>",
        esc(&frequency),
        integer(state, &f("out")),
        envelope_svg(rates, levels, false)
    );
    out.push_str("<h3>ENVELOPE</h3><div class=\"row four\">");
    for s in 1..=4 {
        out.push_str(&field_knob(
            state,
            draft,
            &f(&format!("eg.r{s}")),
            &format!("R{s}"),
        ));
    }
    for s in 1..=4 {
        out.push_str(&field_knob(
            state,
            draft,
            &f(&format!("eg.l{s}")),
            &format!("L{s}"),
        ));
    }
    out.push_str("</div><h3>OUTPUT</h3><div class=\"row three\">");
    out.push_str(&field_knob(state, draft, &f("out"), "LEVEL"));
    out.push_str(&field_knob(state, draft, &f("vel"), "VELOCITY"));
    out.push_str(&field_knob(state, draft, &f("ams"), "AM SENS"));
    out.push_str("</div><h3>FREQUENCY</h3><div class=\"row three\">");
    out.push_str(&field_knob(state, draft, &f("coarse"), "COARSE"));
    out.push_str(&field_knob(state, draft, &f("fine"), "FINE"));
    out.push_str(&field_knob(state, draft, &f("detune"), "DETUNE"));
    out.push_str("</div><div class=\"row\">");
    out.push_str(&field_switch(state, draft, &f("fixed"), "FIXED"));
    out.push_str("</div><h3>KEYBOARD SCALING</h3><div class=\"row four\">");
    out.push_str(&field_knob(state, draft, &f("bp"), "BREAK PT"));
    out.push_str(&field_knob(state, draft, &f("ld"), "L DEPTH"));
    out.push_str(&field_knob(state, draft, &f("rd"), "R DEPTH"));
    out.push_str(&field_knob(state, draft, &f("rs"), "RATE SCL"));
    out.push_str("</div><div class=\"row curves\">");
    out.push_str(&field_segment(state, draft, &f("lc"), "L CURVE"));
    out.push_str(&field_segment(state, draft, &f("rc"), "R CURVE"));
    out.push_str("</div></section>");
    out
}

/// The ids the panel prints by hand; anything else the plugin publishes is
/// drawn after them so no control is ever lost.
fn placed(id: &str) -> bool {
    const GLOBAL: [&str; 5] = ["algorithm", "feedback", "transpose", "sync", "pms"];
    if GLOBAL.contains(&id) || id.starts_with("lfo.") || id.starts_with("peg.") {
        return true;
    }
    let Some(rest) = id.strip_prefix("op") else {
        return false;
    };
    let mut chars = rest.chars();
    matches!(chars.next(), Some('1'..='6')) && chars.next() == Some('.')
}

fn extras(state: &State, draft: &Draft) -> String {
    let extra: Vec<&Field> = draft
        .order
        .iter()
        .filter(|id| !placed(id))
        .filter_map(|id| draft.field(id))
        .collect();
    if extra.is_empty() {
        return String::new();
    }
    let mut body = String::new();
    for field in extra {
        body.push_str(&field_generic(state, draft, field));
    }
    group("more", "red", "MORE", &body)
}

fn page_perform(state: &State) -> String {
    let Some(parameters) = &state.parameters else {
        return "<div class=\"page\"><section class=\"plate\"><h2>NO CONTROLS</h2><p>Waiting for RackForge to send the parameter schema.</p></section></div>".to_owned();
    };
    let mut pages = parameters.pages.clone();
    for parameter in &parameters.parameters {
        if !pages.iter().any(|(id, _)| *id == parameter.page) {
            pages.push((parameter.page.clone(), parameter.page.clone()));
        }
    }
    let mut out = String::from("<div class=\"page page-perform\">");
    for (page_id, page_name) in pages {
        let members: Vec<&Parameter> = parameters
            .parameters
            .iter()
            .filter(|p| p.page == page_id)
            .collect();
        if members.is_empty() {
            continue;
        }
        let mut body = String::new();
        for parameter in members {
            body.push_str(&control(state, parameter));
        }
        let colour = match page_id.as_str() {
            "output" => "red",
            "operators" => "amber",
            _ => "blue",
        };
        out.push_str(&group(&page_id, colour, &page_name.to_uppercase(), &body));
    }
    out.push_str("</div>");
    out
}

fn format_value(kind: &ParameterKind, value: f64) -> String {
    match kind {
        ParameterKind::Float { step, unit, .. } => {
            let decimals = if *step >= 1.0 {
                0
            } else if *step >= 0.1 {
                1
            } else {
                2
            };
            format!("{value:.decimals$}{}", unit.as_deref().unwrap_or(""))
        }
        ParameterKind::Integer { unit, .. } => {
            format!("{}{}", value.round() as i64, unit.as_deref().unwrap_or(""))
        }
        ParameterKind::Boolean => if value >= 0.5 { "ON" } else { "OFF" }.into(),
        ParameterKind::Enum { choices } => choices
            .iter()
            .find(|(v, _)| *v == value.round() as i64)
            .map(|(_, name)| name.clone())
            .unwrap_or_default(),
    }
}

fn control(state: &State, parameter: &Parameter) -> String {
    let value = state
        .parameter_value(parameter.index)
        .unwrap_or(parameter.value);
    let binding = format!("data-param=\"{}\"", parameter.index);
    let label = parameter
        .name
        .strip_prefix("Operator ")
        .map(|n| format!("OP{n}"))
        .unwrap_or_else(|| parameter.name.to_uppercase());
    match &parameter.kind {
        ParameterKind::Float {
            minimum,
            maximum,
            step,
            ..
        } => knob_html(
            &binding,
            &label,
            &parameter.name,
            value,
            *minimum,
            *maximum,
            *step,
            &format_value(&parameter.kind, value),
            parameter.default,
        ),
        ParameterKind::Integer {
            minimum, maximum, ..
        } => knob_html(
            &binding,
            &label,
            &parameter.name,
            value.round(),
            *minimum as f64,
            *maximum as f64,
            1.0,
            &format_value(&parameter.kind, value),
            parameter.default,
        ),
        ParameterKind::Boolean => switch_html(&binding, &label, &parameter.name, value >= 0.5),
        ParameterKind::Enum { choices } => {
            let options: Vec<(String, String)> = choices
                .iter()
                .map(|(v, name)| (v.to_string(), short_label(name).to_uppercase()))
                .collect();
            segment_html(
                &binding,
                &label,
                &options,
                &format!("{}", value.round() as i64),
            )
        }
    }
}

fn page_programs(state: &State) -> String {
    let Some(instance) = &state.instance else {
        return "<div class=\"page\"><section class=\"plate\"><h2>NO CATALOG</h2><p>Waiting for RackForge to send the programs.</p></section></div>".to_owned();
    };
    let mut banks: Vec<(String, String)> = instance
        .banks
        .iter()
        .map(|b| (b.id.clone(), b.name.clone()))
        .collect();
    for sound in &instance.sounds {
        if !banks.iter().any(|(id, _)| *id == sound.bank) {
            banks.push((sound.bank.clone(), sound.bank.clone()));
        }
    }
    let editing = state.draft.is_some();
    let mut out = String::from("<div class=\"page page-programs\">");
    for (bank_id, bank_name) in banks {
        let sounds: Vec<_> = instance
            .sounds
            .iter()
            .filter(|s| s.bank == bank_id)
            .collect();
        if sounds.is_empty() {
            continue;
        }
        let mut body = String::from("<div class=\"pads\">");
        for (index, sound) in sounds.iter().enumerate() {
            let selected = instance.selected.as_deref() == Some(sound.id.as_str());
            let saved = if sound.id.starts_with("custom.") {
                " saved"
            } else {
                ""
            };
            let _ = write!(
                body,
                "<button type=\"button\" class=\"pad{saved}\" data-sound=\"{}\" aria-pressed=\"{selected}\"{}><span class=\"n\">{:02}</span><span class=\"name\">{}</span></button>",
                esc(&sound.id),
                if editing { " disabled" } else { "" },
                index + 1,
                esc(&sound.name)
            );
        }
        body.push_str("</div>");
        let colour = if bank_id == "bank-user" {
            "amber"
        } else {
            "blue"
        };
        out.push_str(&group(&bank_id, colour, &bank_name.to_uppercase(), &body));
    }
    if editing {
        out.push_str(
            "<p class=\"note\">A program is open. Save or exit it before changing what plays.</p>",
        );
    }
    out.push_str("</div>");
    out
}

/// The label RackForge shows in its performance bar while a draft is open.
pub fn surface_info(state: &State) -> Option<String> {
    let draft = state.draft.as_ref()?;
    Some(format!(
        "{}{}",
        if draft.dirty { "* " } else { "" },
        draft.name
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{FieldValue, State};
    use serde_json::json;

    fn state() -> State {
        let mut state = State::default();
        assert!(state.apply_context(&crate::model::tests::context(), "org.rackforge.rf7"));
        state
    }

    #[test]
    fn the_display_reads_like_the_instrument_it_is() {
        let mut state = state();
        let html = header(&state);
        assert!(html.contains("EDITING · UNSAVED"));
        assert!(
            html.contains("<span class=\"code\">U01</span>"),
            "the saved program it came from: {html}"
        );
        assert!(
            html.contains("data-action=\"compare\" aria-pressed=\"false\" disabled"),
            "nothing has moved, so there is nothing to compare"
        );
        let mut moved = state.clone();
        moved
            .draft
            .as_mut()
            .unwrap()
            .fields
            .get_mut("feedback")
            .unwrap()
            .value = FieldValue::Integer(1);
        assert!(header(&moved).contains("data-action=\"compare\" aria-pressed=\"false\">"));
        moved.comparing = true;
        let html = header(&moved);
        assert!(html.contains("COMPARING · AS OPENED"));
        assert!(html.contains("data-action=\"compare\" aria-pressed=\"true\">"));
        assert_eq!(program_number("program-128"), "128");
        assert_eq!(program_number("custom.user.rf7-014"), "U14");
        assert_eq!(program_number("custom.other"), "US");
        state.draft.as_mut().unwrap().original_program_id = None;
        assert!(header(&state).contains("<span class=\"code\">--</span>"));
        assert!(html.contains("value=\"Quiet tines\""));
        assert!(
            html.contains("data-action=\"save\" aria-pressed=\"true\""),
            "SAVE lit: unsaved"
        );
        assert!(
            html.contains("data-action=\"edit\" aria-pressed=\"true\""),
            "EDIT lit while editing"
        );
        assert!(html.contains("data-action=\"new\" aria-pressed=\"false\" disabled"));
        state.draft = None;
        let html = header(&state);
        assert!(html.contains("PLAYING"));
        assert!(html.contains("<span class=\"code\">02</span>"), "{html}");
        assert!(html.contains("RF EP SOFT"));
        assert!(
            html.contains("data-action=\"edit\" aria-pressed=\"false\">"),
            "EDIT dark and live"
        );
        assert!(html.contains("data-action=\"save\" aria-pressed=\"false\" disabled"));
        state.disconnect("Host timed out");
        assert!(header(&state).contains("HOST TIMED OUT"));
        assert!(header(&state).contains("data-action=\"edit\" aria-pressed=\"false\" disabled"));
    }

    #[test]
    fn the_tabs_follow_the_page_and_a_draft_opens_the_voice_page() {
        let state = state();
        assert_eq!(page_id(&state), "voice", "a draft opens on the voice page");
        let html = tabs(&state);
        assert_eq!(html.matches("data-tab=").count(), 4);
        assert!(html.contains("class=\"tab active\" data-tab=\"voice\" aria-selected=\"true\""));
        let mut elsewhere = state;
        elsewhere.page = "nowhere".into();
        assert_eq!(
            page_id(&elsewhere),
            "programs",
            "an unknown page falls back"
        );
    }

    #[test]
    fn the_voice_page_prints_the_chart_and_its_controls() {
        let mut state = state();
        let html = page(&state);
        assert!(html.contains("aria-label=\"Algorithm 5\""));
        assert!(html.contains("<span class=\"num\">05</span>"));
        assert!(html.contains("CARRIERS 1 · 3 · 5"));
        assert!(html.contains("<select data-field=\"algorithm\""));
        assert!(html.contains(
            "data-field=\"feedback\" min=\"0\" max=\"7\" step=\"1\" value=\"6\" data-default=\"6\""
        ));
        assert!(
            html.contains("--knob-turn:96.43deg"),
            "the knob points at 6 of 7"
        );
        assert!(html.contains("aria-checked=\"true\" data-field=\"sync\""));
        assert!(
            !html.contains("group-more"),
            "everything had a printed place"
        );
        // A pending edit shows in place of the draft's value.
        state
            .pending_fields
            .insert("feedback".into(), (FieldValue::Integer(0), false));
        assert!(
            page(&state)
                .contains("data-field=\"feedback\" min=\"0\" max=\"7\" step=\"1\" value=\"0\"")
        );
    }

    #[test]
    fn the_operator_page_prints_six_operators_with_their_roles() {
        let mut state = state();
        state.page = "operators".into();
        assert!(
            !page(&state).contains("class=\"sw mini\""),
            "no switch until the parameters have arrived"
        );
        state.apply_parameters(&json!({
            "schema": {"pages": [{"id": "operators", "name": "Operators"}], "parameters": [
                {"index": 11, "id": "operator_1", "name": "Operator 1", "page": "operators", "kind": {"type": "boolean", "default": true}},
                {"index": 12, "id": "operator_2", "name": "Operator 2", "page": "operators", "kind": {"type": "boolean", "default": true}}
            ]},
            "values": [{"index": 11, "value": 1.0}, {"index": 12, "value": 0.0}]
        }));
        let html = page(&state);
        assert!(html.contains("aria-checked=\"true\" data-param=\"11\" data-toggle"));
        assert!(html.contains("aria-checked=\"false\" data-param=\"12\" data-toggle"));
        assert_eq!(
            html.matches("class=\"sw mini\"").count(),
            2,
            "only the operators the schema names"
        );
        assert_eq!(html.matches("class=\"op ").count(), 6);
        assert!(html.contains("class=\"op carrier\" data-op=\"1\""));
        assert!(html.contains("class=\"op modulator\" data-op=\"2\""));
        assert!(html.contains("data-field=\"op1.coarse\""));
        assert_eq!(html.matches("<svg class=\"env\"").count(), 6);
    }

    #[test]
    fn without_a_draft_the_editing_pages_explain_themselves() {
        let mut state = state();
        state.draft = None;
        state.page = "voice".into();
        let html = page(&state);
        assert!(html.contains("NO PROGRAM OPEN"));
        assert!(html.contains("<strong>RF EP SOFT</strong>"));
    }

    #[test]
    fn programs_are_pads_grouped_by_bank_and_locked_while_editing() {
        let mut playing = state();
        playing.page = "programs".into();
        playing.draft = None;
        let html = page(&playing);
        assert!(html.contains("<h2>BANK 1</h2>"));
        assert!(html.contains("<h2>YOUR PROGRAMS</h2>"));
        assert!(html.contains("data-sound=\"program-002\" aria-pressed=\"true\""));
        assert!(html.contains("class=\"pad saved\" data-sound=\"custom.user.rf7-001\""));
        assert!(!html.contains("disabled"));
        let mut editing = state();
        editing.page = "programs".into();
        let html = page(&editing);
        assert!(html.contains("aria-pressed=\"false\" disabled"));
        assert!(html.contains("Save or exit it"));
    }

    #[test]
    fn unknown_fields_are_still_printed() {
        let mut state = state();
        let draft = state.draft.as_mut().unwrap();
        draft.order.push("portamento".into());
        draft.fields.insert(
            "portamento".into(),
            Field {
                id: "portamento".into(),
                label: "Portamento".into(),
                detail: String::new(),
                value: FieldValue::Integer(3),
                kind: FieldKind::Number {
                    minimum: 0,
                    maximum: 99,
                    unit: None,
                },
            },
        );
        let html = page(&state);
        assert!(html.contains("group-more"));
        assert!(html.contains("data-field=\"portamento\""));
    }

    #[test]
    fn the_perform_page_follows_the_parameter_schema() {
        let mut state = state();
        state.page = "perform".into();
        state.apply_parameters(&json!({
            "schema": {"pages": [{"id": "output", "name": "Output"}, {"id": "operators", "name": "Operators"}], "parameters": [
                {"index": 0, "id": "gain", "name": "Output Gain", "page": "output",
                 "kind": {"type": "float", "minimum": 0.0, "maximum": 2.0, "default": 0.3, "step": 0.01, "unit": "x"}},
                {"index": 11, "id": "operator_1", "name": "Operator 1", "page": "operators", "kind": {"type": "boolean", "default": true}},
                {"index": 5, "id": "wheel_target", "name": "Mod Wheel Target", "page": "performance",
                 "kind": {"type": "enum", "default": 0, "choices": [{"value": 0, "name": "Pitch"}, {"value": 1, "name": "Amplitude"}]}}
            ]},
            "values": [{"index": 0, "value": 0.3}, {"index": 11, "value": 1.0}, {"index": 5, "value": 1.0}]
        }));
        let html = page(&state);
        assert!(html.contains("<h2>OUTPUT</h2>"));
        assert!(html.contains(
            "data-param=\"0\" min=\"0\" max=\"2\" step=\"0.01\" value=\"0.3\" data-default=\"0.3\""
        ));
        assert!(html.contains("<output>0.30x</output>"));
        assert!(html.contains("aria-checked=\"true\" data-param=\"11\""));
        assert!(html.contains("data-param=\"5\" data-value=\"1\" aria-pressed=\"true\">AMP<"));
        assert!(
            html.contains("<h2>PERFORMANCE</h2>"),
            "an unlisted page still prints"
        );
    }

    #[test]
    fn the_config_surface_offers_the_cartridge_and_nothing_to_play() {
        let mut state = state();
        state.surface = "config".into();
        let html = header(&state);
        assert!(html.contains("SETUP"));
        assert!(
            !html.contains("data-action=\"edit\""),
            "no program keys here"
        );
        assert!(!html.contains("data-action=\"save\""));

        let html = page(&state);
        assert!(html.contains("<h2>VOICE CARTRIDGE</h2>"));
        assert!(html.contains("data-action=\"choose-cartridge\">INSTALL…"));
        // Nothing to remove until something is installed.
        assert!(html.contains("data-action=\"clear-cartridge\" disabled"));
        assert!(html.contains("FACTORY BANK"));
        assert!(!html.contains("ALREADY CHOSEN"));

        state.resources = vec![(CARTRIDGE.to_owned(), true)];
        state.grants = vec![crate::model::Grant {
            id: "g1".into(),
            name: "ROM1A.syx".into(),
            resource: CARTRIDGE.into(),
        }];
        let html = page(&state);
        assert!(html.contains("REPLACE…"));
        assert!(html.contains("YOUR CARTRIDGE"));
        assert!(!html.contains("data-action=\"clear-cartridge\" disabled"));
        assert!(html.contains("data-grant=\"g1\" aria-pressed=\"false\""));
        assert!(html.contains("ROM1A.syx"));
        // Once SETUP has installed a grant it names the source and lights
        // that pad, as long as the host still lists the grant.
        state.installed_grant = Some("g1".into());
        let html = page(&state);
        assert!(html.contains("<dt>SOURCE</dt><dd>ROM1A.syx</dd>"));
        assert!(html.contains("data-grant=\"g1\" aria-pressed=\"true\""));
        state.installed_grant = Some("gone".into());
        assert!(page(&state).contains("YOUR CARTRIDGE"));
        state.installed_grant = Some("g1".into());

        // While the host is working, nothing can be pressed twice.
        state.busy = "Installing…".into();
        let html = page(&state);
        assert!(html.contains("data-action=\"choose-cartridge\" disabled"));
        assert!(html.contains("data-grant=\"g1\" aria-pressed=\"true\" disabled"));
        assert!(html.contains("class=\"note busy\">Installing…"));
    }

    #[test]
    fn a_knob_points_where_its_value_says() {
        assert!((knob_angle(0.0, 0.0, 99.0) + 135.0).abs() < 1e-9);
        assert!((knob_angle(99.0, 0.0, 99.0) - 135.0).abs() < 1e-9);
        assert!(knob_angle(50.0, 0.0, 99.0).abs() < 2.0);
        assert!(
            (knob_angle(-5.0, 0.0, 99.0) + 135.0).abs() < 1e-9,
            "clamped"
        );
        assert!((knob_angle(1.0, 1.0, 1.0) + 135.0).abs() < 1e-9, "no range");
    }

    #[test]
    fn text_is_escaped_everywhere_a_name_can_reach() {
        let mut state = state();
        state.draft.as_mut().unwrap().name = "<b>\"x\"</b>".into();
        let html = header(&state);
        assert!(!html.contains("<b>"));
        assert!(html.contains("&lt;b&gt;&quot;x&quot;&lt;/b&gt;"));
        assert_eq!(esc("a&b"), "a&amp;b");
    }

    #[test]
    fn envelopes_are_drawn_from_their_numbers() {
        let fast = envelope_svg([99, 99, 99, 99], [99, 99, 99, 0], false);
        let slow = envelope_svg([0, 0, 0, 0], [99, 99, 99, 0], false);
        assert!(fast.contains("<polyline"));
        assert_ne!(fast, slow);
        let pitch = envelope_svg([50; 4], [50; 4], true);
        assert!(pitch.contains("class=\"mid\""));
        assert!(
            !pitch.contains("<polygon"),
            "a pitch envelope is a trace, not an amount"
        );
        assert!(fast.contains("<polygon"), "an operator envelope is filled");
    }
}
