//! The declarative editor: a voice as pages of fields, and a field edit as a
//! change to a voice.
//!
//! The host draws this tree on every surface — the Desktop window, the Web
//! shell, the two-line display of a controller — so it is shaped for the
//! smallest: short pages, one operator per page, and the operator's four
//! groups as sub-pages, so no list is longer than nine entries.
//!
//! Field identifiers are the contract between the two halves of this file.
//! `view` writes them; `apply` reads them. A test walks every field the view
//! publishes through `apply` so the two cannot drift apart.

use crate::document::{
    CURVE_NAMES, EditorChoice, EditorField, EditorFieldKind, EditorPage, EditorValue, EditorView,
    PROGRAM_EDITOR_SCHEMA_VERSION, WAVEFORM_NAMES, curve_index, curve_name, waveform_index,
    waveform_name,
};
use rf7_dsp::ALGORITHMS;
use rf7_voice::{OPERATORS, Voice, printable_name};

/// The whole editor for one voice.
pub fn view(voice: &Voice) -> EditorView {
    let name = printable_name(&voice.name);
    EditorView {
        schema_version: PROGRAM_EDITOR_SCHEMA_VERSION,
        title: if name.is_empty() {
            "RF-7 voice".to_owned()
        } else {
            name.to_owned()
        },
        pages: vec![
            voice_page(voice),
            lfo_page(voice),
            pitch_eg_page(voice),
            EditorPage {
                id: "operators".into(),
                label: "Operators".into(),
                detail: "Six operators, each on its own page".into(),
                enabled: true,
                pages: (0..OPERATORS)
                    .map(|index| operator_page(voice, index))
                    .collect(),
                fields: Vec::new(),
            },
        ],
    }
}

fn voice_page(voice: &Voice) -> EditorPage {
    EditorPage {
        id: "voice".into(),
        label: "Voice".into(),
        detail: "Algorithm, feedback and tuning".into(),
        enabled: true,
        pages: Vec::new(),
        fields: vec![
            EditorField {
                id: "algorithm".into(),
                label: "Algorithm".into(),
                detail: "Which operators are heard, and who modulates whom".into(),
                value: EditorValue::Choice(format!("{}", voice.algorithm + 1)),
                kind: EditorFieldKind::Choice {
                    options: (0..32u8)
                        .map(|index| EditorChoice {
                            value: format!("{}", index + 1),
                            label: format!("Algorithm {}", index + 1),
                            detail: Some(algorithm_summary(index)),
                        })
                        .collect(),
                },
                live_preview: true,
            },
            number(
                "feedback",
                "Feedback",
                "How hard the loop operator feeds itself",
                voice.feedback,
                0,
                7,
                None,
            ),
            number(
                "transpose",
                "Transpose",
                "Semitones either side of centre",
                i64::from(voice.transpose) - 24,
                -24,
                24,
                Some("st"),
            ),
            toggle(
                "sync",
                "Oscillator sync",
                "Restart every operator's phase on each key",
                voice.oscillator_sync,
            ),
            number(
                "pms",
                "Pitch mod sensitivity",
                "How far the LFO can bend the pitch",
                voice.pitch_mod_sensitivity,
                0,
                7,
                None,
            ),
        ],
    }
}

fn lfo_page(voice: &Voice) -> EditorPage {
    EditorPage {
        id: "lfo".into(),
        label: "LFO".into(),
        detail: "One oscillator shared by every note".into(),
        enabled: true,
        pages: Vec::new(),
        fields: vec![
            number(
                "lfo.speed",
                "Speed",
                "0 is slowest, 99 fastest",
                voice.lfo.speed,
                0,
                99,
                None,
            ),
            number(
                "lfo.delay",
                "Delay",
                "How long a phrase waits before the LFO arrives",
                voice.lfo.delay,
                0,
                99,
                None,
            ),
            number(
                "lfo.pmd",
                "Pitch depth",
                "Vibrato, before the wheel adds any",
                voice.lfo.pitch_mod_depth,
                0,
                99,
                None,
            ),
            number(
                "lfo.amd",
                "Amp depth",
                "Tremolo, for operators that answer it",
                voice.lfo.amp_mod_depth,
                0,
                99,
                None,
            ),
            EditorField {
                id: "lfo.wave".into(),
                label: "Waveform".into(),
                detail: "The shape the LFO traces".into(),
                value: EditorValue::Choice(waveform_name(voice.lfo.shape()).to_owned()),
                kind: EditorFieldKind::Choice {
                    options: WAVEFORM_NAMES
                        .iter()
                        .zip([
                            "Triangle",
                            "Saw down",
                            "Saw up",
                            "Square",
                            "Sine",
                            "Sample and hold",
                        ])
                        .map(|(value, label)| EditorChoice {
                            value: (*value).to_owned(),
                            label: (*label).to_owned(),
                            detail: None,
                        })
                        .collect(),
                },
                live_preview: true,
            },
            toggle(
                "lfo.sync",
                "Key sync",
                "Restart the LFO on the first key of a phrase",
                voice.lfo.sync,
            ),
        ],
    }
}

