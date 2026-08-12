use crate::core::control_server::envelope::control_response::{ControlErrorCode, ControlErrorResponse, ControlSuccessResponse};
use crate::core::control_server::scene_bridge::await_bridge_reply::{await_bridge_reply, AwaitBridgeReplyError, SCENE_BRIDGE_TIMEOUT_SECONDS};
use crate::core::control_server::scene_bridge::emit_scene_request::emit_scene_request;
use crate::core::control_server::scene_bridge::scene_op::SceneOp;
use crate::core::control_server::state::control_bridge_state::{ControlBridgeReply, ControlBridgeState};
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use log::{debug, warn};
use serde_json::Value;
use tauri::{AppHandle, Manager};
use uuid::Uuid;

const BRIDGE_UNAVAILABLE_MESSAGE: &str = "Scene bridge state is unavailable.";
const EMIT_FAILED_MESSAGE: &str = "Failed to deliver the scene request to the app window.";
const REPLY_LOST_MESSAGE: &str = "The scene request was dropped before the app window answered.";
const UNKNOWN_ERROR_MESSAGE: &str = "The app window reported a failure with no message.";

/// `POST /v1/scene/{op}` — runs one operation against the live 3D editor in the webview.
///
/// The webview cannot call this server back over HTTP (CSP), so the round trip is: emit a
/// correlated Tauri event, wait on a oneshot, and let `control_bridge_reply_command` complete it.
/// With no listener mounted the wait ends in `SCENE_BRIDGE_TIMEOUT` rather than hanging.
pub async fn post_scene_handler(
  State(app_handle): State<AppHandle>,
  Path(op): Path<String>,
  body: Bytes,
) -> Response {
  let Some(scene_op) = SceneOp::from_str(&op) else {
    return ControlErrorResponse::new(ControlErrorCode::BadRequest, unknown_op_message(&op))
      .into_response();
  };

  let payload = match parse_payload(&body) {
    Ok(payload) => payload,
    Err(message) => {
      return ControlErrorResponse::new(ControlErrorCode::BadRequest, message).into_response();
    }
  };

  // `None` when the state was never managed — answer with an envelope instead of panicking the
  // request task, which would drop the connection with no response.
  let Some(bridge_state) = app_handle.try_state::<ControlBridgeState>() else {
    warn!("[ControlServer] Control bridge state is not managed by Tauri.");

    return ControlErrorResponse::new(ControlErrorCode::Internal, BRIDGE_UNAVAILABLE_MESSAGE)
      .into_response();
  };

  let request_id = Uuid::new_v4();

  let pending = match emit_scene_request(&app_handle, &bridge_state, request_id, scene_op, payload) {
    Ok(pending) => pending,
    Err(err) => {
      warn!("[ControlServer] Failed to emit scene request {}: {:?}", request_id, err);

      return ControlErrorResponse::new(ControlErrorCode::Internal, EMIT_FAILED_MESSAGE)
        .into_response();
    }
  };

  // NB: `pending` is consumed here; its `Drop` is what releases the correlation-map entry, so
  // the count logged below is read after the release.
  let result = await_bridge_reply(pending).await;

  debug!(
    "[ControlServer] Scene request {} ({}) finished; {} scene request(s) still pending.",
    request_id,
    scene_op.to_str(),
    bridge_state.pending_count(),
  );

  match result {
    Ok(reply) => to_reply_response(reply),
    Err(AwaitBridgeReplyError::TimedOut) => {
      ControlErrorResponse::new(ControlErrorCode::SceneBridgeTimeout, timeout_message())
        .into_response()
    }
    Err(AwaitBridgeReplyError::SenderDropped) => {
      warn!("[ControlServer] Scene request {} lost its reply channel.", request_id);

      ControlErrorResponse::new(ControlErrorCode::Internal, REPLY_LOST_MESSAGE).into_response()
    }
  }
}

