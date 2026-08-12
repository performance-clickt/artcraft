use crate::core::control_server::scene_bridge::scene_op::SceneOp;
use crate::core::control_server::state::control_bridge_state::{ControlBridgeState, PendingSceneRequest};
use crate::core::events::basic_sendable_event_trait::BasicSendableEvent;
use crate::core::events::control_scene_request_event::ControlSceneRequestEvent;
use crate::core::events::sendable_event_error::SendableEventError;
use serde_json::Value;
use tauri::AppHandle;
use uuid::Uuid;

/// Reserves `request_id` in the correlation map and emits the scene request to the webview.
///
/// Registration happens BEFORE the emit so a frontend that replies synchronously cannot beat us
/// to the map. If the emit fails there is nobody to answer, and dropping the guard on the error
/// path releases the reservation immediately instead of leaving it for the timeout to collect.
pub fn emit_scene_request<'a>(
  app_handle: &AppHandle,
  bridge_state: &'a ControlBridgeState,
  request_id: Uuid,
  op: SceneOp,
  payload: Value,
) -> Result<PendingSceneRequest<'a>, SendableEventError> {
  let pending = bridge_state.register(request_id);

  let event = ControlSceneRequestEvent {
    request_id,
    op,
    payload,
  };

  event.send(app_handle)?;

  Ok(pending)
}
