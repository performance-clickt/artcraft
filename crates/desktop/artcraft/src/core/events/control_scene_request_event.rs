use crate::core::control_server::scene_bridge::scene_op::SceneOp;
use crate::core::events::basic_sendable_event_trait::{BasicEventStatus, BasicSendableEvent};
use enums::tauri::ux::tauri_event_name::TauriEventName;
use serde_derive::Serialize;
use serde_json::Value;
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
}
