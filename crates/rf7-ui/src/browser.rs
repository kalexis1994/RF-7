//! The browser layer: DOM in, DOM out, and the host on the other side of
//! `postMessage`. Decisions live in the sibling modules; this one wires them.

use crate::client::{self, Client};
use crate::model::{FieldValue, State};
use crate::{PLUGIN_ID, PROTOCOL, render};
use js_sys::{JSON, Object};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::{JsCast, prelude::*};
use web_sys::{
    Document, Element, Event, HtmlInputElement, HtmlSelectElement, MessageEvent, PointerEvent,
    Window,
};

const SECTIONS: [&str; 3] = ["header", "tabs", "page"];
const POLL_MS: f64 = 3000.0;
/// How far a pointer travels for a knob's whole range, in pixels. The other
/// RackForge instruments use the same throw.
const KNOB_THROW: f64 = 180.0;

/// A knob under the pointer: the pointer, its input, where it started.
struct Drag {
    pointer: i32,
    input: Element,
    start_y: f64,
    start_value: f64,
}

struct App {
    window: Window,
    document: Document,
    origin: String,
    state: State,
    client: Client,
    rendered: [String; 3],
    last_poll: f64,
    info: Option<String>,
    drag: Option<Drag>,
}
type Shared = Rc<RefCell<App>>;

impl App {
    fn now(&self) -> f64 {
        self.window.performance().map(|p| p.now()).unwrap_or(0.0)
    }

    fn send(&self, message: &Value) -> Result<(), JsValue> {
        let parent = self
            .window
            .parent()?
            .ok_or_else(|| JsValue::from_str("missing host"))?;
        let data = JSON::parse(&message.to_string())?;
        parent.post_message(&data, &self.origin)
    }

    /// Send whatever may go, then draw.
    fn pump(&mut self) {
        let now = self.now();
        if self.client.timed_out(now) {
            self.state
                .disconnect("The host stopped answering. Waiting for it...");
        }
        if self.state.connected {
            if self.client.is_idle() && now - self.last_poll > POLL_MS {
                self.last_poll = now;
                self.client.queue(client::fetch_parameters());
            }
            let info = render::surface_info(&self.state);
            if info != self.info {
                self.info = info.clone();
                self.client
                    .queue(client::surface_info("RF-7", info.as_deref()));
            }
            if let Some(request) = self.client.next(now)
                && self.send(&request).is_err()
            {
                self.client.clear();
                self.state
                    .disconnect("Could not reach RackForge. Waiting for it...");
            }
        }
        self.render();
    }

    fn render(&mut self) {
        let html = [
            render::header(&self.state),
            render::tabs(&self.state),
            render::page(&self.state),
        ];
        let active = self.document.active_element();
        for (index, name) in SECTIONS.iter().enumerate() {
            if self.rendered[index] == html[index] {
                continue;
            }
            let Some(section) = self.document.get_element_by_id(name) else {
                continue;
            };
            // A control mid-edit — a knob under the pointer, an open list, a
            // name being typed — keeps its section; the values around it are
            // refreshed in place instead.
            let busy = self.drag.is_some()
                || active.as_ref().is_some_and(|el| {
                    matches!(el.tag_name().as_str(), "INPUT" | "SELECT")
                        && el.closest(&format!("#{name}")).ok().flatten().is_some()
                });
            if busy {
                self.sync_values(&section, active.as_ref());
                continue;
            }
            section.set_inner_html(&html[index]);
            self.rendered[index] = html[index].clone();
        }
        if let Some(root) = self.document.document_element() {
            let _ = root.set_attribute(
                "data-editing",
                if self.state.draft.is_some() {
                    "true"
                } else {
                    "false"
                },
            );
        }
    }

