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

pub const SEARCH_SESSION_MEDIA_FILES_URL_PATH: &str = "/v1/media_files/search_session";

/// Searches the signed-in user's own media files. NB: The backend caps this search at a fixed
/// page of results and exposes no pagination, so callers cannot page past the first page.
pub async fn search_session_media_files(
  api_host: &ApiHost,
  maybe_creds: Option<&StorytellerCredentialSet>,
  search_term: &str,
) -> Result<SearchSessionMediaFilesSuccessResponse, StorytellerError> {
  let url = search_session_media_files_route(search_term);

  Ok(basic_json_get_request(
    api_host,
    &url,
    maybe_creds,
  ).await?)
}

/// NB: The canonical response type is defined server-side in `storyteller-web` and is
/// `Serialize`-only, so this module declares the `Deserialize` subset the client consumes.
#[derive(Deserialize, Debug)]
pub struct SearchSessionMediaFilesSuccessResponse {
  pub success: bool,

  #[serde(default)]
  pub results: Vec<SearchSessionMediaFileListItem>,
}

#[derive(Deserialize, Debug)]
pub struct SearchSessionMediaFileListItem {
  pub token: MediaFileToken,
  pub media_class: MediaFileClass,
  pub media_type: MediaFileType,
  pub maybe_engine_category: Option<MediaFileEngineCategory>,
  pub media_links: MediaLinks,
  pub maybe_title: Option<String>,
  pub is_user_upload: bool,
  pub created_at: DateTime<Utc>,
}

fn search_session_media_files_route(search_term: &str) -> String {
  let query_string = QueryStringSerializer::new(String::new())
      .append_pair("search_term", search_term)
      .finish();

  format!("{}?{}", SEARCH_SESSION_MEDIA_FILES_URL_PATH, query_string)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_route_escapes_the_search_term() {
    assert_eq!(
      search_session_media_files_route("red car&x=1"),
      "/v1/media_files/search_session?search_term=red+car%26x%3D1",
    );
  }
}
