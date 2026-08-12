use crate::core::control_server::envelope::control_response::{ControlErrorCode, ControlErrorResponse, ControlSuccessResponse};
use crate::core::control_server::require_signed_in_credentials::require_signed_in_credentials;
use crate::core::control_server::require_tauri_state::require_tauri_state;
use crate::core::state::app_env_configs::app_env_configs::AppEnvConfigs;
use crate::services::storyteller::commands::storyteller_get_credits_command::storyteller_get_credits_command;
use crate::services::storyteller::state::storyteller_credential_manager::StorytellerCredentialManager;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use log::warn;
use tauri::AppHandle;

const EMPTY_PAYLOAD_MESSAGE: &str = "The credits command returned no payload.";

/// `GET /v1/credits` — the balance an agent checks before committing to a generation. Proxies the
/// same command the app UI reads, so the two can never disagree.
pub async fn get_credits_handler(State(app_handle): State<AppHandle>) -> Response {
  let app_env_configs = match require_tauri_state::<AppEnvConfigs>(&app_handle) {
    Ok(state) => state,
    Err(error) => return error.into_response(),
  };

  let creds_manager = match require_tauri_state::<StorytellerCredentialManager>(&app_handle) {
    Ok(state) => state,
    Err(error) => return error.into_response(),
  };

  // Checked up front so a signed-out app gets the actionable `NOT_LOGGED_IN` code instead of
  // whatever opaque rejection the backend returns for an anonymous credits read.
  if let Err(error) = require_signed_in_credentials(&app_handle) {
    return error.into_response();
  }

  match storyteller_get_credits_command(app_env_configs, creds_manager).await {
    Ok(success) => match success.payload {
      Some(payload) => ControlSuccessResponse::new(payload).into_response(),
      None => {
        warn!("[ControlServer] Credits command succeeded with an empty payload.");

        ControlErrorResponse::new(ControlErrorCode::Internal, EMPTY_PAYLOAD_MESSAGE)
          .into_response()
      }
    },
    Err(failure) => {
      let message = failure
        .error_message
        .unwrap_or_else(|| "Failed to read credits.".to_string());

      warn!("[ControlServer] Credits command failed: {}", message);

      ControlErrorResponse::new(ControlErrorCode::UpstreamApiError, message).into_response()
    }
  }
}