fn pitch_eg_page(voice: &Voice) -> EditorPage {
    let mut fields = Vec::new();
    for segment in 0..4 {
        fields.push(number(
            &format!("peg.r{}", segment + 1),
            &format!("Rate {}", segment + 1),
            "Higher is faster",
            voice.pitch_eg_rate[segment],
            0,
            99,
            None,
        ));
    }
    for segment in 0..4 {
        fields.push(number(
            &format!("peg.l{}", segment + 1),
            &format!("Level {}", segment + 1),
            "50 is no change; above bends up, below bends down",
            voice.pitch_eg_level[segment],
            0,
            99,
            None,
        ));
    }
    EditorPage {
        id: "pitch-eg".into(),
        label: "Pitch envelope".into(),
        detail: "Four rates and four levels, shared by every operator".into(),
        enabled: true,
        pages: Vec::new(),
        fields,
    }
}

fn operator_page(voice: &Voice, index: usize) -> EditorPage {
    let operator = &voice.operators[index];
    let algorithm = &ALGORITHMS[usize::from(voice.algorithm.min(31))];
    let n = index + 1;
    let role = if algorithm.is_carrier(index) {
        "carrier: heard directly"
    } else {
        "modulator: shapes another operator"
    };
    let prefix = format!("op{n}");
    let mut envelope = Vec::new();
    for segment in 0..4 {
        envelope.push(number(
            &format!("{prefix}.eg.r{}", segment + 1),
            &format!("Rate {}", segment + 1),
            "Higher is faster",
            operator.eg_rate[segment],
            0,
            99,
            None,
        ));
    }
    for segment in 0..4 {
        envelope.push(number(
            &format!("{prefix}.eg.l{}", segment + 1),
            &format!("Level {}", segment + 1),
            "Level 3 holds while the key is down; level 4 ends the release",
            operator.eg_level[segment],
            0,
            99,
            None,
        ));
    }
    EditorPage {
        id: prefix.clone(),
        label: format!("Operator {n}"),
        detail: role.into(),
        enabled: true,
        pages: vec![
            EditorPage {
                id: format!("{prefix}.freq"),
                label: "Frequency".into(),
                detail: "Ratio to the key, or a fixed pitch".into(),
                enabled: true,
                pages: Vec::new(),
                fields: vec![
                    toggle(
                        &format!("{prefix}.fixed"),
                        "Fixed frequency",
                        "Ignore the key and hold one pitch",
                        operator.fixed_frequency,
                    ),
                    number(
                        &format!("{prefix}.coarse"),
                        "Coarse",
                        "Ratio 0 is a half; fixed mode uses the low two bits as decades",
                        operator.coarse,
                        0,
                        31,
                        None,
                    ),
                    number(
                        &format!("{prefix}.fine"),
                        "Fine",
                        "Hundredths of the coarse ratio",
                        operator.fine,
                        0,
                        99,
                        None,
                    ),
                    number(
                        &format!("{prefix}.detune"),
                        "Detune",
                        "Small steps either side of centre",
                        i64::from(operator.detune) - 7,
                        -7,
                        7,
                        None,
                    ),
                ],
            },
            EditorPage {
                id: format!("{prefix}.level"),
                label: "Level".into(),
                detail: "Output, and what velocity and the LFO do to it".into(),
                enabled: true,
                pages: Vec::new(),
                fields: vec![
                    number(
                        &format!("{prefix}.out"),
                        "Output level",
                        "On a modulator this is the modulation depth",
                        operator.output_level,
                        0,
                        99,
                        None,
                    ),
                    number(
                        &format!("{prefix}.vel"),
                        "Velocity sensitivity",
                        "0 ignores velocity; 7 answers it fully",
                        operator.velocity_sensitivity,
                        0,
                        7,
                        None,
                    ),
                    number(
                        &format!("{prefix}.ams"),
                        "Amp mod sensitivity",
                        "How much LFO tremolo reaches this operator",
                        operator.amp_mod_sensitivity,
                        0,
                        3,
                        None,
                    ),
                ],
            },
            EditorPage {
                id: format!("{prefix}.eg"),
                label: "Envelope".into(),
                detail: "Four rates, four levels".into(),
                enabled: true,
                pages: Vec::new(),
                fields: envelope,
            },
            EditorPage {
                id: format!("{prefix}.scaling"),
                label: "Keyboard scaling".into(),
                detail: "Level and rate across the keyboard".into(),
                enabled: true,
                pages: Vec::new(),
                fields: vec![
                    number(
                        &format!("{prefix}.bp"),
                        "Break point",
                        "39 is C3",
                        operator.break_point,
                        0,
                        99,
                        None,
                    ),
                    number(
                        &format!("{prefix}.ld"),
                        "Left depth",
                        "Below the break point",
                        operator.left_depth,
                        0,
                        99,
                        None,
                    ),
                    curve(&format!("{prefix}.lc"), "Left curve", operator.left_curve),
                    number(
                        &format!("{prefix}.rd"),
                        "Right depth",
                        "Above the break point",
                        operator.right_depth,
                        0,
                        99,
                        None,
                    ),
                    curve(&format!("{prefix}.rc"), "Right curve", operator.right_curve),
                    number(
                        &format!("{prefix}.rs"),
                        "Rate scaling",
                        "Shorter envelopes higher up the keyboard",
                        operator.rate_scaling,
                        0,
                        7,
                        None,
                    ),
                ],
            },
        ],
        fields: Vec::new(),
    }
}

