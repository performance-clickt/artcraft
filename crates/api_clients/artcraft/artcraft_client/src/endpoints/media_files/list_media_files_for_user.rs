use crate::credentials::storyteller_credential_set::StorytellerCredentialSet;
use crate::error::storyteller_error::StorytellerError;
use crate::utils::api_host::ApiHost;
use crate::utils::basic_json_get_request::basic_json_get_request;
use artcraft_api_defs::common::responses::media_links::MediaLinks;
use chrono::{DateTime, Utc};
use enums::by_table::media_files::media_file_class::MediaFileClass;
use enums::by_table::media_files::media_file_engine_category::MediaFileEngineCategory;
use enums::by_table::media_files::media_file_type::MediaFileType;
use serde_derive::Deserialize;
use tokens::tokens::media_files::MediaFileToken;
use url::form_urlencoded::Serializer as QueryStringSerializer;
use url::Url;

pub const LIST_MEDIA_FILES_FOR_USER_URL_PATH_PREFIX: &str = "/v1/media_files/list/user";

/// Only ever used as a throwaway base for percent-encoding a path segment.
const PATH_ENCODING_PLACEHOLDER_URL: &str = "http://placeholder.invalid";

pub struct ListMediaFilesForUserArgs<'a> {
  /// The username whose library is listed. For the signed-in user's own library this is the
  /// `username` from `get_session_info`, which is what the app's "My Library" view uses.
  pub username: &'a str,

  /// Zero-based page index. The backend paginates this route by page, not by opaque cursor.
  pub page_index: usize,

  pub page_size: usize,

  /// The desktop app passes `true` here; without it the backend hides files the user uploaded
  /// themselves, so the listing would not match what "My Library" shows.
  pub include_user_uploads: bool,
}

pub async fn list_media_files_for_user(
  api_host: &ApiHost,
  maybe_creds: Option<&StorytellerCredentialSet>,
  args: ListMediaFilesForUserArgs<'_>,
) -> Result<ListMediaFilesForUserSuccessResponse, StorytellerError> {
  let url = list_media_files_for_user_route(&args);

  Ok(basic_json_get_request(
    api_host,
    &url,
    maybe_creds,
  ).await?)
}

/// NB: The canonical response type is defined server-side in `storyteller-web` and is
/// `Serialize`-only, so this module declares the `Deserialize` subset the client consumes.
/// Serde ignores unknown fields, so new server fields do not break this.
#[derive(Deserialize, Debug)]
pub struct ListMediaFilesForUserSuccessResponse {
  pub success: bool,

  #[serde(default)]
  pub results: Vec<MediaFileForUserListItem>,

  /// Absent on some error-ish payloads, so it is optional here rather than required.
  pub pagination: Option<MediaFilePaginationPage>,
}

#[derive(Deserialize, Debug)]
pub struct MediaFileForUserListItem {
  pub token: MediaFileToken,
  pub media_class: MediaFileClass,
  pub media_type: MediaFileType,
  pub maybe_engine_category: Option<MediaFileEngineCategory>,
  pub media_links: MediaLinks,
  pub maybe_title: Option<String>,
  pub is_user_upload: bool,
  pub created_at: DateTime<Utc>,
}

#[derive(Deserialize, Debug)]
pub struct MediaFilePaginationPage {
  pub current: usize,
  pub total_page_count: usize,
}

fn list_media_files_for_user_route(args: &ListMediaFilesForUserArgs) -> String {
  let encoded_username = encode_path_segment(args.username);

  let query_string = QueryStringSerializer::new(String::new())
      .append_pair("page_index", &args.page_index.to_string())
      .append_pair("page_size", &args.page_size.to_string())
      .append_pair("include_user_uploads", &args.include_user_uploads.to_string())
      .finish();

  format!(
    "{}/{}?{}",
    LIST_MEDIA_FILES_FOR_USER_URL_PATH_PREFIX,
    encoded_username,
    query_string,
  )
}

/// Percent-encodes a single path segment. NB: `form_urlencoded` is deliberately NOT used here —
/// it encodes a space as `+`, which is a literal plus inside a URL path, so a username containing
/// a space would resolve to a different (or missing) user.
fn encode_path_segment(segment: &str) -> String {
  let mut url = Url::parse(PATH_ENCODING_PLACEHOLDER_URL)
      .expect("the placeholder URL is a valid, static base URL");

  url.path_segments_mut()
      .expect("an http URL always supports path segments")
      .push(segment);

  url.path()
      .trim_start_matches('/')
      .to_string()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_route_includes_pagination_and_uploads_flag() {
    let route = list_media_files_for_user_route(&ListMediaFilesForUserArgs {
      username: "someuser",
      page_index: 2,
      page_size: 25,
      include_user_uploads: true,
    });

    assert_eq!(
      route,
      "/v1/media_files/list/user/someuser?page_index=2&page_size=25&include_user_uploads=true",
    );
  }

  #[test]
  fn test_route_escapes_the_username() {
    let route = list_media_files_for_user_route(&ListMediaFilesForUserArgs {
      username: "a b/../c",
      page_index: 0,
      page_size: 10,
      include_user_uploads: false,
    });

    assert!(
      route.starts_with("/v1/media_files/list/user/a%20b%2F..%2Fc?"),
      "unexpected route: {}",
      route,
    );
  }
}
