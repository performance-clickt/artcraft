use crate::core::control_server::scene_bridge::scene_op::SceneOp;
use crate::core::events::basic_sendable_event_trait::{BasicEventStatus, BasicSendableEvent};
use crate::core::events::sendable_event_error::SendableEventError;
use enums::tauri::ux::tauri_event_name::TauriEventName;
use log::info;
use serde_derive::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

/// Asks the webview to run one scene operation on the live 3D editor.
///
/// The webview CSP blocks the frontend from calling the loopback control server, so this event
/// is the outbound half of the bridge: the control server emits it, `<ControlBridge/>` executes
/// it, and the answer comes back through `control_bridge_reply_command` carrying the same
/// `request_id`. The wrapper the trait adds means the frontend sees
/// `{status: "success", data: {request_id, op, payload}}`.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ControlSceneRequestEvent {
  pub request_id: Uuid,
  pub op: SceneOp,
  /// Op-specific arguments, verbatim from the HTTP request body. `Value::Null` when the caller
  /// sent no body (e.g. `status`).
  pub payload: Value,
}

impl BasicSendableEvent for ControlSceneRequestEvent {
  const FRONTEND_EVENT_NAME: TauriEventName = TauriEventName::ControlSceneRequestEvent;
  const EVENT_STATUS: BasicEventStatus = BasicEventStatus::Success;

  /// NB: `send` is overridden purely to keep the payload out of the log. The trait's default
  /// `Debug`-formats the whole wrapped event, and this one carries the scene JSON verbatim from
  /// the HTTP body — a multi-megabyte apply_scene in an agent loop would write tens of megabytes
  /// of log and block the emitting task formatting it. The correlation id, the op and the payload
  /// shape are what a reader actually needs.
  fn send(&self, app: &AppHandle) -> Result<(), SendableEventError> {
    info!(
      "[ControlServer] Emitting {} request {} ({}), payload: {}",
      Self::FRONTEND_EVENT_NAME.to_str(),
      self.request_id,
      self.op.to_str(),
      describe_payload_shape(&self.payload),
    );

    let wrapped = ControlSceneRequestEventWrapper {
      status: Self::EVENT_STATUS,
      data: self,
    };

    app.emit(Self::FRONTEND_EVENT_NAME.to_str(), wrapped)
        .map_err(SendableEventError::from)
  }
}

/// The `{status, data}` envelope the trait's default `send` wraps every event in, restated here
/// because the trait's own wrapper type is private. NB: The field names and the status encoding
/// must stay identical to it — the frontend parses one shape for all events.
#[derive(Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct ControlSceneRequestEventWrapper<'a> {
  status: BasicEventStatus,
  data: &'a ControlSceneRequestEvent,
}

/// A one-line, size-bounded description of the payload: enough to tell an empty body from a scene
/// graph, without ever formatting the body itself.
fn describe_payload_shape(payload: &Value) -> String {
  match payload {
    Value::Null => "null".to_string(),
    Value::Bool(_) => "bool".to_string(),
    Value::Number(_) => "number".to_string(),
    Value::String(text) => format!("string of {} chars", text.len()),
    Value::Array(items) => format!("array of {} items", items.len()),
    Value::Object(fields) => format!("object with {} fields", fields.len()),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  #[test]
  fn test_an_empty_payload_is_described_without_its_contents() {
    assert_eq!(describe_payload_shape(&Value::Null), "null");
  }

  #[test]
  fn test_a_scene_object_is_described_by_field_count_only() {
    let payload = json!({"objects": [1, 2, 3], "name": "scene"});

    assert_eq!(describe_payload_shape(&payload), "object with 2 fields");
  }

  #[test]
  fn test_an_array_is_described_by_length_only() {
    let payload = json!([1, 2, 3]);

    assert_eq!(describe_payload_shape(&payload), "array of 3 items");
  }
}
