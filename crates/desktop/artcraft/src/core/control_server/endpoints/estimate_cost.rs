use crate::core::commands::cost_estimate::estimate_image_cost_command::estimate_image_cost_command;
use crate::core::commands::cost_estimate::estimate_splat_cost_command::estimate_splat_cost_command;
use crate::core::commands::cost_estimate::estimate_video_cost_command::estimate_video_cost_command;
use crate::core::commands::response::failure_response_wrapper::CommandErrorResponseWrapper;
use crate::core::commands::response::success_response_wrapper::CommandSuccessResponseWrapper;
use crate::core::control_server::envelope::control_response::{ControlErrorCode, ControlErrorResponse, ControlSuccessResponse};
use crate::core::control_server::require_tauri_state::require_tauri_state;
use crate::core::state::app_env_configs::app_env_configs::AppEnvConfigs;
use artcraft_api_defs::generate::cost_estimate::estimate_image_cost::{EstimateImageCostError, EstimateImageCostRequest};
use artcraft_api_defs::generate::cost_estimate::estimate_splat_cost::{EstimateSplatCostError, EstimateSplatCostRequest};
use artcraft_api_defs::generate::cost_estimate::estimate_video_cost::{EstimateVideoCostError, EstimateVideoCostRequest};
use axum::body::Bytes;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use log::warn;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

const EMPTY_PAYLOAD_MESSAGE: &str = "The cost estimate command returned no payload.";
const FALLBACK_FAILURE_MESSAGE: &str = "Failed to estimate cost.";

/// `POST /v1/estimate_cost` — the pre-flight an agent runs before spending credits.
///
/// NB: The body is read as raw bytes rather than through axum's `Json` extractor on purpose. An
/// extractor rejection is emitted by axum itself as bare text, which would be the one response
/// on this server that is not a control envelope; deserializing here keeps every failure —
/// malformed JSON, unknown `kind`, missing field — inside `BAD_REQUEST`.
pub async fn post_estimate_cost_handler(
  State(app_handle): State<AppHandle>,
  body: Bytes,
) -> Response {
  let request = match serde_json::from_slice::<EstimateCostRequest>(&body) {
    Ok(request) => request,
    Err(err) => {
      return ControlErrorResponse::new(ControlErrorCode::BadRequest, err.to_string())
        .into_response();
    }
  };

  let app_env_configs = match require_tauri_state::<AppEnvConfigs>(&app_handle) {
    Ok(state) => state,
    Err(error) => return error.into_response(),
  };

  match request {
    EstimateCostRequest::Image(request) => into_control_response(
      estimate_image_cost_command(request, app_env_configs).await,
      |error: EstimateImageCostError| error.error_message,
    ),
    EstimateCostRequest::Video(request) => into_control_response(
      estimate_video_cost_command(request, app_env_configs).await,
      |error: EstimateVideoCostError| error.error_message,
    ),
    EstimateCostRequest::Splat(request) => into_control_response(
      estimate_splat_cost_command(request, app_env_configs).await,
      |error: EstimateSplatCostError| error.error_message,
    ),
  }
}

/// The request body: `{"kind": "image"|"video"|"splat", ...}` where the remaining fields are the
/// upstream estimate request for that kind, reused verbatim rather than re-declared here so this
/// endpoint cannot drift from the request shape the backend actually accepts.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EstimateCostRequest {
  Image(EstimateImageCostRequest),
  Video(EstimateVideoCostRequest),
  Splat(EstimateSplatCostRequest),
}

/// Maps a cost estimate command result onto the control envelope.
///
/// NB: Failures map to `UPSTREAM_API_ERROR` even though the command labels them `InvalidInput`.
/// That label is unconditional in the command — every failure of the backend call gets it,
/// transport errors included — so it carries no information about whose fault the failure was.
/// The upstream message is passed through verbatim, which is what actually tells a caller
/// whether they sent a bad model. Structurally invalid bodies are already rejected as
/// `BAD_REQUEST` above.
fn into_control_response<T: Serialize, E>(
  result: Result<CommandSuccessResponseWrapper<T>, CommandErrorResponseWrapper<(), E>>,
  error_message: impl FnOnce(E) -> String,
) -> Response
where
  E: Serialize,
{
  match result {
    Ok(success) => match success.payload {
      Some(payload) => ControlSuccessResponse::new(payload).into_response(),
      None => {
        warn!("[ControlServer] Cost estimate command succeeded with an empty payload.");

        ControlErrorResponse::new(ControlErrorCode::Internal, EMPTY_PAYLOAD_MESSAGE)
          .into_response()
      }
    },
    Err(failure) => {
      let message = failure
        .error_details
        .map(error_message)
        .or(failure.error_message)
        .unwrap_or_else(|| FALLBACK_FAILURE_MESSAGE.to_string());

      warn!("[ControlServer] Cost estimate command failed: {}", message);

      ControlErrorResponse::new(ControlErrorCode::UpstreamApiError, message).into_response()
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use enums::common::generation::common_image_model::CommonImageModel;
  use enums::common::generation_provider::GenerationProvider;

  #[test]
  fn test_image_body_deserializes_into_the_upstream_request() {
    let body = r#"{
      "kind": "image",
      "model": "flux_1_schnell",
      "provider": "artcraft",
      "generation_mode": {"type": "text_to_image"},
      "image_batch_count": 1
    }"#;

    let request = serde_json::from_str::<EstimateCostRequest>(body)
      .expect("a well-formed image body should deserialize");

    let EstimateCostRequest::Image(request) = request else {
      panic!("`kind: image` must select the image variant");
    };

    assert_eq!(request.model, CommonImageModel::Flux1Schnell);
    assert!(request.provider == GenerationProvider::Artcraft);
    assert_eq!(request.image_batch_count, Some(1));
  }

  #[test]
  fn test_splat_body_selects_the_splat_variant() {
    let body = r#"{"kind": "splat", "model": "marble_1p0", "provider": "artcraft"}"#;

    let request = serde_json::from_str::<EstimateCostRequest>(body)
      .expect("a well-formed splat body should deserialize");

    assert!(matches!(request, EstimateCostRequest::Splat(_)));
  }

  #[test]
  fn test_unknown_and_missing_kinds_are_rejected() {
    // Rejected here rather than defaulted, so a typo'd kind can never silently price the wrong
    // generation type.
    assert!(serde_json::from_str::<EstimateCostRequest>(r#"{"kind": "banana"}"#).is_err());
    assert!(serde_json::from_str::<EstimateCostRequest>(r#"{"model": "flux_1_schnell"}"#).is_err());
    assert!(serde_json::from_str::<EstimateCostRequest>("not json").is_err());
  }
}
