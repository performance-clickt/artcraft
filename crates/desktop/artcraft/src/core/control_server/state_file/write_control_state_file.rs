use crate::core::control_server::state::control_server_settings::ControlServerSettings;
use crate::core::state::data_dir::app_data_root::AppDataRoot;
use crate::core::state::data_dir::trait_data_subdir::DataSubdir;
use chrono::Utc;
use errors::AnyhowResult;
use serde::Serialize;
use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

const CONTROL_STATE_FILE_NAME: &str = "control_server.json";
const CONTROL_STATE_FILE_VERSION: u32 = 1;

#[cfg(unix)]
const CONTROL_STATE_FILE_MODE: u32 = 0o600;

/// The discovery file at `~/Artcraft/state/control_server.json` that the MCP server reads to
/// find this launch's control server. NB: It carries the bearer token, so it is owner-only.
/// Owner-only is enforced on unix; on other platforms the file inherits the parent directory's
/// ACL and a warning is logged (see `warn_if_owner_only_is_unsupported`).
#[derive(Serialize)]
struct ControlStateFile {
  version: u32,
  pid: u32,
  port: u16,
  token: String,
  started_at: String,
}

pub fn write_control_state_file(
  app_data_root: &AppDataRoot,
  settings: &ControlServerSettings,
) -> AnyhowResult<PathBuf> {
  let path = app_data_root.state_dir().path().join(CONTROL_STATE_FILE_NAME);

  let state_file = ControlStateFile {
    version: CONTROL_STATE_FILE_VERSION,
    pid: std::process::id(),
    port: settings.port(),
    token: settings.token().to_string(),
    started_at: Utc::now().to_rfc3339(),
  };

  let contents = serde_json::to_vec_pretty(&state_file)?;

  let mut file = create_owner_only_file(&path)?;
  file.write_all(&contents)?;
  file.flush()?;

  warn_if_owner_only_is_unsupported(&path);

  Ok(path)
}

/// Replaces any previous file rather than truncating it in place, so the token is never written
/// into a file this launch does not own. `OpenOptions::mode` applies only on creation, so an
/// existing file left behind with a loose mode would keep it while already holding the token,
/// and an existing symlink would redirect both the write and the mode change to its target.
/// Unlinking first and creating exclusively closes both.
fn create_owner_only_file(path: &Path) -> AnyhowResult<File> {
  remove_file_if_present(path)?;

  let mut options = OpenOptions::new();
  options.write(true).create_new(true);

  #[cfg(unix)]
  {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(CONTROL_STATE_FILE_MODE);
  }

  Ok(options.open(path)?)
}

/// Removes the path itself, not a symlink's target, and treats "already gone" as success.
fn remove_file_if_present(path: &Path) -> AnyhowResult<()> {
  match std::fs::remove_file(path) {
    Ok(()) => Ok(()),
    Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
    Err(err) => Err(err.into()),
  }
}

#[cfg(unix)]
fn warn_if_owner_only_is_unsupported(_path: &Path) {}

/// The owner-only guarantee is unix-only: there is no ACL handling here yet, so say so out loud
/// rather than silently leaving the token readable by whoever the inherited ACL allows.
#[cfg(not(unix))]
fn warn_if_owner_only_is_unsupported(path: &Path) {
  use log::warn;

  warn!(
    "[ControlServer] {:?} holds the control server bearer token but cannot be restricted to the \
     current user on this platform; it inherits the parent directory's permissions.",
    path,
  );
}
