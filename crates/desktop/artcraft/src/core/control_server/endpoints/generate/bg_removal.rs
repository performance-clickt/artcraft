use crate::core::commands::enqueue::image_bg_removal::enqueue_image_bg_removal_command::{
  handle_request as handle_bg_removal_request, EnqueueImageBgRemovalCommand,
};
use crate::core::control_server::endpoints::generate::common::decode_image_input::reject_conflicting_image_input;
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
use crate::services::storyteller::state::storyteller_credential_manager::StorytellerCredentialManager;
use axum::extract::rejection::JsonRejection;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::Json;
use tauri::AppHandle;

const ENDPOINT: &str = "POST /v1/generate/bg_removal";

/// `POST /v1/generate/bg_removal`.
///
/// NB: This is the one command that already takes base64 natively (`base64_image`), including the
/// `data:` prefix handling, so the body needs no lowering — it is the command struct itself.
pub async fn post_bg_removal_handler(
  State(app_handle): State<AppHandle>,
  body: Result<Json<EnqueueImageBgRemovalCommand>, JsonRejection>,
) -> Response {
  match handle_bg_removal(&app_handle, body).await {
    Ok(response) => response.into_response(),
    Err(error) => error.into_response(),
  }
}

async fn handle_bg_removal(
  app_handle: &AppHandle,
  body: Result<Json<EnqueueImageBgRemovalCommand>, JsonRejection>,
) -> Result<ControlSuccessResponse<EnqueuedTaskResponse>, ControlErrorResponse> {
  let request = validate(read_json_body(body)?)?;

  let app_env_configs = require_tauri_state::<AppEnvConfigs>(app_handle)?;
  let app_data_root = require_tauri_state::<AppDataRoot>(app_handle)?;
  let artcraft_usage_tracker = require_tauri_state::<ArtcraftUsageTracker>(app_handle)?;
  let provider_priority_store = require_tauri_state::<ProviderPriorityStore>(app_handle)?;
  let task_database = require_tauri_state::<TaskDatabase>(app_handle)?;
  let storyteller_creds_manager = require_tauri_state::<StorytellerCredentialManager>(app_handle)?;

  let success = handle_bg_removal_request(
    &request,
    app_handle,
    &app_data_root,
    &app_env_configs,
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

/// NB: The command treats the two image fields as alternatives and picks one silently. Over HTTP
/// that would hide a caller's mistake, so supplying both is rejected here, as is supplying neither
/// (which is what an empty body is).
fn validate(
  request: EnqueueImageBgRemovalCommand,
) -> Result<EnqueueImageBgRemovalCommand, ControlErrorResponse> {
  reject_conflicting_image_input(
    "image_media_token",
    "base64_image",
    request.image_media_token.is_some(),
    request.base64_image.is_some(),
  )?;

  if request.image_media_token.is_none() && request.base64_image.is_none() {
    return Err(ControlErrorResponse::new(
      ControlErrorCode::BadRequest,
      "Supply either `image_media_token` or `base64_image`.",
    ));
  }

  Ok(request)
}

#[cfg(test)]
mod tests {
  use super::*;
  use tokens::tokens::media_files::MediaFileToken;

  const PNG_PIXEL_BASE64: &str = "iVBORw0KGgo=";

  #[test]
  fn test_empty_body_is_rejected() {
    let request: EnqueueImageBgRemovalCommand =
      serde_json::from_str("{}").expect("an empty object should deserialize");

    assert_bad_request(validate(request));
  }

  #[test]
  fn test_both_image_inputs_together_are_rejected() {
    let request = EnqueueImageBgRemovalCommand {
      image_media_token: Some(MediaFileToken::new_from_str("mf_test")),
      base64_image: Some(PNG_PIXEL_BASE64.to_string()),
      frontend_caller: None,
      frontend_subscriber_id: None,
      frontend_subscriber_payload: None,
    };

    assert_bad_request(validate(request));
  }

  #[test]
  fn test_either_image_input_alone_is_accepted() {
    let token_request: EnqueueImageBgRemovalCommand =
      serde_json::from_str(r#"{"image_media_token": "mf_test"}"#).expect("body should deserialize");
    let base64_request: EnqueueImageBgRemovalCommand =
      serde_json::from_str(&format!(r#"{{"base64_image": "{}"}}"#, PNG_PIXEL_BASE64))
        .expect("body should deserialize");

    assert!(validate(token_request).is_ok());
    assert!(validate(base64_request).is_ok());
  }

  fn assert_bad_request(
    result: Result<EnqueueImageBgRemovalCommand, ControlErrorResponse>,
  ) {
    let error = match result {
      Ok(_) => panic!("expected a rejection"),
      Err(error) => error,
    };

    assert_eq!(error.error.code, ControlErrorCode::BadRequest);
  }
}
