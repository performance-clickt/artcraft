use crate::core::commands::generate::models::image::list_image_models_command::list_image_models_command;
use crate::core::commands::generate::models::video::list_video_models_command::list_video_models_command;
use crate::core::commands::response::failure_response_wrapper::CommandErrorResponseWrapper;
use crate::core::commands::response::success_response_wrapper::CommandSuccessResponseWrapper;
use crate::core::control_server::envelope::control_response::{ControlErrorCode, ControlErrorResponse, ControlSuccessResponse};
use crate::core::control_server::require_tauri_state::require_tauri_state;
use crate::core::state::app_env_configs::app_env_configs::AppEnvConfigs;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use log::warn;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

const UNKNOWN_KIND_MESSAGE: &str = "Query parameter `kind` must be one of: image, video.";
const EMPTY_PAYLOAD_MESSAGE: &str = "The model list command returned no payload.";

/// `GET /v1/models?kind=image|video` — the catalog an agent picks a model from before spending
/// credits. Thin proxy over the same command the UI calls, so the 60s in-memory cache and the
/// stale-on-refresh-failure fallback in that command are shared, not duplicated here.
pub async fn get_models_handler(
  State(app_handle): State<AppHandle>,
  // NB: `Result<Query<..>, _>` rather than `Query<..>`. An extractor rejection is emitted by axum
  // itself as bare text, which would be the one response on this server that is not a control
  // envelope; taking the rejection keeps an unparseable query string inside `BAD_REQUEST`.
  query: Result<Query<ModelsQuery>, QueryRejection>,
) -> Response {
  let maybe_kind = query
    .ok()
    .and_then(|Query(query)| query.kind)
    .and_then(|kind| ModelKind::from_query_value(&kind));

  let Some(kind) = maybe_kind else {
    return ControlErrorResponse::new(ControlErrorCode::BadRequest, UNKNOWN_KIND_MESSAGE)
      .into_response();
  };

  let app_env_configs = match require_tauri_state::<AppEnvConfigs>(&app_handle) {
    Ok(state) => state,
    Err(error) => return error.into_response(),
  };

  match kind {
    ModelKind::Image => into_control_response(list_image_models_command(app_env_configs).await),
    ModelKind::Video => into_control_response(list_video_models_command(app_env_configs).await),
  }
}

/// `kind` is `Option` on purpose: an absent or malformed value must produce our `BAD_REQUEST`
/// envelope, whereas a required field would make axum reject the request with a bare-text 400
/// that no control-protocol client can parse.
#[derive(Deserialize)]
pub struct ModelsQuery {
  pub kind: Option<String>,
}

/// The model catalogs the control server exposes. Deliberately closed — an unrecognized kind is
/// a client bug, so it is rejected rather than defaulted to images.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ModelKind {
  Image,
  Video,
}

impl ModelKind {
  fn from_query_value(value: &str) -> Option<Self> {
    match value {
      "image" => Some(Self::Image),
      "video" => Some(Self::Video),
      _ => None,
    }
  }
}

/// Maps a command result onto the control envelope. A command failure here is always a failure
/// of the call out to the backend catalog (the command has no other error path), so it maps to
/// `UPSTREAM_API_ERROR` rather than `INTERNAL`.
fn into_control_response<T: Serialize>(
  result: Result<CommandSuccessResponseWrapper<T>, CommandErrorResponseWrapper<(), ()>>,
) -> Response {
  match result {
    Ok(success) => match success.payload {
      Some(payload) => ControlSuccessResponse::new(payload).into_response(),
      None => {
        warn!("[ControlServer] Model list command succeeded with an empty payload.");

        ControlErrorResponse::new(ControlErrorCode::Internal, EMPTY_PAYLOAD_MESSAGE)
          .into_response()
      }
    },
    Err(failure) => {
      let message = failure
        .error_message
        .unwrap_or_else(|| "Failed to list models.".to_string());

      warn!("[ControlServer] Model list command failed: {}", message);

      ControlErrorResponse::new(ControlErrorCode::UpstreamApiError, message).into_response()
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use axum::http::StatusCode;

  #[test]
  fn test_supported_kinds_are_parsed() {
    assert_eq!(ModelKind::from_query_value("image"), Some(ModelKind::Image));
    assert_eq!(ModelKind::from_query_value("video"), Some(ModelKind::Video));
  }

  #[test]
  fn test_unsupported_kinds_are_rejected() {
    assert_eq!(ModelKind::from_query_value("banana"), None);
    assert_eq!(ModelKind::from_query_value(""), None);
    // NB: Case-sensitive by design — the protocol documents lowercase kinds.
    assert_eq!(ModelKind::from_query_value("Image"), None);
  }

  #[test]
  fn test_rejected_kind_maps_to_http_400() {
    assert_eq!(ControlErrorCode::BadRequest.to_str(), "BAD_REQUEST");
    assert_eq!(ControlErrorCode::BadRequest.http_status(), StatusCode::BAD_REQUEST);
  }
}
