use crate::core::control_server::envelope::control_response::{ControlErrorCode, ControlErrorResponse, ControlSuccessResponse};
use crate::services::storyteller::state::storyteller_credential_manager::StorytellerCredentialManager;
use crate::version::ARTCRAFT_VERSION;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use log::warn;
use serde::Serialize;
use tauri::{AppHandle, Manager};

const CREDENTIALS_UNAVAILABLE_MESSAGE: &str = "Credential state is unavailable.";

/// `GET /v1/health` — liveness plus the two facts a client needs before doing anything else.
#[derive(Serialize)]
pub struct HealthResponse {
  pub app_version: String,
  pub pid: u32,
  pub logged_in: bool,
}

pub async fn get_health_handler(State(app_handle): State<AppHandle>) -> Response {
  let Some(logged_in) = maybe_is_logged_in(&app_handle) else {
    warn!("[ControlServer] Storyteller credential manager is not managed by Tauri.");

    return ControlErrorResponse::new(ControlErrorCode::Internal, CREDENTIALS_UNAVAILABLE_MESSAGE)
      .into_response();
  };

  ControlSuccessResponse::new(HealthResponse {
    app_version: ARTCRAFT_VERSION.to_string(),
    pid: std::process::id(),
    logged_in,
  }).into_response()
}

/// `None` when the credential manager is not managed — the caller answers with an error envelope
/// instead of panicking the request task, which would drop the connection with no response.
fn maybe_is_logged_in(app_handle: &AppHandle) -> Option<bool> {
  let creds_manager = app_handle.try_state::<StorytellerCredentialManager>()?;

  match creds_manager.get_credentials() {
    // NB: Only the session cookie means "signed in". `avt` is also present for anonymous
    // visitors, so credential *presence* would report a never-signed-in user as logged in.
    Ok(Some(credentials)) => Some(credentials.session.is_some()),
    Ok(None) => Some(false),
    Err(err) => {
      warn!("[ControlServer] Failed to read Storyteller credentials: {:?}", err);
      Some(false)
    }
  }
}
