use crate::core::commands::generate::generate_video::generate_video_command::handle_request as handle_generate_video_request;
use crate::core::commands::generate::generate_video::request::TauriGenerateVideoRequest;
use crate::core::control_server::endpoints::generate::common::enqueued_task_response::{
  notify_frontend_of_enqueue_success, EnqueuedTaskResponse,
};
use crate::core::control_server::endpoints::generate::common::generate_error_mapping::generate_error_to_control_response;
use crate::core::control_server::endpoints::generate::common::json_body::read_json_body;
use crate::core::control_server::endpoints::generate::common::require_tauri_state::require_tauri_state;
use crate::core::control_server::envelope::control_response::{
  ControlErrorCode, ControlErrorResponse, ControlSuccessResponse,
};
use crate::core::state::app_env_configs::app_env_configs::AppEnvConfigs;
use crate::core::state::artcraft_usage_tracker::artcraft_usage_tracker::ArtcraftUsageTracker;
use crate::core::state::data_dir::app_data_root::AppDataRoot;
use crate::core::state::provider_priority::ProviderPriorityStore;
use crate::core::state::task_database::TaskDatabase;
use crate::services::grok::state::grok_credential_manager::GrokCredentialManager;
use crate::services::sora::state::sora_credential_manager::SoraCredentialManager;
use crate::services::storyteller::state::storyteller_credential_manager::StorytellerCredentialManager;
use axum::extract::rejection::JsonRejection;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::Json;
use tauri::AppHandle;

const ENDPOINT: &str = "POST /v1/generate/video";

/// `POST /v1/generate/video`.
///
/// NB: The body type is the Tauri command's own request struct. Unlike the image endpoint it takes
/// no inline image bytes — every input is a media token — so there is nothing to lower and a
/// mirrored struct would only be a second thing to keep in sync.
pub async fn post_generate_video_handler(
  State(app_handle): State<AppHandle>,
  body: Result<Json<TauriGenerateVideoRequest>, JsonRejection>,
) -> Response {
  match handle_generate_video(&app_handle, body).await {
    Ok(response) => response.into_response(),
    Err(error) => error.into_response(),
  }
}

async fn handle_generate_video(
  app_handle: &AppHandle,
  body: Result<Json<TauriGenerateVideoRequest>, JsonRejection>,
) -> Result<ControlSuccessResponse<EnqueuedTaskResponse>, ControlErrorResponse> {
  let request = normalize_start_frame(read_json_body(body)?)?;

  let app_env_configs = require_tauri_state::<AppEnvConfigs>(app_handle)?;
  let app_data_root = require_tauri_state::<AppDataRoot>(app_handle)?;
  let artcraft_usage_tracker = require_tauri_state::<ArtcraftUsageTracker>(app_handle)?;
  let provider_priority_store = require_tauri_state::<ProviderPriorityStore>(app_handle)?;
  let task_database = require_tauri_state::<TaskDatabase>(app_handle)?;
  let grok_creds_manager = require_tauri_state::<GrokCredentialManager>(app_handle)?;
  let sora_creds_manager = require_tauri_state::<SoraCredentialManager>(app_handle)?;
  let storyteller_creds_manager = require_tauri_state::<StorytellerCredentialManager>(app_handle)?;

  let success = handle_generate_video_request(
    request,
    app_handle,
    &app_env_configs,
    &app_data_root,
    &artcraft_usage_tracker,
    &provider_priority_store,
    &task_database,
    &grok_creds_manager,
    &sora_creds_manager,
    &storyteller_creds_manager,
  )
    .await
    .map_err(|err| generate_error_to_control_response(ENDPOINT, err))?;

  notify_frontend_of_enqueue_success(app_handle, &success);

  Ok(ControlSuccessResponse::new(
    EnqueuedTaskResponse::from_enqueue_success(&task_database, &success).await,
  ))
}

/// Validates the model and mirrors `generate_video_command`'s legacy/modern start-frame fixup.
///
/// NB: `#[allow(deprecated)]` is deliberate: `image_media_token` is deprecated *for callers*, but
/// the legacy provider handlers still read it, so the command aliases the two fields together and
/// this endpoint must behave identically or a legacy-shaped body would silently lose its image.
#[allow(deprecated)]
fn normalize_start_frame(
  mut request: TauriGenerateVideoRequest,
) -> Result<TauriGenerateVideoRequest, ControlErrorResponse> {
  if request.model.is_none() {
    return Err(ControlErrorResponse::new(
      ControlErrorCode::BadRequest,
      "Field `model` is required.",
    ));
  }

  if request.image_media_token.is_none() && request.start_frame_image_media_token.is_some() {
    request.image_media_token = request.start_frame_image_media_token.clone();
  }

  if request.image_media_token.is_some() && request.start_frame_image_media_token.is_none() {
    request.start_frame_image_media_token = request.image_media_token.clone();
  }

  Ok(request)
}

#[cfg(test)]
mod tests {
  use super::*;

  mod validation_tests {
    use super::*;

    #[test]
    fn test_empty_body_is_rejected_for_a_missing_model() {
      let request: TauriGenerateVideoRequest =
        serde_json::from_str("{}").expect("an empty object should deserialize");

      match normalize_start_frame(request) {
        Ok(_) => panic!("expected a rejection"),
        Err(error) => assert_eq!(error.error.code, ControlErrorCode::BadRequest),
      }
    }
  }

  mod start_frame_tests {
    use super::*;

    #[test]
    #[allow(deprecated)]
    fn test_a_modern_start_frame_is_mirrored_onto_the_legacy_field() {
      let request: TauriGenerateVideoRequest = serde_json::from_str(r#"{
        "model": "veo_3_fast",
        "start_frame_image_media_token": "mf_test"
      }"#).expect("body should deserialize");

      let request = expect_validated(normalize_start_frame(request));

      assert_eq!(request.image_media_token, request.start_frame_image_media_token);
      assert!(request.image_media_token.is_some());
    }

    #[test]
    #[allow(deprecated)]
    fn test_a_legacy_image_token_is_mirrored_onto_the_start_frame_field() {
      let request: TauriGenerateVideoRequest = serde_json::from_str(r#"{
        "model": "veo_3_fast",
        "image_media_token": "mf_test"
      }"#).expect("body should deserialize");

      let request = expect_validated(normalize_start_frame(request));

      assert_eq!(request.start_frame_image_media_token, request.image_media_token);
      assert!(request.start_frame_image_media_token.is_some());
    }
  }

  // NB: `ControlErrorResponse` is not `Debug` (it is a wire type), so this stands in for
  // `unwrap`/`expect` rather than widening the envelope's derives for tests alone.
  fn expect_validated(
    result: Result<TauriGenerateVideoRequest, ControlErrorResponse>,
  ) -> TauriGenerateVideoRequest {
    match result {
      Ok(request) => request,
      Err(error) => panic!("expected success, got {}", error.error.code.to_str()),
    }
  }
}