    /// Update every control in a section from state, except the one in use.
    fn sync_values(&self, section: &Element, active: Option<&Element>) {
        let Ok(controls) = section.query_selector_all("[data-field],[data-param]") else {
            return;
        };
        for index in 0..controls.length() {
            let Some(node) = controls.get(index) else {
                continue;
            };
            let Ok(element) = node.dyn_into::<Element>() else {
                continue;
            };
            if active.is_some_and(|a| a.is_same_node(Some(&element))) {
                continue;
            }
            let value = if let Some(id) = element.get_attribute("data-field") {
                self.state.field_value(&id).map(|v| match v {
                    FieldValue::Boolean(b) => Value::Bool(b),
                    FieldValue::Integer(i) => json!(i),
                    FieldValue::Choice(c) => Value::String(c),
                })
            } else {
                element
                    .get_attribute("data-param")
                    .and_then(|p| p.parse::<usize>().ok())
                    .and_then(|p| self.state.parameter_value(p))
                    .map(|v| json!(v))
            };
            let Some(value) = value else {
                continue;
            };
            if let Some(wanted) = element.get_attribute("data-value") {
                // One key of a segmented control.
                let current = match &value {
                    Value::String(text) => text.clone(),
                    other => format!("{}", other.as_f64().unwrap_or(0.0).round() as i64),
                };
                let _ = element.set_attribute("aria-pressed", &(wanted == current).to_string());
            } else if element.has_attribute("data-toggle") {
                let on = value
                    .as_bool()
                    .unwrap_or(value.as_f64().unwrap_or(0.0) >= 0.5);
                let _ = element.set_attribute("aria-checked", if on { "true" } else { "false" });
            } else if let Some(select) = element.dyn_ref::<HtmlSelectElement>() {
                let text = match &value {
                    Value::String(s) => s.clone(),
                    other => format!("{}", other.as_f64().unwrap_or(0.0).round() as i64),
                };
                select.set_value(&text);
            } else if let Some(input) = element.dyn_ref::<HtmlInputElement>()
                && let Some(number) = value.as_f64()
            {
                input.set_value_as_number(number);
                show_value(&element, number);
            }
        }
    }

    fn field_from_element(&self, element: &Element, id: &str) -> Option<FieldValue> {
        if element.has_attribute("data-toggle") {
            let current = self
                .state
                .field_value(id)
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            return Some(FieldValue::Boolean(!current));
        }
        if let Some(select) = element.dyn_ref::<HtmlSelectElement>() {
            return Some(FieldValue::Choice(select.value()));
        }
        let input = element.dyn_ref::<HtmlInputElement>()?;
        let number = input.value_as_number();
        number
            .is_finite()
            .then(|| FieldValue::Integer(number.round() as i64))
    }

    fn edit_field(&mut self, id: &str, value: FieldValue, preview: bool) {
        let Some(draft_id) = self.state.draft.as_ref().map(|d| d.draft_id) else {
            return;
        };
        self.state
            .pending_fields
            .insert(id.to_owned(), (value.clone(), false));
        self.client
            .queue(client::edit_field(draft_id, id, &value.to_json(), preview));
    }

    fn set_parameter(&mut self, index: usize, value: f64) {
        if !value.is_finite() {
            return;
        }
        self.state.pending_parameters.insert(index, (value, false));
        self.client.queue(client::set_parameter(index, value));
    }

    fn parameter_from_element(&self, index: usize, element: &Element) -> Option<f64> {
        if element.has_attribute("data-toggle") {
            let current = self.state.parameter_value(index).unwrap_or(0.0);
            return Some(if current >= 0.5 { 0.0 } else { 1.0 });
        }
        if let Some(select) = element.dyn_ref::<HtmlSelectElement>() {
            return select.value().parse().ok();
        }
        element
            .dyn_ref::<HtmlInputElement>()
            .map(|input| input.value_as_number())
    }

    fn step_algorithm(&mut self, delta: i64) {
        let current = self
            .state
            .field_value("algorithm")
            .and_then(|v| v.as_choice().and_then(|a| a.parse::<i64>().ok()))
            .unwrap_or(1);
        let next = (current + delta).clamp(1, 32);
        if next != current {
            self.edit_field("algorithm", FieldValue::Choice(next.to_string()), false);
        }
    }

