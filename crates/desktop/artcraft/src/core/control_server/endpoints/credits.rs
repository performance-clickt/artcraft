use crate::core::control_server::envelope::control_response::{ControlErrorCode, ControlErrorResponse, ControlSuccessResponse};
use crate::core::state::app_env_configs::app_env_configs::AppEnvConfigs;
use crate::services::storyteller::commands::storyteller_get_credits_command::storyteller_get_credits_command;
use crate::services::storyteller::state::storyteller_credential_manager::StorytellerCredentialManager;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use log::warn;
use tauri::{AppHandle, Manager};

const CONFIGS_UNAVAILABLE_MESSAGE: &str = "App environment configuration is unavailable.";
const CREDENTIALS_UNAVAILABLE_MESSAGE: &str = "Credential state is unavailable.";
const NOT_LOGGED_IN_MESSAGE: &str = "ArtCraft is not signed in. Sign in from the app to read credits.";
const EMPTY_PAYLOAD_MESSAGE: &str = "The credits command returned no payload.";

/// `GET /v1/credits` — the balance an agent checks before committing to a generation. Proxies the
/// same command the app UI reads, so the two can never disagree.
pub async fn get_credits_handler(State(app_handle): State<AppHandle>) -> Response {
  // NB: `try_state` rather than `state` — a missing managed type must answer with an error
  // envelope, not panic the request task and drop the connection with no response at all.
  let Some(app_env_configs) = app_handle.try_state::<AppEnvConfigs>() else {
    warn!("[ControlServer] App environment configs are not managed by Tauri.");

    return ControlErrorResponse::new(ControlErrorCode::Internal, CONFIGS_UNAVAILABLE_MESSAGE)
      .into_response();
  };

  let Some(creds_manager) = app_handle.try_state::<StorytellerCredentialManager>() else {
    warn!("[ControlServer] Storyteller credential manager is not managed by Tauri.");

    return ControlErrorResponse::new(ControlErrorCode::Internal, CREDENTIALS_UNAVAILABLE_MESSAGE)
      .into_response();
  };

  // Checked up front so a signed-out app gets the actionable `NOT_LOGGED_IN` code instead of
  // whatever opaque rejection the backend returns for an anonymous credits read.
  if !is_logged_in(&creds_manager) {
    return ControlErrorResponse::new(ControlErrorCode::NotLoggedIn, NOT_LOGGED_IN_MESSAGE)
      .into_response();
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

/// NB: Only the session cookie means "signed in". `avt` is also present for anonymous visitors,
/// so credential *presence* would report a never-signed-in user as logged in.
fn is_logged_in(creds_manager: &StorytellerCredentialManager) -> bool {
  match creds_manager.get_credentials() {
    Ok(Some(credentials)) => credentials.session.is_some(),
    Ok(None) => false,
    Err(err) => {
      warn!("[ControlServer] Failed to read Storyteller credentials: {:?}", err);
      false
    }
  }
}
