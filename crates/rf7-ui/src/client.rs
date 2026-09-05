//! What to ask the host, and in what order.
//!
//! One request is in flight at a time, the way the host expects its plugin
//! surfaces to behave. Requests carry a key; queuing a second request with a
//! key already waiting replaces it, so a slider dragged through fifty values
//! sends the latest one, not fifty. A reply that takes more than five seconds
//! drops the connection state and the queue with it: whatever was sent is no
//! longer known to have happened, and the next context will say.

use crate::PROTOCOL;
use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    /// Identifies what the request changes, for coalescing and for applying
    /// the reply: `parameters`, `param:3`, `field:op1.out`, `select`, ...
    pub key: String,
    pub method: String,
    pub params: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Reply {
    pub key: String,
    pub ok: bool,
    pub result: Value,
    pub error: String,
}

#[derive(Debug, Default)]
pub struct Client {
    queue: Vec<Request>,
    pending: Option<(String, Request, f64)>,
    serial: u64,
}

pub const TIMEOUT_MS: f64 = 5000.0;

impl Client {
    pub fn queue(&mut self, request: Request) {
        if let Some(slot) = self.queue.iter_mut().find(|r| r.key == request.key) {
            *slot = request;
        } else {
            self.queue.push(request);
        }
    }

    pub fn is_idle(&self) -> bool {
        self.pending.is_none() && self.queue.is_empty()
    }

    /// True if a reply took too long; the caller should treat the host as
    /// gone until the next context.
    pub fn timed_out(&mut self, now: f64) -> bool {
        if self
            .pending
            .as_ref()
            .is_some_and(|(_, _, sent)| now - sent > TIMEOUT_MS)
        {
            self.pending = None;
            self.queue.clear();
            return true;
        }
        false
    }

    /// The next message to post, if one may go now.
    pub fn next(&mut self, now: f64) -> Option<Value> {
        if self.pending.is_some() || self.queue.is_empty() {
            return None;
        }
        let request = self.queue.remove(0);
        self.serial = self.serial.wrapping_add(1);
        let id = format!("rf7-{}", self.serial);
        let message = json!({
            "protocol": PROTOCOL,
            "kind": "request",
            "request_id": id,
            "method": request.method,
            "params": request.params,
        });
        self.pending = Some((id, request, now));
        Some(message)
    }

    /// Match a response to the request in flight.
    pub fn reply(&mut self, message: &Value) -> Option<Reply> {
        let (id, _, _) = self.pending.as_ref()?;
        if message["request_id"].as_str() != Some(id) {
            return None;
        }
        let (_, request, _) = self.pending.take()?;
        Some(Reply {
            key: request.key,
            ok: message["ok"].as_bool() == Some(true),
            result: message["result"].clone(),
            error: message["error"]
                .as_str()
                .or_else(|| message["error"]["message"].as_str())
                .unwrap_or("The host refused the request.")
                .to_owned(),
        })
    }

    pub fn clear(&mut self) {
        self.queue.clear();
        self.pending = None;
    }
}

pub fn fetch_parameters() -> Request {
    Request {
        key: "parameters".into(),
        method: "plugin.parameters".into(),
        params: json!({}),
    }
}

pub fn set_parameter(index: usize, value: f64) -> Request {
    Request {
        key: format!("param:{index}"),
        method: "plugin.set_parameter".into(),
        params: json!({"parameter_index": index, "value": value}),
    }
}

pub fn select_sound(sound_id: &str) -> Request {
    Request {
        key: "select".into(),
        method: "plugin.select_sound".into(),
        params: json!({"sound_id": sound_id}),
    }
}

pub fn begin_program_edit(program_id: Option<&str>) -> Request {
    Request {
        key: "begin".into(),
        method: "plugin.begin_program_edit".into(),
        params: json!({"program_id": program_id}),
    }
}

pub fn edit_field(draft_id: u64, field_id: &str, value: &Value, preview: bool) -> Request {
    Request {
        key: format!("field:{field_id}"),
        method: "plugin.edit_program_field".into(),
        params: json!({"draft_id": draft_id, "field_id": field_id, "value": value, "preview": preview}),
    }
}

pub fn set_program_name(draft_id: u64, name: &str) -> Request {
    Request {
        key: "name".into(),
        method: "plugin.set_program_name".into(),
        params: json!({"draft_id": draft_id, "name": name}),
    }
}

pub fn save_program(draft_id: u64) -> Request {
    Request {
        key: "save".into(),
        method: "plugin.save_program".into(),
        params: json!({"draft_id": draft_id}),
    }
}

pub fn cancel_program(draft_id: u64) -> Request {
    Request {
        key: "cancel".into(),
        method: "plugin.cancel_program".into(),
        params: json!({"draft_id": draft_id}),
    }
}

pub fn surface_info(label: &str, value: Option<&str>) -> Request {
    Request {
        key: "info".into(),
        method: "plugin.set_surface_info".into(),
        params: json!({"label": label, "value": value}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(request: &Value, result: Value) -> Value {
        json!({"protocol": PROTOCOL, "kind": "response", "request_id": request["request_id"], "ok": true, "result": result})
    }

    #[test]
    fn one_request_flies_at_a_time_and_a_dragged_field_sends_its_latest_value() {
        let mut client = Client::default();
        client.queue(fetch_parameters());
        let first = client.next(0.0).unwrap();
        assert_eq!(first["method"], "plugin.parameters");
        assert_eq!(first["protocol"], PROTOCOL);
        for value in [10, 20, 30] {
            client.queue(edit_field(
                7,
                "op1.out",
                &json!({"type": "integer", "value": value}),
                true,
            ));
        }
        client.queue(set_parameter(0, 0.5));
        assert!(client.next(1.0).is_none(), "the fetch is still in flight");
        // A stale reply is ignored; the real one frees the line.
        assert!(
            client
                .reply(&json!({"request_id": "rf7-999", "ok": true}))
                .is_none()
        );
        let reply = client.reply(&ok(&first, json!({"values": []}))).unwrap();
        assert_eq!(reply.key, "parameters");
        assert!(reply.ok);
        let edit = client.next(2.0).unwrap();
        assert_eq!(edit["params"]["value"]["value"], 30);
        assert_eq!(edit["params"]["preview"], true);
        client.reply(&ok(&edit, Value::Null));
        let set = client.next(3.0).unwrap();
        assert_eq!(set["method"], "plugin.set_parameter");
        assert_eq!(set["params"]["parameter_index"], 0);
        client.reply(&ok(&set, json!({"value": 0.5})));
        assert!(client.is_idle());
    }

    #[test]
    fn a_reply_that_never_comes_drops_the_queue_with_it() {
        let mut client = Client::default();
        client.queue(select_sound("program-002"));
        let sent = client.next(0.0).unwrap();
        client.queue(save_program(7));
        assert!(!client.timed_out(4000.0));
        assert!(client.timed_out(6000.0));
        assert!(client.is_idle(), "nothing is known to have been sent");
        assert!(client.reply(&ok(&sent, Value::Null)).is_none());
    }

    #[test]
    fn an_error_reply_carries_the_hosts_reason() {
        let mut client = Client::default();
        client.queue(begin_program_edit(None));
        let sent = client.next(0.0).unwrap();
        assert_eq!(sent["params"]["program_id"], Value::Null);
        let reply = client
            .reply(&json!({"request_id": sent["request_id"], "ok": false, "error": "another edit is active"}))
            .unwrap();
        assert!(!reply.ok);
        assert_eq!(reply.error, "another edit is active");
    }
}