    fn action(&mut self, action: &str, element: &Element) {
        let draft_id = self.state.draft.as_ref().map(|d| d.draft_id);
        match (action, draft_id) {
            ("edit", None) => {
                if let Some(sound) = self.state.selected_sound().filter(|s| s.editable) {
                    let id = sound.id.clone();
                    self.client.queue(client::begin_program_edit(Some(&id)));
                }
            }
            ("new", None) => self.client.queue(client::begin_program_edit(None)),
            ("save", Some(id)) => self.client.queue(client::save_program(id)),
            ("cancel", Some(id)) => self.client.queue(client::cancel_program(id)),
            ("name", Some(id)) => {
                if let Some(input) = element.dyn_ref::<HtmlInputElement>() {
                    let name = input.value();
                    let name = name.trim();
                    if !name.is_empty() && name.len() <= 64 && name.is_ascii() {
                        if let Some(draft) = &mut self.state.draft {
                            draft.name = name.to_owned();
                        }
                        self.client.queue(client::set_program_name(id, name));
                    }
                }
            }
            ("alg-prev", Some(_)) => self.step_algorithm(-1),
            ("alg-next", Some(_)) => self.step_algorithm(1),
            _ => {}
        }
    }

    fn handle_event(&mut self, kind: &str, event: &Event) {
        let Some(target) = event.target().and_then(|t| t.dyn_into::<Element>().ok()) else {
            return;
        };
        let Some(element) = target
            .closest("[data-field],[data-param],[data-sound],[data-action],[data-tab]")
            .ok()
            .flatten()
        else {
            return;
        };
        if let Some(tab) = element.get_attribute("data-tab") {
            if kind == "click" {
                self.state.page = tab;
            }
        } else if let Some(id) = element.get_attribute("data-field") {
            match element.get_attribute("data-value") {
                // One key of a segmented control names its own value.
                Some(value) => {
                    if kind == "click" {
                        self.edit_field(&id, FieldValue::Choice(value), false);
                    }
                }
                None => {
                    let is_toggle = element.has_attribute("data-toggle");
                    let is_select = element.dyn_ref::<HtmlSelectElement>().is_some();
                    let fire = match kind {
                        "click" => is_toggle,
                        "input" => !is_toggle && !is_select,
                        "change" => !is_toggle,
                        _ => false,
                    };
                    if fire && let Some(value) = self.field_from_element(&element, &id) {
                        self.edit_field(&id, value, kind == "input");
                    }
                }
            }
        } else if let Some(index) = element
            .get_attribute("data-param")
            .and_then(|p| p.parse::<usize>().ok())
        {
            match element
                .get_attribute("data-value")
                .and_then(|value| value.parse::<f64>().ok())
            {
                Some(value) => {
                    if kind == "click" {
                        self.set_parameter(index, value);
                    }
                }
                None => {
                    let is_toggle = element.has_attribute("data-toggle");
                    let is_select = element.dyn_ref::<HtmlSelectElement>().is_some();
                    let fire = match kind {
                        "click" => is_toggle,
                        "input" => !is_toggle && !is_select,
                        "change" => !is_toggle && is_select,
                        _ => false,
                    };
                    if fire && let Some(value) = self.parameter_from_element(index, &element) {
                        self.set_parameter(index, value);
                    }
                }
            }
        } else if let Some(sound) = element.get_attribute("data-sound") {
            if kind == "click" && self.state.draft.is_none() {
                self.client.queue(client::select_sound(&sound));
            }
        } else if let Some(action) = element.get_attribute("data-action") {
            let fire = if action == "name" {
                kind == "change"
            } else {
                kind == "click"
            };
            if fire {
                self.action(&action, &element);
            }
        }
        self.pump();
    }

    /// Turn the knob under the pointer, and tell the host as it moves.
    fn drag_to(&mut self, current_y: f64, preview: bool) {
        let Some(drag) = &self.drag else {
            return;
        };
        let input = drag.input.clone();
        let minimum = attribute(&input, "min").unwrap_or(0.0);
        let maximum = attribute(&input, "max").unwrap_or(1.0);
        let step = attribute(&input, "step").unwrap_or(0.0);
        if maximum <= minimum {
            return;
        }
        let raw = drag.start_value + (drag.start_y - current_y) / KNOB_THROW * (maximum - minimum);
        let value = if step.is_finite() && step > 0.0 {
            (minimum + ((raw - minimum) / step).round() * step).clamp(minimum, maximum)
        } else {
            raw.clamp(minimum, maximum)
        };
        if let Some(field) = input.dyn_ref::<HtmlInputElement>() {
            field.set_value_as_number(value);
        }
        show_value(&input, value);
        if let Some(id) = input.get_attribute("data-field") {
            self.edit_field(&id, FieldValue::Integer(value.round() as i64), preview);
        } else if let Some(index) = input
            .get_attribute("data-param")
            .and_then(|p| p.parse::<usize>().ok())
        {
            self.set_parameter(index, value);
        }
        self.pump();
    }
}

