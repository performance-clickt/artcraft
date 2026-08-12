use crate::core::control_server::envelope::control_response::{ControlErrorCode, ControlErrorResponse, ControlSuccessResponse};
use crate::core::control_server::require_tauri_state::require_tauri_state;
use crate::core::state::app_env_configs::app_env_configs::AppEnvConfigs;
use crate::core::state::app_preferences::app_preferences_manager::AppPreferencesManager;
use crate::core::state::app_preferences::preferred_download_directory::PreferredDownloadDirectory;
use crate::core::state::data_dir::app_data_root::AppDataRoot;
use crate::core::state::data_dir::trait_data_subdir::DataSubdir;
use crate::core::state::expanduser::expanduser;
use artcraft_client::error::client_error::ClientError;
use artcraft_client::error::storyteller_error::StorytellerError;
use artcraft_client::recipes::download_media_file::{download_media_file, DownloadMediaFileArgs, DownloadPath};
use axum::body::Bytes;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use log::{info, warn};
use serde_derive::{Deserialize, Serialize};
use std::io::ErrorKind;
use std::path::{Component, PathBuf};
use tauri::AppHandle;
use tokens::tokens::media_files::MediaFileToken;

const DOWNLOAD_FAILED_MESSAGE: &str = "Failed to fetch the media file from the ArtCraft API.";
const FILE_EXISTS_MESSAGE: &str = "A file for this media token already exists in the destination directory; move or delete it first.";
const DEST_DIR_NOT_ABSOLUTE_MESSAGE: &str = "`dest_dir` must be an absolute path (`~` is allowed).";
const DEST_DIR_TRAVERSAL_MESSAGE: &str = "`dest_dir` must not contain `..` path components.";
const DEST_DIR_UNRESOLVABLE_MESSAGE: &str = "`dest_dir` could not be resolved to a directory.";
const DEST_DIR_NOT_CREATABLE_MESSAGE: &str = "`dest_dir` could not be created.";

/// `POST /v1/media/download`
#[derive(Deserialize)]
pub struct DownloadMediaRequest {
  pub media_token: MediaFileToken,

  /// Absolute destination directory; `~` is expanded. Defaults to the same directory the app's
  /// own "download" action writes to (the user's configured download directory).
  pub dest_dir: Option<String>,
}

#[derive(Serialize)]
pub struct DownloadMediaResponse {
  /// Absolute path to the file that was written.
  pub path: String,

  pub filesize_bytes: usize,
}

/// NB: The body is read as raw bytes rather than through axum's `Json` extractor on purpose. An
/// extractor rejection is emitted by axum itself as bare text, which would be the one response on
/// this server that is not a control envelope; deserializing here keeps malformed JSON inside
/// `BAD_REQUEST`.
pub async fn download_media_handler(
  State(app_handle): State<AppHandle>,
  body: Bytes,
) -> Response {
  let request = match serde_json::from_slice::<DownloadMediaRequest>(&body) {
    Ok(request) => request,
    Err(err) => {
      return ControlErrorResponse::new(ControlErrorCode::BadRequest, err.to_string())
        .into_response();
    }
  };

  let app_env_configs = match require_tauri_state::<AppEnvConfigs>(&app_handle) {
    Ok(state) => state,
    Err(error) => return error.into_response(),
  };

  let app_data_root = match require_tauri_state::<AppDataRoot>(&app_handle) {
    Ok(state) => state,
    Err(error) => return error.into_response(),
  };

  let destination_directory = match resolve_destination_directory(
    request.dest_dir.as_deref(),
    read_preferred_download_directory(&app_handle).as_ref(),
    &app_data_root,
  ) {
    Ok(directory) => directory,
    Err(message) => {
      return ControlErrorResponse::new(ControlErrorCode::BadRequest, message).into_response();
    }
  };

  if let Err(err) = std::fs::create_dir_all(&destination_directory) {
    warn!("[ControlServer] Failed to create {:?}: {:?}", destination_directory, err);

    return ControlErrorResponse::new(ControlErrorCode::BadRequest, DEST_DIR_NOT_CREATABLE_MESSAGE)
      .into_response();
  }

  let result = download_media_file(DownloadMediaFileArgs {
    media_token: &request.media_token,
    api_host: &app_env_configs.storyteller_host,
    download_path: DownloadPath::Directory(&destination_directory),
  }).await;

  let result = match result {
    Ok(result) => result,
    Err(err) => {
      // NB: The token itself is safe to log; credentials are never part of this path.
      warn!(
        "[ControlServer] Failed to download media file {}: {:?}",
        request.media_token.as_str(),
        err,
      );

      // An already-written file is the caller's problem to resolve, not a backend failure: the
      // recipe refuses to truncate a file it did not create.
      if let StorytellerError::Client(ClientError::IoError(io_error)) = &err {
        if io_error.kind() == ErrorKind::AlreadyExists {
          return ControlErrorResponse::new(ControlErrorCode::BadRequest, FILE_EXISTS_MESSAGE)
            .into_response();
        }
      }

      return ControlErrorResponse::new(ControlErrorCode::UpstreamApiError, DOWNLOAD_FAILED_MESSAGE)
        .into_response();
    }
  };

  let Some(path) = result.downloaded_file_path.to_str() else {
    warn!("[ControlServer] Downloaded path is not valid UTF-8: {:?}", result.downloaded_file_path);

    return ControlErrorResponse::new(ControlErrorCode::Internal, DEST_DIR_UNRESOLVABLE_MESSAGE)
      .into_response();
  };

  info!("[ControlServer] Downloaded {} bytes to {:?}", result.filesize_bytes, path);

  ControlSuccessResponse::new(DownloadMediaResponse {
    path: path.to_string(),
    filesize_bytes: result.filesize_bytes,
  }).into_response()
}

