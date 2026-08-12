use crate::core::control_server::state::control_server_settings::ControlServerSettings;
use crate::core::state::data_dir::app_data_root::AppDataRoot;
use crate::core::state::data_dir::trait_data_subdir::DataSubdir;
use chrono::Utc;
use errors::AnyhowResult;
use serde::Serialize;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

const CONTROL_STATE_FILE_NAME: &str = "control_server.json";
const CONTROL_STATE_FILE_VERSION: u32 = 1;

#[cfg(unix)]
const CONTROL_STATE_FILE_MODE: u32 = 0o600;

/// The discovery file at `~/Artcraft/state/control_server.json` that the MCP server reads to
/// find this launch's control server. NB: It carries the bearer token, so it is owner-only.
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

  let mut file = open_owner_only_file(&path)?;
  file.write_all(&contents)?;
  file.flush()?;

  restrict_existing_file_to_owner_only(&path)?;

  Ok(path)
}

/// Creates the file with owner-only permissions so the token is never briefly world-readable.
fn open_owner_only_file(path: &Path) -> AnyhowResult<File> {
  let mut options = OpenOptions::new();
  options.write(true).create(true).truncate(true);

  #[cfg(unix)]
  {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(CONTROL_STATE_FILE_MODE);
  }

  Ok(options.open(path)?)
}

/// `OpenOptions::mode` only applies when the file is created, so a file left behind by an
/// earlier launch keeps its old mode. Re-apply it every launch.
#[cfg(unix)]
fn restrict_existing_file_to_owner_only(path: &Path) -> AnyhowResult<()> {
  use std::os::unix::fs::PermissionsExt;
  std::fs::set_permissions(path, std::fs::Permissions::from_mode(CONTROL_STATE_FILE_MODE))?;
  Ok(())
}

#[cfg(not(unix))]
fn restrict_existing_file_to_owner_only(_path: &Path) -> AnyhowResult<()> {
  Ok(())
}
