use crate::core::commands::enqueue::image_to_gaussian::enqueue_image_to_gaussian_command::{
  handle_request as handle_image_to_gaussian_request, EnqueueImageToGaussianRequest,
};
use crate::core::control_server::endpoints::generate::common::enqueued_task_response::{
  notify_frontend_of_enqueue_success, EnqueuedTaskResponse,
};
use crate::core::control_server::endpoints::generate::common::generate_error_mapping::generate_error_to_control_response;
use crate::core::control_server::endpoints::generate::common::known_fields::read_json_body_with_known_fields;
use crate::core::control_server::require_tauri_state::require_tauri_state;
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
use serde_json::Value;
use tauri::AppHandle;

const ENDPOINT: &str = "POST /v1/generate/world";

/// Every field the command's request struct understands, so a misspelled one is a `BAD_REQUEST`
/// instead of a silently dropped parameter (and a billed generation with the wrong inputs).
const KNOWN_FIELDS: &[&str] = &[
  "model",
  "provider",
  "prompt",
  "image_media_tokens",
  "frontend_caller",
  "frontend_subscriber_id",
  "frontend_subscriber_payload",
];

/// `POST /v1/generate/world` — image to 3D gaussian splat ("world").
pub async fn post_generate_world_handler(
  State(app_handle): State<AppHandle>,
  body: Result<Json<Value>, JsonRejection>,
) -> Response {
  match handle_generate_world(&app_handle, body).await {
    Ok(response) => response.into_response(),
    Err(error) => error.into_response(),
  }
}

async fn handle_generate_world(
  app_handle: &AppHandle,
  body: Result<Json<Value>, JsonRejection>,
) -> Result<ControlSuccessResponse<EnqueuedTaskResponse>, ControlErrorResponse> {
  let request = validate(read_json_body_with_known_fields::<EnqueueImageToGaussianRequest>(body, KNOWN_FIELDS)?)?;

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

  mod body_field_tests {
    use super::*;

    const GOOD_BODY: &str = r#"{"model": "marble_0p1_mini", "prompt": "a quiet street"}"#;
    const TYPO_BODY: &str = r#"{"model": "marble_0p1_mini", "promt": "a quiet street"}"#;

    #[test]
    fn test_a_realistic_body_is_accepted() {
      assert!(read_body(GOOD_BODY).is_ok());
    }

    #[test]
    fn test_a_misspelled_field_is_rejected_instead_of_dropped() {
      match read_body(TYPO_BODY) {
        Ok(_) => panic!("expected a rejection"),
        Err(error) => {
          assert_eq!(error.error.code, ControlErrorCode::BadRequest);
          assert_eq!(error.error.message, "Unknown field(s): `promt`.");
        }
      }
    }

    fn read_body(body: &str) -> Result<EnqueueImageToGaussianRequest, ControlErrorResponse> {
      let body: Value = serde_json::from_str(body).expect("the test body is valid JSON");

      read_json_body_with_known_fields::<EnqueueImageToGaussianRequest>(Ok(Json(body)), KNOWN_FIELDS)
    }
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