fn number(
    id: &str,
    label: &str,
    detail: &str,
    value: impl Into<i64>,
    minimum: i64,
    maximum: i64,
    unit: Option<&str>,
) -> EditorField {
    EditorField {
        id: id.to_owned(),
        label: label.to_owned(),
        detail: detail.to_owned(),
        value: EditorValue::Integer(value.into()),
        kind: EditorFieldKind::Number {
            minimum,
            maximum,
            step: 1,
            decimals: 0,
            unit: unit.map(str::to_owned),
            allow_inherited: false,
        },
        live_preview: true,
    }
}

fn toggle(id: &str, label: &str, detail: &str, value: bool) -> EditorField {
    EditorField {
        id: id.to_owned(),
        label: label.to_owned(),
        detail: detail.to_owned(),
        value: EditorValue::Boolean(value),
        kind: EditorFieldKind::Toggle,
        live_preview: true,
    }
}

fn curve(id: &str, label: &str, index: u8) -> EditorField {
    EditorField {
        id: id.to_owned(),
        label: label.to_owned(),
        detail: "Negative curves cut away from the break point, positive boost".into(),
        value: EditorValue::Choice(curve_name(rf7_voice::Curve::from_index(index)).to_owned()),
        kind: EditorFieldKind::Choice {
            options: CURVE_NAMES
                .iter()
                .zip(["-LIN", "-EXP", "+EXP", "+LIN"])
                .map(|(value, label)| EditorChoice {
                    value: (*value).to_owned(),
                    label: (*label).to_owned(),
                    detail: None,
                })
                .collect(),
        },
        live_preview: true,
    }
}

