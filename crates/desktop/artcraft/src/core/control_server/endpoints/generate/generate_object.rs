use crate::core::commands::enqueue::image_to_object::enqueue_image_to_3d_object_command::{
  handle_request as handle_image_to_object_request, EnqueueImageTo3dObjectRequest,
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
use crate::core::state::provider_priority::ProviderPriorityStore;
use crate::core::state::task_database::TaskDatabase;
use crate::services::storyteller::state::storyteller_credential_manager::StorytellerCredentialManager;
use axum::extract::rejection::JsonRejection;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::Value;
use tauri::AppHandle;

const ENDPOINT: &str = "POST /v1/generate/object";

/// Every field the command's request struct understands, so a misspelled one is a `BAD_REQUEST`
/// instead of a silently dropped parameter (and a billed generation with the wrong inputs).
const KNOWN_FIELDS: &[&str] = &[
  "image_media_token",
  "model",
  "frontend_caller",
  "frontend_subscriber_id",
  "frontend_subscriber_payload",
];

/// `POST /v1/generate/object` — image to 3D mesh.
pub async fn post_generate_object_handler(
  State(app_handle): State<AppHandle>,
  body: Result<Json<Value>, JsonRejection>,
) -> Response {
  match handle_generate_object(&app_handle, body).await {
    Ok(response) => response.into_response(),
    Err(error) => error.into_response(),
  }
}

async fn handle_generate_object(
  app_handle: &AppHandle,
  body: Result<Json<Value>, JsonRejection>,
) -> Result<ControlSuccessResponse<EnqueuedTaskResponse>, ControlErrorResponse> {
  let request = validate(read_json_body_with_known_fields::<EnqueueImageTo3dObjectRequest>(body, KNOWN_FIELDS)?)?;

  let app_env_configs = require_tauri_state::<AppEnvConfigs>(app_handle)?;
  let app_data_root = require_tauri_state::<AppDataRoot>(app_handle)?;
  let artcraft_usage_tracker = require_tauri_state::<ArtcraftUsageTracker>(app_handle)?;
  let provider_priority_store = require_tauri_state::<ProviderPriorityStore>(app_handle)?;
  let task_database = require_tauri_state::<TaskDatabase>(app_handle)?;
  let storyteller_creds_manager = require_tauri_state::<StorytellerCredentialManager>(app_handle)?;

  let success = handle_image_to_object_request(
    request,
    app_handle,
    &app_env_configs,
    &app_data_root,
    &artcraft_usage_tracker,
    &provider_priority_store,
    &task_database,
    &storyteller_creds_manager,
  )
    .await
    .map_err(|err| generate_error_to_control_response(ENDPOINT, err))?;

  notify_frontend_of_enqueue_success(app_handle, &success);

  Ok(ControlSuccessResponse::new(
    EnqueuedTaskResponse::from_enqueue_success(&task_database, &success).await,
  ))
}

/// NB: Both fields are `Option` on the command struct, so an empty body deserializes cleanly and
/// would otherwise reach the provider before failing. Checking here is what makes an empty body a
/// `BAD_REQUEST` rather than a round trip to the upstream API.
fn validate(
  request: EnqueueImageTo3dObjectRequest,
) -> Result<EnqueueImageTo3dObjectRequest, ControlErrorResponse> {
  if request.model.is_none() {
    return Err(ControlErrorResponse::new(
      ControlErrorCode::BadRequest,
      "Field `model` is required.",
    ));
  }

  if request.image_media_token.is_none() {
    return Err(ControlErrorResponse::new(
      ControlErrorCode::BadRequest,
      "Field `image_media_token` is required.",
    ));
  }

  Ok(request)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_empty_body_is_rejected() {
    let request: EnqueueImageTo3dObjectRequest =
      serde_json::from_str("{}").expect("an empty object should deserialize");

    assert_bad_request(validate(request));
  }

  #[test]
  fn test_a_model_without_an_image_is_rejected() {
    let request: EnqueueImageTo3dObjectRequest =
      serde_json::from_str(r#"{"model": "hunyuan_3d_2_1"}"#).expect("body should deserialize");

    assert_bad_request(validate(request));
  }

  #[test]
  fn test_a_model_with_an_image_is_accepted() {
    let request: EnqueueImageTo3dObjectRequest = serde_json::from_str(r#"{
      "model": "hunyuan_3d_2_1",
      "image_media_token": "mf_test"
    }"#).expect("body should deserialize");

    assert!(validate(request).is_ok());
  }

  mod body_field_tests {
    use super::*;

    const GOOD_BODY: &str = r#"{"model": "hunyuan_3d_2_1", "image_media_token": "mf_test"}"#;
    const TYPO_BODY: &str = r#"{"model": "hunyuan_3d_2_1", "image_media_tokn": "mf_test"}"#;

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
          assert_eq!(error.error.message, "Unknown field(s): `image_media_tokn`.");
        }
      }
    }

    fn read_body(body: &str) -> Result<EnqueueImageTo3dObjectRequest, ControlErrorResponse> {
      let body: Value = serde_json::from_str(body).expect("the test body is valid JSON");

      read_json_body_with_known_fields::<EnqueueImageTo3dObjectRequest>(Ok(Json(body)), KNOWN_FIELDS)
    }
  }

  fn assert_bad_request(
    result: Result<EnqueueImageTo3dObjectRequest, ControlErrorResponse>,
  ) {
    let error = match result {
      Ok(_) => panic!("expected a rejection"),
      Err(error) => error,
    };

    assert_eq!(error.error.code, ControlErrorCode::BadRequest);
  }
}
