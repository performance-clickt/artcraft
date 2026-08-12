use crate::core::control_server::envelope::control_response::{ControlErrorCode, ControlErrorResponse};
use crate::core::control_server::require_tauri_state::require_tauri_state;
use crate::services::storyteller::state::storyteller_credential_manager::StorytellerCredentialManager;
use artcraft_client::credentials::storyteller_credential_set::StorytellerCredentialSet;
use log::warn;
use tauri::AppHandle;

pub const NOT_LOGGED_IN_MESSAGE: &str =
  "No ArtCraft account is signed in. Sign in inside the app first.";

/// Reads the app's stored credentials, requiring a real signed-in session.
///
/// This is the single sign-in policy for the control endpoints, so every one of them answers a
/// signed-out app with the same code and the same actionable message.
///
/// NB: Only the session cookie means "signed in" — the `avt` visitor cookie is present for
/// anonymous users too, so credential *presence* would report a never-signed-in user as logged in.
/// The webview cookie jar is never parsed.
pub fn require_signed_in_credentials(
  app_handle: &AppHandle,
) -> Result<StorytellerCredentialSet, ControlErrorResponse> {
  let credential_manager = require_tauri_state::<StorytellerCredentialManager>(app_handle)?;

  let maybe_credentials = match credential_manager.get_credentials() {
    Ok(maybe_credentials) => maybe_credentials,
    Err(err) => {
      warn!("[ControlServer] Failed to read Storyteller credentials: {:?}", err);
      None
    }
  };

  match maybe_credentials {
    Some(credentials) if credentials.session.is_some() => Ok(credentials),
    _ => Err(ControlErrorResponse::new(
      ControlErrorCode::NotLoggedIn,
      NOT_LOGGED_IN_MESSAGE,
    )),
  }
}
