//! What the surface knows, read out of the host's messages.
//!
//! Everything here is forgiving on the way in — a context missing a field
//! becomes an empty list, not a crash — and strict on the way out: the state
//! a renderer sees is fully typed.

use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sound {
    pub id: String,
    pub name: String,
    pub bank: String,
    pub editable: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bank {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Instance {
    pub plugin_id: String,
    pub plugin_name: String,
    pub banks: Vec<Bank>,
    pub sounds: Vec<Sound>,
    pub selected: Option<String>,
}

/// A value of an editor field, as the host types it.
#[derive(Clone, Debug, PartialEq)]
pub enum FieldValue {
    Boolean(bool),
    Integer(i64),
    Choice(String),
}

impl FieldValue {
    pub fn from_json(value: &Value) -> Option<Self> {
        match value["type"].as_str()? {
            "boolean" => value["value"].as_bool().map(Self::Boolean),
            "integer" => value["value"].as_i64().map(Self::Integer),
            "choice" => value["value"].as_str().map(|s| Self::Choice(s.to_owned())),
            _ => None,
        }
    }

    pub fn to_json(&self) -> Value {
        match self {
            Self::Boolean(b) => serde_json::json!({"type": "boolean", "value": b}),
            Self::Integer(i) => serde_json::json!({"type": "integer", "value": i}),
            Self::Choice(c) => serde_json::json!({"type": "choice", "value": c}),
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Integer(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_choice(&self) -> Option<&str> {
        match self {
            Self::Choice(c) => Some(c),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub value: String,
    pub label: String,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FieldKind {
    Toggle,
    Number {
        minimum: i64,
        maximum: i64,
        unit: Option<String>,
    },
    Choice {
        options: Vec<Choice>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub id: String,
    pub label: String,
    pub detail: String,
    pub value: FieldValue,
    pub kind: FieldKind,
}

/// The program under edit: the host's draft, flattened to fields by id. The
/// page tree is kept only to render fields the surface does not know a place
/// for, so a newer plugin's editor still shows every control.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Draft {
    pub draft_id: u64,
    pub name: String,
    pub dirty: bool,
    pub original_program_id: Option<String>,
    pub fields: BTreeMap<String, Field>,
    /// Field ids in the order the plugin published them.
    pub order: Vec<String>,
}

impl Draft {
    pub fn field(&self, id: &str) -> Option<&Field> {
        self.fields.get(id)
    }

    pub fn integer(&self, id: &str) -> Option<i64> {
        self.fields.get(id).and_then(|f| f.value.as_i64())
    }

    pub fn boolean(&self, id: &str) -> Option<bool> {
        self.fields.get(id).and_then(|f| f.value.as_bool())
    }

    pub fn choice(&self, id: &str) -> Option<&str> {
        self.fields.get(id).and_then(|f| f.value.as_choice())
    }
}

/// One of the plugin's public parameters, from `plugin.parameters`.
#[derive(Clone, Debug, PartialEq)]
pub struct Parameter {
    pub index: usize,
    pub id: String,
    pub name: String,
    pub page: String,
    pub kind: ParameterKind,
    pub value: f64,
    /// Where the schema puts it, which is where a double-click returns it.
    pub default: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ParameterKind {
    Float {
        minimum: f64,
        maximum: f64,
        step: f64,
        unit: Option<String>,
        logarithmic: bool,
    },
    Integer {
        minimum: i64,
        maximum: i64,
        unit: Option<String>,
    },
    Boolean,
    Enum {
        choices: Vec<(i64, String)>,
    },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Parameters {
    pub pages: Vec<(String, String)>,
    pub parameters: Vec<Parameter>,
}

/// A file the host has granted this plugin before, and can grant again
/// without opening its explorer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Grant {
    pub id: String,
    pub name: String,
    pub resource: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    pub connected: bool,
    /// Which of the plugin's two surfaces this is: `play` or `config`.
    pub surface: String,
    /// One entry per declared resource: whether a file is installed in it.
    pub resources: Vec<(String, bool)>,
    pub grants: Vec<Grant>,
    /// What the config surface is waiting for, if anything.
    pub busy: String,
    /// The last thing that happened, for the player to read.
    pub notice: String,
    pub lighting: String,
    pub status: String,
    /// Which page of the panel is showing. The surface owns this; the host
    /// has no opinion about it.
    pub page: String,
    pub instance: Option<Instance>,
    pub draft: Option<Draft>,
    pub parameters: Option<Parameters>,
    /// Field edits the host has not echoed back yet, shown in place of the
    /// draft's value so a slider does not jump while its request is in flight.
    pub pending_fields: BTreeMap<String, (FieldValue, bool)>,
    pub pending_parameters: BTreeMap<usize, (f64, bool)>,
    /// Every field as it was when the draft opened, so one knob can be sent
    /// back to where it started without leaving the edit.
    pub baseline: BTreeMap<String, FieldValue>,
    /// COMPARE is down: the program as it opened is what plays and what the
    /// knobs show, and nothing can be edited until it is released.
    pub comparing: bool,
    /// The grant SETUP installed last, so the cartridge can be named. The
    /// host reports only that something is installed.
    pub installed_grant: Option<String>,
}

impl State {
    /// Read a `context` message. Returns false if it is not for this plugin.
    pub fn apply_context(&mut self, message: &Value, plugin_id: &str) -> bool {
        if message["instance"]["plugin_id"].as_str() != Some(plugin_id) {
            return false;
        }
        self.connected = true;
        if let Some(surface) = message["surface"].as_str() {
            self.surface = surface.to_owned();
        }
        if let Some(lighting @ ("day" | "stage")) = message["host"]["lighting"].as_str() {
            self.lighting = lighting.to_owned();
        }
        self.instance = Some(read_instance(&message["instance"]));
        let had_draft = self.draft.is_some();
        let previous_id = self.draft.as_ref().map(|d| d.draft_id);
        self.draft = read_draft(&message["program_draft"]);
        match &self.draft {
            Some(draft) if previous_id != Some(draft.draft_id) => {
                self.baseline = draft
                    .fields
                    .iter()
                    .map(|(id, field)| (id.clone(), field.value.clone()))
                    .collect();
            }
            None => self.baseline.clear(),
            _ => {}
        }
        if self.draft.is_none() || previous_id != self.draft.as_ref().map(|d| d.draft_id) {
            self.comparing = false;
        }
        // Opening a program puts the panel on the voice page; closing one
        // sends it back to the library rather than leaving an empty plate.
        match (had_draft, self.draft.is_some()) {
            (false, true) => self.page = "voice".into(),
            (true, false) if self.page == "voice" || self.page == "operators" => {
                self.page = "programs".into()
            }
            _ => {}
        }
        // Anything the host has answered is now visible in its own context.
        self.pending_fields.retain(|_, (_, answered)| !*answered);
        self.pending_parameters
            .retain(|_, (_, answered)| !*answered);
        true
    }

    pub fn apply_parameters(&mut self, result: &Value) -> bool {
        match read_parameters(result) {
            Some(parameters) => {
                self.parameters = Some(parameters);
                true
            }
            None => false,
        }
    }

    /// A `parameter_changed` push from the host: one value, already
    /// canonical.
    pub fn apply_parameter_change(&mut self, message: &Value) {
        let (Some(index), Some(value)) = (
            message["parameter_index"].as_u64(),
            message["value"].as_f64(),
        ) else {
            return;
        };
        if let Some(parameters) = &mut self.parameters
            && let Some(parameter) = parameters
                .parameters
                .iter_mut()
                .find(|p| p.index == index as usize)
        {
            parameter.value = value;
        }
        self.pending_parameters.remove(&(index as usize));
    }

    pub fn selected_sound(&self) -> Option<&Sound> {
        let instance = self.instance.as_ref()?;
        let id = instance.selected.as_deref()?;
        instance.sounds.iter().find(|s| s.id == id)
    }

    /// The value a field shows: the program as it opened while COMPARE is
    /// down, else the edit in flight, else the draft's.
    pub fn field_value(&self, id: &str) -> Option<FieldValue> {
        if self.comparing
            && let Some(value) = self.baseline.get(id)
        {
            return Some(value.clone());
        }
        if let Some((value, _)) = self.pending_fields.get(id) {
            return Some(value.clone());
        }
        self.draft.as_ref()?.fields.get(id).map(|f| f.value.clone())
    }

    /// The fields the edit has moved, with the value each had when the draft
    /// opened: what COMPARE plays.
    pub fn moved_fields(&self) -> Vec<(String, FieldValue)> {
        let Some(draft) = &self.draft else {
            return Vec::new();
        };
        self.baseline
            .iter()
            .filter(|(id, opened)| draft.fields.get(*id).is_some_and(|f| f.value != **opened))
            .map(|(id, opened)| (id.clone(), opened.clone()))
            .collect()
    }

    /// The name SETUP can give the installed cartridge: the grant it
    /// installed last, if the host still lists it.
    pub fn installed_cartridge_name(&self) -> Option<&str> {
        let id = self.installed_grant.as_deref()?;
        self.grants
            .iter()
            .find(|grant| grant.id == id)
            .map(|grant| grant.name.as_str())
    }

    pub fn parameter_value(&self, index: usize) -> Option<f64> {
        if let Some((value, _)) = self.pending_parameters.get(&index) {
            return Some(*value);
        }
        self.parameters
            .as_ref()?
            .parameters
            .iter()
            .find(|p| p.index == index)
            .map(|p| p.value)
    }

    /// Whether a file is installed in one of the plugin's resources.
    pub fn resource_installed(&self, id: &str) -> bool {
        self.resources
            .iter()
            .any(|(resource, installed)| resource == id && *installed)
    }

    /// Read `plugin.resource_status`: one entry per declared resource.
    pub fn apply_resources(&mut self, result: &Value) -> bool {
        let Some(entries) = result.as_array() else {
            return false;
        };
        self.resources = entries
            .iter()
            .filter_map(|entry| {
                Some((
                    entry["resource_id"].as_str()?.to_owned(),
                    entry["installed"].as_bool().unwrap_or(false),
                ))
            })
            .collect();
        true
    }

    /// Read `plugin.resource_bindings`: the files granted before.
    pub fn apply_grants(&mut self, result: &Value) -> bool {
        let Some(entries) = result.as_array() else {
            return false;
        };
        self.grants = entries
            .iter()
            .filter_map(|entry| {
                Some(Grant {
                    id: entry["grant_id"].as_str()?.to_owned(),
                    name: string(&entry["display_name"]),
                    resource: string(&entry["resource_id"]),
                })
            })
            .collect();
        true
    }

    pub fn disconnect(&mut self, status: &str) {
        self.connected = false;
        self.status = status.to_owned();
        self.pending_fields.clear();
        self.pending_parameters.clear();
    }
}

fn string(value: &Value) -> String {
    value.as_str().unwrap_or_default().to_owned()
}

fn read_instance(value: &Value) -> Instance {
    Instance {
        plugin_id: string(&value["plugin_id"]),
        plugin_name: string(&value["plugin_name"]),
        banks: value["banks"]
            .as_array()
            .map(|banks| {
                banks
                    .iter()
                    .map(|bank| Bank {
                        id: string(&bank["id"]),
                        name: string(&bank["name"]),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        sounds: value["sounds"]
            .as_array()
            .map(|sounds| {
                sounds
                    .iter()
                    .map(|sound| Sound {
                        id: string(&sound["id"]),
                        name: string(&sound["name"]),
                        bank: string(&sound["bank"]),
                        editable: sound["editable"].as_bool().unwrap_or(false),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        selected: value["selected_sound_id"].as_str().map(str::to_owned),
    }
}

fn read_draft(value: &Value) -> Option<Draft> {
    let draft_id = value["draft_id"].as_u64()?;
    let mut draft = Draft {
        draft_id,
        name: string(&value["name"]),
        dirty: value["dirty"].as_bool().unwrap_or(false),
        original_program_id: value["original_program_id"].as_str().map(str::to_owned),
        fields: BTreeMap::new(),
        order: Vec::new(),
    };
    if let Some(pages) = value["editor"]["pages"].as_array() {
        for page in pages {
            read_page(page, &mut draft);
        }
    }
    Some(draft)
}

fn read_page(page: &Value, draft: &mut Draft) {
    if let Some(fields) = page["fields"].as_array() {
        for field in fields {
            if let Some(field) = read_field(field) {
                draft.order.push(field.id.clone());
                draft.fields.insert(field.id.clone(), field);
            }
        }
    }
    if let Some(pages) = page["pages"].as_array() {
        for child in pages {
            read_page(child, draft);
        }
    }
}

fn read_field(value: &Value) -> Option<Field> {
    let id = value["id"].as_str()?.to_owned();
    let field_value = FieldValue::from_json(&value["value"])?;
    let kind = match value["kind"]["type"].as_str()? {
        "toggle" => FieldKind::Toggle,
        "number" => FieldKind::Number {
            minimum: value["kind"]["minimum"].as_i64()?,
            maximum: value["kind"]["maximum"].as_i64()?,
            unit: value["kind"]["unit"].as_str().map(str::to_owned),
        },
        "choice" => FieldKind::Choice {
            options: value["kind"]["options"]
                .as_array()?
                .iter()
                .map(|option| Choice {
                    value: string(&option["value"]),
                    label: string(&option["label"]),
                    detail: option["detail"].as_str().map(str::to_owned),
                })
                .collect(),
        },
        _ => return None,
    };
    Some(Field {
        id,
        label: string(&value["label"]),
        detail: string(&value["detail"]),
        value: field_value,
        kind,
    })
}

fn read_parameters(result: &Value) -> Option<Parameters> {
    let schema = &result["schema"];
    let pages = schema["pages"]
        .as_array()
        .map(|pages| {
            pages
                .iter()
                .map(|page| (string(&page["id"]), string(&page["name"])))
                .collect()
        })
        .unwrap_or_default();
    let values: BTreeMap<usize, f64> = result["values"]
        .as_array()?
        .iter()
        .filter_map(|entry| {
            Some((
                usize::try_from(entry["index"].as_u64()?).ok()?,
                entry["value"].as_f64().filter(|v| v.is_finite())?,
            ))
        })
        .collect();
    let mut parameters = Vec::new();
    for parameter in schema["parameters"].as_array()? {
        let index = usize::try_from(parameter["index"].as_u64()?).ok()?;
        let kind = &parameter["kind"];
        let kind = match kind["type"].as_str()? {
            "float" => ParameterKind::Float {
                minimum: kind["minimum"].as_f64()?,
                maximum: kind["maximum"].as_f64()?,
                step: kind["step"].as_f64().unwrap_or(0.01),
                unit: kind["unit"].as_str().map(str::to_owned),
                logarithmic: kind["taper"].as_str() == Some("logarithmic"),
            },
            "integer" => ParameterKind::Integer {
                minimum: kind["minimum"].as_i64()?,
                maximum: kind["maximum"].as_i64()?,
                unit: kind["unit"].as_str().map(str::to_owned),
            },
            "boolean" => ParameterKind::Boolean,
            "enum" => ParameterKind::Enum {
                choices: kind["choices"]
                    .as_array()?
                    .iter()
                    .filter_map(|choice| Some((choice["value"].as_i64()?, string(&choice["name"]))))
                    .collect(),
            },
            _ => continue,
        };
        let default = match &kind {
            ParameterKind::Boolean => f64::from(u8::from(
                parameter["kind"]["default"].as_bool().unwrap_or(false),
            )),
            _ => parameter["kind"]["default"].as_f64().unwrap_or(0.0),
        };
        parameters.push(Parameter {
            index,
            id: string(&parameter["id"]),
            name: string(&parameter["name"]),
            page: string(&parameter["page"]),
            kind,
            value: *values.get(&index)?,
            default,
        });
    }
    parameters.sort_by_key(|p| p.index);
    Some(Parameters { pages, parameters })
}

/// Coarse, fine and detune as the panel writes them: a ratio, or a fixed
/// frequency.
pub fn frequency_text(fixed: bool, coarse: i64, fine: i64, detune: i64) -> String {
    let detune = if detune == 0 {
        String::new()
    } else {
        format!(" {detune:+}")
    };
    if fixed {
        let decade = 10f64.powi((coarse & 3) as i32);
        let hertz = decade * 10f64.powf(fine as f64 / 100.0);
        if hertz >= 100.0 {
            format!("{hertz:.0} Hz{detune}")
        } else {
            format!("{hertz:.2} Hz{detune}")
        }
    } else {
        let base = if coarse == 0 { 0.5 } else { coarse as f64 };
        let ratio = base * (1.0 + fine as f64 / 100.0);
        format!("×{ratio:.2}{detune}")
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;

    pub(crate) fn context() -> Value {
        json!({
            "protocol": "rackforge.plugin.web@1", "kind": "context", "surface": "play",
            "instance": {
                "instance_id": "i1", "plugin_id": "org.rackforge.rf7", "plugin_name": "RF-7",
                "banks": [{"id": "bank-1", "name": "Bank 1", "order": 0}, {"id": "bank-user", "name": "Your programs", "order": 1}],
                "sounds": [
                    {"id": "program-001", "name": "RF TINES", "bank": "bank-1", "editable": true},
                    {"id": "program-002", "name": "RF EP SOFT", "bank": "bank-1", "editable": true},
                    {"id": "custom.user.rf7-001", "name": "Quiet tines", "bank": "bank-user", "editable": true}
                ],
                "selected_sound_id": "program-002"
            },
            "program_draft": {
                "draft_id": 7, "instance_id": "i1", "name": "Quiet tines", "dirty": true,
                "preview_sound_id": "custom.user.rf7-001", "storage_path": "programs/x.json", "document_json": "{}",
                "original_program_id": "custom.user.rf7-001",
                "editor": {"schema_version": 1, "title": "Quiet tines", "pages": [
                    {"id": "voice", "label": "Voice", "detail": "", "enabled": true, "fields": [
                        {"id": "algorithm", "label": "Algorithm", "detail": "", "live_preview": true,
                         "value": {"type": "choice", "value": "5"},
                         "kind": {"type": "choice", "options": [{"value": "5", "label": "Algorithm 5", "detail": "out 1,3,5; 2>1 4>3 6>5"}]}},
                        {"id": "feedback", "label": "Feedback", "detail": "loop", "live_preview": true,
                         "value": {"type": "integer", "value": 6},
                         "kind": {"type": "number", "minimum": 0, "maximum": 7, "step": 1, "decimals": 0, "allow_inherited": false}},
                        {"id": "sync", "label": "Oscillator sync", "detail": "", "live_preview": true,
                         "value": {"type": "boolean", "value": true}, "kind": {"type": "toggle"}}
                    ]},
                    {"id": "operators", "label": "Operators", "detail": "", "enabled": true, "pages": [
                        {"id": "op1", "label": "Operator 1", "detail": "carrier", "enabled": true, "pages": [
                            {"id": "op1.freq", "label": "Frequency", "detail": "", "enabled": true, "fields": [
                                {"id": "op1.coarse", "label": "Coarse", "detail": "", "live_preview": true,
                                 "value": {"type": "integer", "value": 1},
                                 "kind": {"type": "number", "minimum": 0, "maximum": 31, "step": 1, "decimals": 0, "allow_inherited": false}}
                            ]}
                        ]}
                    ]}
                ]}
            },
            "audition": null,
            "host": {"active_mode": "play", "master_level": 800, "master_pan": 0, "lighting": "stage"}
        })
    }

    #[test]
    fn a_context_becomes_typed_state() {
        let mut state = State::default();
        assert!(state.apply_context(&context(), "org.rackforge.rf7"));
        assert!(state.connected);
        assert_eq!(state.lighting, "stage");
        let instance = state.instance.as_ref().unwrap();
        assert_eq!(instance.sounds.len(), 3);
        assert_eq!(state.selected_sound().unwrap().name, "RF EP SOFT");
        let draft = state.draft.as_ref().unwrap();
        assert_eq!(draft.draft_id, 7);
        assert!(draft.dirty);
        assert_eq!(draft.choice("algorithm"), Some("5"));
        assert_eq!(draft.integer("feedback"), Some(6));
        assert_eq!(draft.boolean("sync"), Some(true));
        assert_eq!(draft.integer("op1.coarse"), Some(1));
        assert_eq!(draft.order, ["algorithm", "feedback", "sync", "op1.coarse"]);
        assert!(!state.apply_context(&context(), "org.rackforge.other"));
    }

    #[test]
    fn the_panel_follows_the_draft_between_the_library_and_the_voice() {
        let mut state = State::default();
        assert_eq!(state.page, "");
        state.apply_context(&context(), "org.rackforge.rf7");
        assert_eq!(state.page, "voice", "a draft opens the voice page");
        state.page = "operators".into();
        state.apply_context(&context(), "org.rackforge.rf7");
        assert_eq!(state.page, "operators", "and stays where the player put it");
        let mut without = context();
        without["program_draft"] = Value::Null;
        state.apply_context(&without, "org.rackforge.rf7");
        assert_eq!(state.page, "programs", "closing it returns to the library");
        state.page = "perform".into();
        state.apply_context(&without, "org.rackforge.rf7");
        assert_eq!(
            state.page, "perform",
            "a page that still has content is kept"
        );
    }

    #[test]
    fn the_draft_as_it_opened_is_kept_until_it_closes() {
        let mut state = State::default();
        state.apply_context(&context(), "org.rackforge.rf7");
        assert_eq!(
            state.baseline.get("feedback"),
            Some(&FieldValue::Integer(6))
        );
        // The host echoes an edit: the baseline does not move with it.
        let mut edited = context();
        edited["program_draft"]["editor"]["pages"][0]["fields"][1]["value"] =
            json!({"type": "integer", "value": 2});
        state.apply_context(&edited, "org.rackforge.rf7");
        assert_eq!(state.draft.as_ref().unwrap().integer("feedback"), Some(2));
        assert_eq!(
            state.baseline.get("feedback"),
            Some(&FieldValue::Integer(6))
        );
        // A new draft is a new baseline; no draft is none.
        edited["program_draft"]["draft_id"] = json!(8);
        state.apply_context(&edited, "org.rackforge.rf7");
        assert_eq!(
            state.baseline.get("feedback"),
            Some(&FieldValue::Integer(2))
        );
        edited["program_draft"] = Value::Null;
        state.apply_context(&edited, "org.rackforge.rf7");
        assert!(state.baseline.is_empty());
    }

    #[test]
    fn compare_shows_and_names_only_what_the_edit_has_moved() {
        let mut state = State::default();
        state.apply_context(&context(), "org.rackforge.rf7");
        assert!(state.moved_fields().is_empty(), "nothing moved yet");
        let mut edited = context();
        edited["program_draft"]["editor"]["pages"][0]["fields"][1]["value"] =
            json!({"type": "integer", "value": 2});
        state.apply_context(&edited, "org.rackforge.rf7");
        assert_eq!(
            state.moved_fields(),
            vec![("feedback".to_owned(), FieldValue::Integer(6))]
        );
        state.comparing = true;
        assert_eq!(state.field_value("feedback"), Some(FieldValue::Integer(6)));
        assert_eq!(
            state.field_value("op1.coarse"),
            Some(FieldValue::Integer(1))
        );
        // The same draft echoed again keeps comparing; another draft, or
        // none, releases the key.
        state.apply_context(&edited, "org.rackforge.rf7");
        assert!(state.comparing);
        edited["program_draft"] = Value::Null;
        state.apply_context(&edited, "org.rackforge.rf7");
        assert!(!state.comparing);
    }

    #[test]
    fn a_pending_edit_shows_until_the_host_has_answered_and_echoed_it() {
        let mut state = State::default();
        state.apply_context(&context(), "org.rackforge.rf7");
        state
            .pending_fields
            .insert("feedback".into(), (FieldValue::Integer(3), false));
        assert_eq!(state.field_value("feedback"), Some(FieldValue::Integer(3)));
        state.apply_context(&context(), "org.rackforge.rf7");
        assert_eq!(
            state.field_value("feedback"),
            Some(FieldValue::Integer(3)),
            "not answered yet"
        );
        state.pending_fields.get_mut("feedback").unwrap().1 = true;
        state.apply_context(&context(), "org.rackforge.rf7");
        assert_eq!(state.field_value("feedback"), Some(FieldValue::Integer(6)));
    }

    #[test]
    fn the_config_surface_reads_what_the_host_has_installed_and_granted() {
        let mut state = State::default();
        let mut context = context();
        context["surface"] = json!("config");
        assert!(state.apply_context(&context, "org.rackforge.rf7"));
        assert_eq!(state.surface, "config");

        assert!(state.apply_resources(&json!([
            {"resource_id": "cartridge", "installed": true}
        ])));
        assert!(state.resource_installed("cartridge"));
        assert!(!state.resource_installed("something-else"));
        assert!(state.apply_resources(&json!([
            {"resource_id": "cartridge", "installed": false}
        ])));
        assert!(!state.resource_installed("cartridge"));
        assert!(!state.apply_resources(&json!({"resource_id": "cartridge"})));

        assert!(state.apply_grants(&json!([
            {"grant_id": "g1", "resource_id": "cartridge", "display_name": "ROM1A.syx", "kind": "file"},
            {"missing": "a grant id"}
        ])));
        assert_eq!(
            state.grants.len(),
            1,
            "an entry without an id is not a grant"
        );
        assert_eq!(state.grants[0].name, "ROM1A.syx");
    }

    #[test]
    fn the_installed_cartridge_is_named_only_while_the_host_still_lists_it() {
        let mut state = State {
            grants: vec![Grant {
                id: "g1".into(),
                name: "ROM1A.syx".into(),
                resource: "cartridge".into(),
            }],
            ..State::default()
        };
        assert_eq!(state.installed_cartridge_name(), None);
        state.installed_grant = Some("g1".into());
        assert_eq!(state.installed_cartridge_name(), Some("ROM1A.syx"));
        state.grants.clear();
        assert_eq!(state.installed_cartridge_name(), None);
    }

    #[test]
    fn parameters_are_read_with_their_schema() {
        let result = json!({
            "schema": {"pages": [{"id": "output", "name": "Output"}], "parameters": [
                {"index": 0, "id": "gain", "name": "Output Gain", "page": "output",
                 "kind": {"type": "float", "minimum": 0.0, "maximum": 2.0, "default": 0.3, "step": 0.01, "unit": "x"}},
                {"index": 5, "id": "wheel_target", "name": "Mod Wheel Target", "page": "performance",
                 "kind": {"type": "enum", "default": 0, "choices": [{"value": 0, "name": "Pitch"}, {"value": 1, "name": "Amplitude"}]}},
                {"index": 11, "id": "operator_1", "name": "Operator 1", "page": "operators", "kind": {"type": "boolean", "default": true}}
            ]},
            "values": [{"index": 0, "value": 0.3}, {"index": 5, "value": 1.0}, {"index": 11, "value": 1.0}]
        });
        let mut state = State::default();
        assert!(state.apply_parameters(&result));
        assert_eq!(state.parameter_value(0), Some(0.3));
        assert_eq!(state.parameter_value(5), Some(1.0));
        let parameters = state.parameters.as_ref().unwrap();
        assert_eq!(parameters.parameters.len(), 3);
        assert!(matches!(
            parameters.parameters[1].kind,
            ParameterKind::Enum { .. }
        ));
        assert_eq!(parameters.parameters[0].default, 0.3);
        assert_eq!(
            parameters.parameters[2].default, 1.0,
            "a boolean default reads as 1"
        );
        state.apply_parameter_change(&json!({"parameter_index": 0, "value": 0.5}));
        assert_eq!(state.parameter_value(0), Some(0.5));
        // A value missing from the snapshot is a snapshot refused.
        let mut broken = result.clone();
        broken["values"] = json!([{"index": 0, "value": 0.3}]);
        assert!(!state.apply_parameters(&broken));
    }

    #[test]
    fn frequencies_read_as_the_panel_writes_them() {
        assert_eq!(frequency_text(false, 1, 0, 0), "×1.00");
        assert_eq!(frequency_text(false, 0, 0, 0), "×0.50");
        assert_eq!(frequency_text(false, 3, 50, 3), "×4.50 +3");
        assert_eq!(frequency_text(true, 0, 0, 0), "1.00 Hz");
        assert_eq!(frequency_text(true, 2, 0, -2), "100 Hz -2");
        assert_eq!(frequency_text(true, 3, 50, 0), "3162 Hz");
    }
}
