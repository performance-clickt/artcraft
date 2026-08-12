use crate::core::control_server::envelope::control_response::{ControlErrorCode, ControlErrorResponse};
use log::error;
use tauri::{AppHandle, Manager, State as TauriState};

/// Fetches a Tauri-managed state value for a control server request.
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
        "[ControlServer] Tauri state {} is not managed; cannot serve generation request.",
        std::any::type_name::<T>(),
      );

      Err(ControlErrorResponse::new(
        ControlErrorCode::Internal,
        "Application state is unavailable.",
      ))
    }
  }
}
