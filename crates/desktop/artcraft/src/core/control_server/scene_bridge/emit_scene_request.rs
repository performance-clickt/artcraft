use crate::core::control_server::scene_bridge::scene_op::SceneOp;
use crate::core::control_server::state::control_bridge_state::{ControlBridgeReply, ControlBridgeState};
use crate::core::events::basic_sendable_event_trait::BasicSendableEvent;
use crate::core::events::control_scene_request_event::ControlSceneRequestEvent;
use crate::core::events::sendable_event_error::SendableEventError;
use serde_json::Value;
use tauri::AppHandle;
use tokio::sync::oneshot;
use uuid::Uuid;

/// Reserves `request_id` in the correlation map and emits the scene request to the webview.
///
/// Registration happens BEFORE the emit so a frontend that replies synchronously cannot beat us
/// to the map. If the emit fails there is nobody to answer, so the reservation is released here
/// rather than being left for the timeout to collect.
pub fn emit_scene_request(
  app_handle: &AppHandle,
  bridge_state: &ControlBridgeState,
  request_id: Uuid,
  op: SceneOp,
  payload: Value,
) -> Result<oneshot::Receiver<ControlBridgeReply>, SendableEventError> {
  let receiver = bridge_state.register(request_id);

  let event = ControlSceneRequestEvent {
    request_id,
    op,
    payload,
  };

  if let Err(err) = event.send(app_handle) {
    bridge_state.cancel(&request_id);
    return Err(err);
  }

  Ok(receiver)
}