fn attribute(element: &Element, name: &str) -> Option<f64> {
    element.get_attribute(name)?.parse().ok()
}

/// Point the knob and refresh the number under it, keeping whatever unit the
/// rendered output already carries.
fn show_value(input: &Element, value: f64) {
    let Some(control) = input.closest(".ctl").ok().flatten() else {
        return;
    };
    if let Some(knob) = control.query_selector(".knob").ok().flatten() {
        let minimum = attribute(input, "min").unwrap_or(0.0);
        let maximum = attribute(input, "max").unwrap_or(1.0);
        let fraction = if maximum > minimum {
            ((value - minimum) / (maximum - minimum)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let _ = knob.set_attribute(
            "style",
            &format!("--knob-turn:{:.2}deg", -135.0 + fraction * 270.0),
        );
    }
    if let Some(output) = control.query_selector("output").ok().flatten() {
        let text = output.text_content().unwrap_or_default();
        let unit = text
            .trim_start_matches(|c: char| c.is_ascii_digit() || c == '-' || c == '.')
            .to_owned();
        let decimals = input
            .get_attribute("step")
            .and_then(|step| step.split_once('.').map(|(_, tail)| tail.len()))
            .unwrap_or(0)
            .min(4);
        output.set_text_content(Some(&format!("{value:.decimals$}{unit}")));
    }
}

fn listen(app: &Shared, kind: &'static str) -> Result<(), JsValue> {
    let root = app
        .borrow()
        .document
        .get_element_by_id("surface")
        .ok_or_else(|| JsValue::from_str("missing surface root"))?;
    let shared = app.clone();
    let callback = Closure::<dyn FnMut(Event)>::new(move |event: Event| {
        shared.borrow_mut().handle_event(kind, &event);
    });
    root.add_event_listener_with_callback(kind, callback.as_ref().unchecked_ref())?;
    callback.forget();
    Ok(())
}

/// Pointer drags on the knobs: press, turn, release.
fn listen_knobs(app: &Shared) -> Result<(), JsValue> {
    let root = app
        .borrow()
        .document
        .get_element_by_id("surface")
        .ok_or_else(|| JsValue::from_str("missing surface root"))?;
    for kind in ["pointerdown", "pointermove", "pointerup", "pointercancel"] {
        let shared = app.clone();
        let callback = Closure::<dyn FnMut(PointerEvent)>::new(move |event: PointerEvent| {
            let mut app = shared.borrow_mut();
            match kind {
                "pointerdown" => {
                    if app.drag.is_some() || event.button() != 0 {
                        return;
                    }
                    let Some(knob) = event
                        .target()
                        .and_then(|t| t.dyn_into::<Element>().ok())
                        .and_then(|t| t.closest("[data-knob]").ok().flatten())
                    else {
                        return;
                    };
                    let Some(input) = knob.query_selector(".knob-input").ok().flatten() else {
                        return;
                    };
                    let start_value = input
                        .dyn_ref::<HtmlInputElement>()
                        .map(|field| field.value_as_number())
                        .filter(|value| value.is_finite())
                        .unwrap_or(0.0);
                    let _ = knob.set_pointer_capture(event.pointer_id());
                    event.prevent_default();
                    app.drag = Some(Drag {
                        pointer: event.pointer_id(),
                        input,
                        start_y: f64::from(event.client_y()),
                        start_value,
                    });
                }
                "pointermove" => {
                    if app
                        .drag
                        .as_ref()
                        .is_some_and(|drag| drag.pointer == event.pointer_id())
                    {
                        app.drag_to(f64::from(event.client_y()), true);
                    }
                }
                _ => {
                    if app
                        .drag
                        .as_ref()
                        .is_some_and(|drag| drag.pointer == event.pointer_id())
                    {
                        // Confirm the value the knob landed on, then let the
                        // page redraw around it.
                        app.drag_to(f64::from(event.client_y()), false);
                        app.drag = None;
                        app.pump();
                    }
                }
            }
        });
        root.add_event_listener_with_callback(kind, callback.as_ref().unchecked_ref())?;
        callback.forget();
    }
    Ok(())
}

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("missing window"))?;
    let document = window
        .document()
        .ok_or_else(|| JsValue::from_str("missing document"))?;
    let origin = window.location().origin()?;
    let app: Shared = Rc::new(RefCell::new(App {
        window,
        document,
        origin,
        state: State::default(),
        client: Client::default(),
        rendered: Default::default(),
        last_poll: 0.0,
        info: None,
        drag: None,
    }));
    for kind in ["input", "change", "click"] {
        listen(&app, kind)?;
    }
    listen_knobs(&app)?;
    let messages = app.clone();
    let callback = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
        let mut app = messages.borrow_mut();
        let from_parent = app
            .window
            .parent()
            .ok()
            .flatten()
            .zip(event.source())
            .is_some_and(|(parent, source)| Object::is(parent.as_ref(), source.as_ref()));
        if !from_parent || event.origin() != app.origin {
            return;
        }
        let Some(text) = JSON::stringify(&event.data())
            .ok()
            .and_then(|text| text.as_string())
        else {
            return;
        };
        if text.len() > 4 * 1024 * 1024 {
            return;
        }
        let Ok(message) = serde_json::from_str::<Value>(&text) else {
            return;
        };
        if message["protocol"] != PROTOCOL {
            return;
        }
        app.handle_message(&message);
    });
    app.borrow()
        .window
        .add_event_listener_with_callback("message", callback.as_ref().unchecked_ref())?;
    callback.forget();
    let timer = app.clone();
    let callback = Closure::<dyn FnMut()>::new(move || {
        timer.borrow_mut().pump();
    });
    app.borrow()
        .window
        .set_interval_with_callback_and_timeout_and_arguments_0(
            callback.as_ref().unchecked_ref(),
            500,
        )?;
    callback.forget();
    app.borrow_mut().render();
    app.borrow()
        .send(&json!({"protocol": PROTOCOL, "kind": "ready"}))
}

