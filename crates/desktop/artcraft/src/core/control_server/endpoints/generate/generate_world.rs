use crate::core::commands::enqueue::image_to_gaussian::enqueue_image_to_gaussian_command::{
  handle_request as handle_image_to_gaussian_request, EnqueueImageToGaussianRequest,
};
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
use crate::core::state::task_database::TaskDatabase;
use crate::services::storyteller::state::storyteller_credential_manager::StorytellerCredentialManager;
use crate::services::worldlabs::state::worldlabs_credential_manager::WorldlabsCredentialManager;
use axum::extract::rejection::JsonRejection;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::Json;
use tauri::AppHandle;

const ENDPOINT: &str = "POST /v1/generate/world";

/// `POST /v1/generate/world` — image to 3D gaussian splat ("world").
pub async fn post_generate_world_handler(
  State(app_handle): State<AppHandle>,
  body: Result<Json<EnqueueImageToGaussianRequest>, JsonRejection>,
) -> Response {
  match handle_generate_world(&app_handle, body).await {
    Ok(response) => response.into_response(),
    Err(error) => error.into_response(),
  }
}

async fn handle_generate_world(
  app_handle: &AppHandle,
  body: Result<Json<EnqueueImageToGaussianRequest>, JsonRejection>,
) -> Result<ControlSuccessResponse<EnqueuedTaskResponse>, ControlErrorResponse> {
  let request = validate(read_json_body(body)?)?;

  let app_env_configs = require_tauri_state::<AppEnvConfigs>(app_handle)?;
  let app_data_root = require_tauri_state::<AppDataRoot>(app_handle)?;
  let artcraft_usage_tracker = require_tauri_state::<ArtcraftUsageTracker>(app_handle)?;
  let task_database = require_tauri_state::<TaskDatabase>(app_handle)?;
  let storyteller_creds_manager = require_tauri_state::<StorytellerCredentialManager>(app_handle)?;
  let worldlabs_creds_manager = require_tauri_state::<WorldlabsCredentialManager>(app_handle)?;

  let success = handle_image_to_gaussian_request(
    request,
    app_handle,
    &app_data_root,
    &artcraft_usage_tracker,
    &task_database,
    &storyteller_creds_manager,
    &worldlabs_creds_manager,
    &app_env_configs,
  )
    .await
    .map_err(|err| generate_error_to_control_response(ENDPOINT, err))?;

  notify_frontend_of_enqueue_success(app_handle, &success);

  Ok(ControlSuccessResponse::new(
    EnqueuedTaskResponse::from_enqueue_success(&task_database, &success).await,
  ))
}

/// NB: A world job needs a model plus something to build the world from. Both are `Option` on the
/// command struct, so without this an empty body would reach the provider before failing.
fn validate(
  request: EnqueueImageToGaussianRequest,
) -> Result<EnqueueImageToGaussianRequest, ControlErrorResponse> {
  if request.model.is_none() {
    return Err(ControlErrorResponse::new(
      ControlErrorCode::BadRequest,
      "Field `model` is required.",
    ));
  }

  let has_prompt = request.prompt.as_deref().is_some_and(|prompt| !prompt.trim().is_empty());
  let has_images = request
    .image_media_tokens
    .as_ref()
    .is_some_and(|tokens| !tokens.is_empty());

  if !has_prompt && !has_images {
    return Err(ControlErrorResponse::new(
      ControlErrorCode::BadRequest,
      "Supply `prompt`, `image_media_tokens`, or both.",
    ));
  }

  Ok(request)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_empty_body_is_rejected() {
    let request: EnqueueImageToGaussianRequest =
      serde_json::from_str("{}").expect("an empty object should deserialize");

    assert_bad_request(validate(request));
  }

  #[test]
  fn test_a_model_with_neither_prompt_nor_images_is_rejected() {
    let request: EnqueueImageToGaussianRequest =
      serde_json::from_str(r#"{"model": "marble_0p1_mini", "prompt": "   "}"#)
        .expect("body should deserialize");

    assert_bad_request(validate(request));
  }

  #[test]
  fn test_a_model_with_a_prompt_or_images_is_accepted() {
    let prompt_request: EnqueueImageToGaussianRequest =
      serde_json::from_str(r#"{"model": "marble_0p1_mini", "prompt": "a quiet street"}"#)
        .expect("body should deserialize");
    let image_request: EnqueueImageToGaussianRequest =
      serde_json::from_str(r#"{"model": "marble_0p1_mini", "image_media_tokens": ["mf_test"]}"#)
        .expect("body should deserialize");

    assert!(validate(prompt_request).is_ok());
    assert!(validate(image_request).is_ok());
  }

  fn assert_bad_request(
    result: Result<EnqueueImageToGaussianRequest, ControlErrorResponse>,
  ) {
    let error = match result {
      Ok(_) => panic!("expected a rejection"),
      Err(error) => error,
    };

    assert_eq!(error.error.code, ControlErrorCode::BadRequest);
  }
}