/// "2>1, 6>5>4>3" — the routing, as the panel's diagram would draw it.
fn algorithm_summary(index: u8) -> String {
    let algorithm = &ALGORITHMS[usize::from(index)];
    let mut chains: Vec<String> = Vec::new();
    for destination in 0..OPERATORS {
        let sources: Vec<String> = (0..OPERATORS)
            .filter(|source| algorithm.modulators[destination] & (1 << source) != 0)
            .map(|source| format!("{}", source + 1))
            .collect();
        if !sources.is_empty() {
            chains.push(format!("{}>{}", sources.join("+"), destination + 1));
        }
    }
    let carriers: Vec<String> = (0..OPERATORS)
        .filter(|op| algorithm.is_carrier(*op))
        .map(|op| format!("{}", op + 1))
        .collect();
    let mut summary = format!("out {}", carriers.join(","));
    if !chains.is_empty() {
        summary.push_str("; ");
        summary.push_str(&chains.join(" "));
    }
    summary.truncate(64);
    summary
}

/// Change one field of a voice. `None` if the field or the value is not one
/// the view publishes, in which case the voice is untouched.
pub fn apply(voice: &Voice, field_id: &str, value: &EditorValue) -> Option<Voice> {
    let mut edited = *voice;
    let ok = match field_id {
        "algorithm" => choice(value)
            .and_then(|v| v.parse::<u8>().ok())
            .filter(|a| (1..=32).contains(a))
            .map(|a| edited.algorithm = a - 1)
            .is_some(),
        "feedback" => set_u8(&mut edited.feedback, value, 0, 7),
        "transpose" => integer(value, -24, 24)
            .map(|t| edited.transpose = (t + 24) as u8)
            .is_some(),
        "sync" => boolean(value).map(|b| edited.oscillator_sync = b).is_some(),
        "pms" => set_u8(&mut edited.pitch_mod_sensitivity, value, 0, 7),
        "lfo.speed" => set_u8(&mut edited.lfo.speed, value, 0, 99),
        "lfo.delay" => set_u8(&mut edited.lfo.delay, value, 0, 99),
        "lfo.pmd" => set_u8(&mut edited.lfo.pitch_mod_depth, value, 0, 99),
        "lfo.amd" => set_u8(&mut edited.lfo.amp_mod_depth, value, 0, 99),
        "lfo.sync" => boolean(value).map(|b| edited.lfo.sync = b).is_some(),
        "lfo.wave" => choice(value)
            .and_then(waveform_index)
            .map(|w| edited.lfo.waveform = w)
            .is_some(),
        _ => apply_indexed(&mut edited, field_id, value),
    };
    ok.then_some(edited)
}

fn apply_indexed(voice: &mut Voice, field_id: &str, value: &EditorValue) -> bool {
    if let Some(rest) = field_id.strip_prefix("peg.") {
        let Some((kind, segment)) = rest.split_at_checked(1) else {
            return false;
        };
        let Some(segment) = segment
            .parse::<usize>()
            .ok()
            .filter(|s| (1..=4).contains(s))
        else {
            return false;
        };
        return match kind {
            "r" => set_u8(&mut voice.pitch_eg_rate[segment - 1], value, 0, 99),
            "l" => set_u8(&mut voice.pitch_eg_level[segment - 1], value, 0, 99),
            _ => false,
        };
    }
    let Some(rest) = field_id.strip_prefix("op") else {
        return false;
    };
    let Some((digit, rest)) = rest.split_at_checked(1) else {
        return false;
    };
    let Some(index) = digit
        .parse::<usize>()
        .ok()
        .filter(|n| (1..=OPERATORS).contains(n))
    else {
        return false;
    };
    let Some(rest) = rest.strip_prefix('.') else {
        return false;
    };
    let operator = &mut voice.operators[index - 1];
    match rest {
        "fixed" => boolean(value)
            .map(|b| operator.fixed_frequency = b)
            .is_some(),
        "coarse" => set_u8(&mut operator.coarse, value, 0, 31),
        "fine" => set_u8(&mut operator.fine, value, 0, 99),
        "detune" => integer(value, -7, 7)
            .map(|d| operator.detune = (d + 7) as u8)
            .is_some(),
        "out" => set_u8(&mut operator.output_level, value, 0, 99),
        "vel" => set_u8(&mut operator.velocity_sensitivity, value, 0, 7),
        "ams" => set_u8(&mut operator.amp_mod_sensitivity, value, 0, 3),
        "bp" => set_u8(&mut operator.break_point, value, 0, 99),
        "ld" => set_u8(&mut operator.left_depth, value, 0, 99),
        "rd" => set_u8(&mut operator.right_depth, value, 0, 99),
        "rs" => set_u8(&mut operator.rate_scaling, value, 0, 7),
        "lc" => choice(value)
            .and_then(curve_index)
            .map(|c| operator.left_curve = c)
            .is_some(),
        "rc" => choice(value)
            .and_then(curve_index)
            .map(|c| operator.right_curve = c)
            .is_some(),
        other => {
            let Some(rest) = other.strip_prefix("eg.") else {
                return false;
            };
            let Some((kind, segment)) = rest.split_at_checked(1) else {
                return false;
            };
            let Some(segment) = segment
                .parse::<usize>()
                .ok()
                .filter(|s| (1..=4).contains(s))
            else {
                return false;
            };
            match kind {
                "r" => set_u8(&mut operator.eg_rate[segment - 1], value, 0, 99),
                "l" => set_u8(&mut operator.eg_level[segment - 1], value, 0, 99),
                _ => false,
            }
        }
    }
}