impl App {
    fn handle_message(&mut self, message: &Value) {
        match message["kind"].as_str() {
            Some("context") => {
                let was_connected = self.state.connected;
                if self.state.apply_context(message, PLUGIN_ID) {
                    self.state.status.clear();
                    if !was_connected {
                        // A fresh host: nothing in flight is known to have
                        // landed, and the controls are fetched at once.
                        self.client.clear();
                        self.last_poll = f64::NEG_INFINITY;
                    }
                }
            }
            Some("parameter_changed") => self.state.apply_parameter_change(message),
            Some("response") => {
                if let Some(reply) = self.client.reply(message) {
                    self.apply_reply(&reply.key, reply.ok, &reply.result, &reply.error);
                }
            }
            _ => {}
        }
        self.pump();
    }

    fn apply_reply(&mut self, key: &str, ok: bool, result: &Value, error: &str) {
        if let Some(id) = key.strip_prefix("field:") {
            if ok {
                if let Some(pending) = self.state.pending_fields.get_mut(id) {
                    pending.1 = true;
                }
            } else {
                self.state.pending_fields.remove(id);
                self.state.status = error.to_owned();
            }
        } else if let Some(index) = key
            .strip_prefix("param:")
            .and_then(|i| i.parse::<usize>().ok())
        {
            if ok {
                if let Some(value) = result["value"].as_f64() {
                    self.state
                        .apply_parameter_change(&json!({"parameter_index": index, "value": value}));
                }
                self.state.pending_parameters.remove(&index);
            } else {
                self.state.pending_parameters.remove(&index);
                self.state.status = error.to_owned();
            }
        } else if key == "parameters" {
            if !ok || !self.state.apply_parameters(result) {
                self.state.status = "RackForge did not send the controls.".into();
            }
        } else if key != "info" && !ok {
            self.state.status = error.to_owned();
        }
    }
}