/// Defaults to the user's configured download directory — the very directory the app's own
/// download action writes to — so an agent that passes nothing writes where the user expects.
/// Falls back to the app-data downloads directory when preferences are unreadable, exactly as
/// `PreferredDownloadDirectory::download_directory` does for an unresolvable system directory.
///
/// A caller-supplied directory must be absolute and free of `..`. NB: `..` is rejected before the
/// directory is created rather than canonicalized away, because canonicalization only works on
/// paths that already exist — and this endpoint creates the directory.
fn resolve_destination_directory(
  maybe_dest_dir: Option<&str>,
  maybe_preferred_directory: Option<&PreferredDownloadDirectory>,
  app_data_root: &AppDataRoot,
) -> Result<PathBuf, &'static str> {
  let Some(dest_dir) = maybe_dest_dir.map(str::trim).filter(|dir| !dir.is_empty()) else {
    return Ok(match maybe_preferred_directory {
      Some(preferred) => preferred.download_directory(app_data_root),
      None => app_data_root.downloads_dir().path().to_path_buf(),
    });
  };

  let expanded = expanduser(dest_dir)
      .map_err(|_| DEST_DIR_UNRESOLVABLE_MESSAGE)?;

  validate_destination_directory(&expanded)?;

  Ok(expanded)
}

/// NB: Unreadable preferences are not fatal — the caller falls back to the app-data downloads
/// directory rather than failing a download over a preference read.
fn read_preferred_download_directory(app_handle: &AppHandle) -> Option<PreferredDownloadDirectory> {
  let app_preferences = require_tauri_state::<AppPreferencesManager>(app_handle).ok()?;

  match app_preferences.get_clone() {
    Ok(preferences) => Some(preferences.preferred_download_directory),
    Err(err) => {
      warn!("[ControlServer] Failed to read app preferences: {:?}", err);
      None
    }
  }
}

fn validate_destination_directory(directory: &PathBuf) -> Result<(), &'static str> {
  if directory.components().any(|component| component == Component::ParentDir) {
    return Err(DEST_DIR_TRAVERSAL_MESSAGE);
  }

  if !directory.is_absolute() {
    return Err(DEST_DIR_NOT_ABSOLUTE_MESSAGE);
  }

  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_relative_destination_is_rejected() {
    let directory = PathBuf::from("relative/dir");

    assert_eq!(
      validate_destination_directory(&directory),
      Err(DEST_DIR_NOT_ABSOLUTE_MESSAGE),
    );
  }

  #[test]
  fn test_traversal_destination_is_rejected() {
    let directory = PathBuf::from("/Users/someone/../../etc");

    assert_eq!(
      validate_destination_directory(&directory),
      Err(DEST_DIR_TRAVERSAL_MESSAGE),
    );
  }

  #[test]
  fn test_plain_absolute_destination_is_accepted() {
    let directory = PathBuf::from("/Users/someone/Artcraft/downloads");

    assert_eq!(validate_destination_directory(&directory), Ok(()));
  }
}