fn integer(value: &EditorValue, minimum: i64, maximum: i64) -> Option<i64> {
    match value {
        EditorValue::Integer(v) if (minimum..=maximum).contains(v) => Some(*v),
        _ => None,
    }
}

fn set_u8(target: &mut u8, value: &EditorValue, minimum: i64, maximum: i64) -> bool {
    match integer(value, minimum, maximum) {
        Some(v) => {
            *target = v as u8;
            true
        }
        None => false,
    }
}

fn boolean(value: &EditorValue) -> Option<bool> {
    match value {
        EditorValue::Boolean(b) => Some(*b),
        _ => None,
    }
}

fn choice(value: &EditorValue) -> Option<&str> {
    match value {
        EditorValue::Choice(c) => Some(c.as_str()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rf7_voice::factory_voice;
    use std::collections::BTreeSet;

    fn walk<'a>(
        page: &'a EditorPage,
        ids: &mut BTreeSet<String>,
        fields: &mut Vec<&'a EditorField>,
    ) {
        assert!(ids.insert(page.id.clone()), "duplicate id {}", page.id);
        assert!(valid_id(&page.id), "bad page id {}", page.id);
        assert!(
            valid_text(&page.label) && valid_text(&page.detail),
            "{}",
            page.id
        );
        assert!(
            !page.pages.is_empty() || !page.fields.is_empty(),
            "empty page {}",
            page.id
        );
        for field in &page.fields {
            assert!(ids.insert(field.id.clone()), "duplicate id {}", field.id);
            assert!(valid_id(&field.id), "bad field id {}", field.id);
            assert!(
                valid_text(&field.label) && valid_text(&field.detail),
                "{}",
                field.id
            );
            fields.push(field);
        }
        for child in &page.pages {
            walk(child, ids, fields);
        }
    }

    fn valid_id(id: &str) -> bool {
        !id.is_empty()
            && !id.starts_with('.')
            && !id.ends_with('.')
            && !id.contains("..")
            && id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b".-_".contains(&b))
    }

    fn valid_text(text: &str) -> bool {
        !text.trim().is_empty() && text.is_ascii() && text.len() <= 64 && !text.contains('\0')
    }

    #[test]
    fn the_view_passes_the_hosts_rules_and_every_field_applies() {
        let voice = factory_voice(0);
        let view = view(&voice);
        assert_eq!(view.schema_version, PROGRAM_EDITOR_SCHEMA_VERSION);
        assert!(valid_text(&view.title));
        let mut ids = BTreeSet::new();
        let mut fields = Vec::new();
        for page in &view.pages {
            walk(page, &mut ids, &mut fields);
        }
        assert!(fields.len() > 120, "only {} fields", fields.len());
        // Every field the view publishes must be one `apply` accepts with the
        // value the view shows for it, and applying that value is a no-op.
        for field in &fields {
            match (&field.kind, &field.value) {
                (
                    EditorFieldKind::Number {
                        minimum, maximum, ..
                    },
                    EditorValue::Integer(v),
                ) => {
                    assert!(
                        (minimum..=maximum).contains(&v),
                        "{} shows {v} outside its range",
                        field.id
                    );
                }
                (EditorFieldKind::Choice { options }, EditorValue::Choice(v)) => {
                    assert!(
                        options.iter().any(|o| o.value == *v),
                        "{} shows an option it lacks",
                        field.id
                    );
                    for option in options {
                        assert!(valid_id(&option.value) && valid_text(&option.label));
                        if let Some(detail) = &option.detail {
                            assert!(valid_text(detail), "{}: {detail}", field.id);
                        }
                    }
                }
                (EditorFieldKind::Toggle, EditorValue::Boolean(_)) => {}
                _ => panic!("{} has a kind and value that do not match", field.id),
            }
            let same = apply(&voice, &field.id, &field.value)
                .unwrap_or_else(|| panic!("{} does not apply", field.id));
            assert_eq!(same, voice, "{} changed the voice on a no-op", field.id);
        }
    }

    #[test]
    fn an_edit_changes_exactly_what_it_names() {
        let voice = factory_voice(0);
        let edited = apply(&voice, "op2.out", &EditorValue::Integer(50)).unwrap();
        assert_eq!(edited.operators[1].output_level, 50);
        let mut expected = voice;
        expected.operators[1].output_level = 50;
        assert_eq!(edited, expected);

        let edited = apply(&voice, "algorithm", &EditorValue::Choice("32".into())).unwrap();
        assert_eq!(edited.algorithm, 31);
        let edited = apply(&voice, "transpose", &EditorValue::Integer(-12)).unwrap();
        assert_eq!(edited.transpose, 12);
        let edited = apply(&voice, "op6.detune", &EditorValue::Integer(7)).unwrap();
        assert_eq!(edited.operators[5].detune, 14);
        let edited = apply(&voice, "op1.lc", &EditorValue::Choice("pos-exp".into())).unwrap();
        assert_eq!(edited.operators[0].left_curve, 2);
        let edited = apply(&voice, "lfo.wave", &EditorValue::Choice("sine".into())).unwrap();
        assert_eq!(edited.lfo.waveform, 4);
        let edited = apply(&voice, "peg.l1", &EditorValue::Integer(60)).unwrap();
        assert_eq!(edited.pitch_eg_level[0], 60);
        let edited = apply(&voice, "op3.eg.r4", &EditorValue::Integer(12)).unwrap();
        assert_eq!(edited.operators[2].eg_rate[3], 12);
    }

    #[test]
    fn a_value_out_of_range_or_a_field_that_does_not_exist_is_refused() {
        let voice = factory_voice(0);
        assert_eq!(apply(&voice, "op2.out", &EditorValue::Integer(100)), None);
        assert_eq!(apply(&voice, "op2.out", &EditorValue::Boolean(true)), None);
        assert_eq!(apply(&voice, "op7.out", &EditorValue::Integer(1)), None);
        assert_eq!(apply(&voice, "op0.out", &EditorValue::Integer(1)), None);
        assert_eq!(
            apply(&voice, "algorithm", &EditorValue::Choice("33".into())),
            None
        );
        assert_eq!(
            apply(&voice, "op1.lc", &EditorValue::Choice("wavy".into())),
            None
        );
        assert_eq!(apply(&voice, "peg.r5", &EditorValue::Integer(1)), None);
        assert_eq!(apply(&voice, "nothing", &EditorValue::Integer(1)), None);
        assert_eq!(apply(&voice, "op1.eg.x1", &EditorValue::Integer(1)), None);
    }

    #[test]
    fn the_algorithm_summaries_read_as_routing() {
        assert_eq!(algorithm_summary(0), "out 1,3; 2>1 4>3 5>4 6>5");
        assert_eq!(algorithm_summary(31), "out 1,2,3,4,5,6");
        assert_eq!(algorithm_summary(4), "out 1,3,5; 2>1 4>3 6>5");
    }

    #[test]
    fn the_view_fits_the_transfer_buffer_with_room() {
        let json = serde_json::to_string(&view(&factory_voice(0))).unwrap();
        assert!(
            json.len() < crate::TRANSFER_BYTES * 3 / 4,
            "editor view is {} bytes",
            json.len()
        );
    }
}
