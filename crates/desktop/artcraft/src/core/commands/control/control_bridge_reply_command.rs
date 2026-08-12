use crate::core::commands::response::shorthand::SuccessOrErrorMessage;
use crate::core::commands::response::success_response_wrapper::CommandSuccessResponseWrapper;
use crate::core::control_server::state::control_bridge_state::{ControlBridgeReply, ControlBridgeReplyError, ControlBridgeState};
use log::debug;
use serde_derive::Deserialize;
use serde_json::Value;
use tauri::State;
use uuid::Uuid;

/// The webview's answer to one `control_scene_request_event`.
///
/// NB: The wire field names (`request_id`, `success`, `data`, `error`) are the bridge contract
/// with `<ControlBridge/>` (HM-921); the local `maybe_` prefixes are this repo's convention for
/// optional fields and must keep their renames.
#[derive(Deserialize)]
pub struct ControlBridgeReplyRequest {
  /// REQUIRED.
  /// Echoed from the request event — this is what correlates the reply with a waiting caller.
  pub request_id: Uuid,

  /// REQUIRED.
  /// Whether the frontend performed the operation.
  pub success: bool,

  /// Operation result, present on success.
  #[serde(default, rename = "data")]
  pub maybe_data: Option<Value>,

  /// Failure detail, present on failure.
  #[serde(default, rename = "error")]
  pub maybe_error: Option<ControlBridgeReplyErrorRequest>,
}

#[derive(Deserialize)]
pub struct ControlBridgeReplyErrorRequest {
  /// A control-protocol error code, e.g. `SCENE_NOT_ACTIVE`. Unrecognized or absent codes are
  /// reported to the HTTP caller as `INTERNAL`.
  #[serde(default, rename = "code")]
  pub maybe_code: Option<String>,

  #[serde(default)]
  pub message: String,
}

/// Hands a scene reply back to the HTTP request that is waiting on it.
///
/// This command NEVER fails from the frontend's point of view. An unknown `request_id` just means
/// the HTTP side already gave up (or the window replied twice); erroring there would surface a
/// spurious failure in the UI for a request no one is waiting on any more.
#[tauri::command]
pub async fn control_bridge_reply_command(
  request: ControlBridgeReplyRequest,
  control_bridge_state: State<'_, ControlBridgeState>,
) -> SuccessOrErrorMessage {

  let request_id = request.request_id;
  let reply = to_bridge_reply(request);

  if !control_bridge_state.complete(&request_id, reply) {
    debug!(
      "[ControlServer] Dropping scene bridge reply for unknown request {} (already timed out or replied).",
      request_id,
    );
  }

  Ok(CommandSuccessResponseWrapper::empty_success())
}

fn to_bridge_reply(request: ControlBridgeReplyRequest) -> ControlBridgeReply {
  ControlBridgeReply {
    success: request.success,
    maybe_data: request.maybe_data,
    maybe_error: request.maybe_error.map(|error| ControlBridgeReplyError {
      maybe_code: error.maybe_code,
      message: error.message,
    }),
  }
}
