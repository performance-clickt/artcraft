use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use log::{info, warn};
use reqwest::Client;
use url::Url;

use tokens::tokens::media_files::MediaFileToken;

use crate::endpoints::media_files::get_media_file::{get_media_file, GetMediaFileSuccessResponse};
use crate::error::api_error::ApiError;
use crate::error::client_error::ClientError;
use crate::error::storyteller_error::StorytellerError;
use crate::utils::api_host::ApiHost;

const FALLBACK_FILE_EXTENSION: &str = "bin";
const MAX_FILE_EXTENSION_LENGTH: usize = 8;

/// Extensions the OS (or a shell) may treat as runnable.
const EXECUTABLE_FILE_EXTENSIONS: &[&str] = &[
  "apk", "app", "bat", "cmd", "com", "dll", "exe", "jar", "js", "msi", "ps1", "scr", "sh", "so",
];

pub struct DownloadMediaFileArgs<'a, P: AsRef<Path>> {
  pub media_token: &'a MediaFileToken,
  pub api_host: &'a ApiHost,
  pub download_path: DownloadPath<P>,
}

pub enum DownloadPath<P: AsRef<Path>> {
  /// Write the file to this exact path, replacing an existing file. The caller chose the name,
  /// so the caller owns vetting it.
  ExactFilename(P),
  /// Write the file into this directory, generating a filename from the CDN URL extension.
  /// NB: An existing file is never replaced on this path — the name is derived from a remote URL,
  /// so silently truncating whatever already sits there is not the caller's decision.
  Directory(P),
}

pub struct DownloadMediaFileResult {
  /// The path the file was written to.
  pub downloaded_file_path: PathBuf,

  /// The full media file response from the API.
  pub media_file_response: GetMediaFileSuccessResponse,

  /// Size of downloaded file in bytes.
  pub filesize_bytes: usize,
}

pub async fn download_media_file<P: AsRef<Path>>(
  args: DownloadMediaFileArgs<'_, P>,
) -> Result<DownloadMediaFileResult, StorytellerError> {
  let DownloadMediaFileArgs {
    media_token,
    api_host,
    download_path
  } = args;

  // 1. Fetch media file info from the API.
  let response = get_media_file(api_host, media_token).await?;
  let media_class = &response.media_file.media_class;
  let cdn_url = &response.media_file.media_links.cdn_url;

  info!("Downloading media file {} of class {} from CDN: {}",
    media_token.as_str(), media_class.to_str(), cdn_url);

  // 2. Determine the output file path.
  let (output_path, refuse_existing_file) = match &download_path {
    DownloadPath::ExactFilename(path) => (path.as_ref().to_path_buf(), false),
    DownloadPath::Directory(dir) => {
      let filename = derive_filename_from_url(cdn_url, &media_token);
      (dir.as_ref().join(filename), true)
    }
  };

  // 3. Download the bytes from the CDN.
  let bytes = download_bytes(cdn_url).await?;

  // 4. Write to disk.
  let mut file = OpenOptions::new()
    .write(true)
    .create(true)
    .create_new(refuse_existing_file)
    .truncate(!refuse_existing_file)
    .open(&output_path)
    .map_err(|err| StorytellerError::Client(ClientError::IoError(err)))?;

  file.write_all(&bytes)
    .map_err(|err| StorytellerError::Client(ClientError::IoError(err)))?;

  file.flush()
    .map_err(|err| StorytellerError::Client(ClientError::IoError(err)))?;

  info!("Downloaded {} bytes to {:?}", bytes.len(), output_path);

  Ok(DownloadMediaFileResult {
    downloaded_file_path: output_path,
    media_file_response: response,
    filesize_bytes: bytes.len(),
  })
}

// ── Helpers ──

async fn download_bytes(url: &Url) -> Result<Vec<u8>, StorytellerError> {
  let client = Client::builder()
    .gzip(true)
    .build()
    .map_err(|err| StorytellerError::Client(ClientError::ReqwestError(err)))?;

  let response = client.get(url.as_str())
    .send()
    .await
    .map_err(|err| StorytellerError::Api(ApiError::OtherReqwestError(err)))?;

  let status_code = response.status();

  if !status_code.is_success() {
    let body = response.text().await.unwrap_or_else(|err| {
      warn!("Failed to retrieve response body: {}", err);
      "".to_string()
    });
    return Err(StorytellerError::Api(ApiError::UncategorizedBadResponseWithStatusAndBody {
      status_code,
      body,
    }));
  }

  let bytes = response.bytes()
    .await
    .map_err(|err| StorytellerError::Client(ClientError::ReqwestError(err)))?;

  Ok(bytes.to_vec())
}

/// Derive a filename from the CDN URL's path extension, falling back to the media token.
///
/// NB: The extension comes off a remote URL, so it is vetted rather than trusted: anything that is
/// not a short alphanumeric extension — and anything that names an executable type — becomes
/// `bin`. Writing `mf_xxx.sh` into a directory the caller then browses is not a download.
fn derive_filename_from_url(url: &Url, media_token: &MediaFileToken) -> String {
  let path = url.path();
  let maybe_extension = Path::new(path)
    .extension() // NB: without dot '.'
    .and_then(|ext| ext.to_str())
    .filter(|ext| is_safe_file_extension(ext));

  format!("{}.{}", media_token.as_str(), maybe_extension.unwrap_or(FALLBACK_FILE_EXTENSION))
}

fn is_safe_file_extension(extension: &str) -> bool {
  if extension.is_empty() || extension.len() > MAX_FILE_EXTENSION_LENGTH {
    return false;
  }

  if !extension.chars().all(|character| character.is_ascii_alphanumeric()) {
    return false;
  }

  let lowercased = extension.to_ascii_lowercase();

  !EXECUTABLE_FILE_EXTENSIONS.contains(&lowercased.as_str())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn derive_filename_from_url_extracts_extension() {
    let url = Url::parse("https://cdn.example.com/files/abc123.png").unwrap();
    let token = MediaFileToken::new_from_str("mf_test123");
    assert_eq!(derive_filename_from_url(&url, &token), "mf_test123.png");
  }

  #[test]
  fn derive_filename_from_url_handles_no_extension() {
    let url = Url::parse("https://cdn.example.com/files/abc123").unwrap();
    let token = MediaFileToken::new_from_str("mf_test123");
    assert_eq!(derive_filename_from_url(&url, &token), "mf_test123.bin");
  }

  #[test]
  fn derive_filename_from_url_rejects_executable_extensions() {
    let url = Url::parse("https://cdn.example.com/files/abc123.sh").unwrap();
    let token = MediaFileToken::new_from_str("mf_test123");
    assert_eq!(derive_filename_from_url(&url, &token), "mf_test123.bin");
  }

  #[test]
  fn derive_filename_from_url_rejects_a_non_alphanumeric_extension() {
    let url = Url::parse("https://cdn.example.com/files/abc123.p%20ng").unwrap();
    let token = MediaFileToken::new_from_str("mf_test123");
    assert_eq!(derive_filename_from_url(&url, &token), "mf_test123.bin");
  }

  #[test]
  fn derive_filename_from_url_handles_query_params() {
    let url = Url::parse("https://cdn.example.com/files/abc123.mp4?token=xyz").unwrap();
    let token = MediaFileToken::new_from_str("mf_test123");
    assert_eq!(derive_filename_from_url(&url, &token), "mf_test123.mp4");
  }
}
