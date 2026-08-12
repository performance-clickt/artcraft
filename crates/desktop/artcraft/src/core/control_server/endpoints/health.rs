use crate::core::control_server::envelope::control_response::ControlSuccessResponse;
use crate::services::storyteller::state::storyteller_credential_manager::StorytellerCredentialManager;
use crate::version::ARTCRAFT_VERSION;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use log::warn;
use serde::Serialize;
use tauri::{AppHandle, Manager};

/// `GET /v1/health` — liveness plus the two facts a client needs before doing anything else.
#[derive(Serialize)]
pub struct HealthResponse {
  pub app_version: String,
  pub pid: u32,
  pub logged_in: bool,
}

pub async fn get_health_handler(State(app_handle): State<AppHandle>) -> Response {
  ControlSuccessResponse::new(HealthResponse {
    app_version: ARTCRAFT_VERSION.to_string(),
    pid: std::process::id(),
    logged_in: is_logged_in(&app_handle),
  }).into_response()
}

fn is_logged_in(app_handle: &AppHandle) -> bool {
  let creds_manager = app_handle.state::<StorytellerCredentialManager>();

  match creds_manager.get_credentials() {
    Ok(Some(credentials)) => !credentials.is_empty(),
    Ok(None) => false,
    Err(err) => {
      warn!("[ControlServer] Failed to read Storyteller credentials: {:?}", err);
      false
    }
  }
}