fn to_reply_response(reply: ControlBridgeReply) -> Response {
  if reply.success {
    return ControlSuccessResponse::new(reply.maybe_data.unwrap_or(Value::Null)).into_response();
  }

  let Some(error) = reply.maybe_error else {
    return ControlErrorResponse::new(ControlErrorCode::Internal, UNKNOWN_ERROR_MESSAGE)
      .into_response();
  };

  let code = to_error_code(error.maybe_code.as_deref());
  let message = if error.message.is_empty() {
    UNKNOWN_ERROR_MESSAGE.to_string()
  } else {
    error.message
  };

  ControlErrorResponse::new(code, message).into_response()
}

/// The frontend only gets to pick from the codes this endpoint is allowed to raise; anything
/// else is a frontend bug and is reported as `INTERNAL` rather than invented into the protocol.
fn to_error_code(maybe_code: Option<&str>) -> ControlErrorCode {
  match maybe_code {
    Some("SCENE_NOT_ACTIVE") => ControlErrorCode::SceneNotActive,
    Some("BAD_REQUEST") => ControlErrorCode::BadRequest,
    _ => ControlErrorCode::Internal,
  }
}

/// An absent body is the normal shape for argument-free ops such as `status`, so it maps to
/// `null` rather than being rejected. A present body must be valid JSON.
fn parse_payload(body: &Bytes) -> Result<Value, String> {
  if body.is_empty() {
    return Ok(Value::Null);
  }

  serde_json::from_slice(body)
    .map_err(|err| format!("Request body is not valid JSON: {}", err))
}

fn unknown_op_message(op: &str) -> String {
  let supported = SceneOp::all_variants()
    .iter()
    .map(|op| op.to_str())
    .collect::<Vec<_>>()
    .join(", ");

  format!("Unknown scene op {:?}. Supported ops: {}.", op, supported)
}

fn timeout_message() -> String {
  format!(
    "The ArtCraft window did not answer within {}s. Open the 3D scene tab in ArtCraft first.",
    SCENE_BRIDGE_TIMEOUT_SECONDS,
  )
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::control_server::state::control_bridge_state::ControlBridgeReplyError;
  use axum::http::StatusCode;

  mod payload_parsing {
    use super::*;

    #[test]
    fn test_empty_body_becomes_null() {
      assert_eq!(parse_payload(&Bytes::new()), Ok(Value::Null));
    }

    #[test]
    fn test_json_body_is_passed_through() {
      let body = Bytes::from_static(br#"{"uuid":"abc"}"#);

      assert_eq!(parse_payload(&body).unwrap()["uuid"], Value::from("abc"));
    }

    #[test]
    fn test_malformed_body_is_rejected() {
      assert!(parse_payload(&Bytes::from_static(b"{nope")).is_err());
    }
  }

  mod reply_mapping {
    use super::*;

    #[test]
    fn test_success_reply_is_a_200() {
      let response = to_reply_response(ControlBridgeReply {
        success: true,
        maybe_data: Some(Value::from(7)),
        maybe_error: None,
      });

      assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn test_scene_not_active_reply_keeps_its_code() {
      let response = to_reply_response(failure_reply(Some("SCENE_NOT_ACTIVE")));

      assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[test]
    fn test_unrecognized_and_missing_codes_become_internal() {
      assert_eq!(to_error_code(Some("NOT_A_REAL_CODE")), ControlErrorCode::Internal);
      assert_eq!(to_error_code(None), ControlErrorCode::Internal);
      assert_eq!(to_reply_response(failure_reply(None)).status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
  }

  mod messages {
    use super::*;

    #[test]
    fn test_unknown_op_message_lists_every_supported_op() {
      let message = unknown_op_message("nope");

      for op in SceneOp::all_variants() {
        assert!(message.contains(op.to_str()), "missing {} in {:?}", op.to_str(), message);
      }
    }
  }

  fn failure_reply(maybe_code: Option<&str>) -> ControlBridgeReply {
    ControlBridgeReply {
      success: false,
      maybe_data: None,
      maybe_error: Some(ControlBridgeReplyError {
        maybe_code: maybe_code.map(|code| code.to_string()),
        message: "nope".to_string(),
      }),
    }
  }
}
