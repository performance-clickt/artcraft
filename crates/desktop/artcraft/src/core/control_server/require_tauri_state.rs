use crate::core::control_server::envelope::control_response::{ControlErrorCode, ControlErrorResponse};
use log::error;
use tauri::{AppHandle, Manager, State as TauriState};

/// Fetches a Tauri-managed state value for a control server request.
///
/// This is the single unmanaged-state policy for every control endpoint: one code, one message,
/// one log line. Hand-rolling `try_state` per endpoint is what let the same failure answer with
/// five different messages.
///
/// NB: `try_state` (not `state`) is deliberate — `state` panics when the type is not managed, and
/// a panic inside an axum handler task drops the connection with no response at all. An unmanaged
/// state is a programming error, so it answers `INTERNAL` and names the type in the log.
pub fn require_tauri_state<T: Send + Sync + 'static>(
  app_handle: &AppHandle,
) -> Result<TauriState<'_, T>, ControlErrorResponse> {
  match app_handle.try_state::<T>() {
    Some(state) => Ok(state),
    None => {
      error!(
        "[ControlServer] Tauri state {} is not managed; cannot serve the request.",
        std::any::type_name::<T>(),
      );

      Err(ControlErrorResponse::new(
        ControlErrorCode::Internal,
        "Application state is unavailable.",
      ))
    }
  }
}
