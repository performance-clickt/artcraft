use crate::core::control_server::endpoints::pagination::{encode_page_cursor, parse_page_cursor, parse_page_limit, parse_raw_query, take_page};
use crate::core::control_server::envelope::control_response::{ControlErrorCode, ControlErrorResponse, ControlSuccessResponse};
use crate::core::state::app_env_configs::app_env_configs::AppEnvConfigs;
use crate::services::storyteller::state::storyteller_credential_manager::StorytellerCredentialManager;
use artcraft_api_defs::common::responses::media_links::MediaLinks;
use artcraft_client::credentials::storyteller_credential_set::StorytellerCredentialSet;
use artcraft_client::endpoints::media_files::list_media_files_for_user::{list_media_files_for_user, ListMediaFilesForUserArgs, MediaFileForUserListItem};
use artcraft_client::endpoints::media_files::search_session_media_files::{search_session_media_files, SearchSessionMediaFileListItem};
use artcraft_client::endpoints::users::get_session_info::get_session_info;
use axum::extract::{RawQuery, State};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use enums::by_table::media_files::media_file_class::MediaFileClass;
use enums::by_table::media_files::media_file_engine_category::MediaFileEngineCategory;
use enums::by_table::media_files::media_file_type::MediaFileType;
use log::warn;
use serde_derive::Serialize;
use tauri::{AppHandle, Manager};
use tokens::tokens::media_files::MediaFileToken;

const APP_STATE_UNAVAILABLE_MESSAGE: &str = "App configuration state is unavailable.";
const NOT_LOGGED_IN_MESSAGE: &str = "No ArtCraft account is signed in. Sign in inside the app first.";
const SESSION_LOOKUP_FAILED_MESSAGE: &str = "Failed to read the signed-in account from the ArtCraft API.";
const LIST_FAILED_MESSAGE: &str = "Failed to list media files from the ArtCraft API.";

/// The desktop app's library view asks for the user's own uploads too, so the control surface
/// matches it — otherwise `/v1/media` would omit files the user can plainly see in the app.
const INCLUDE_USER_UPLOADS: bool = true;

#[derive(Serialize)]
pub struct ListMediaResponse {
  pub media: Vec<MediaSummary>,

  /// Pass back as `?cursor=` to fetch the next page. `null` when this is the last page.
  ///
  /// NB: Always `null` when `search` is set — the upstream search returns a single capped page
  /// and offers no pagination, so there is nothing further to walk to.
  pub next_cursor: Option<String>,
}

#[derive(Serialize)]
pub struct MediaSummary {
  pub token: MediaFileToken,

  /// The user-visible title, when the file has one.
  pub name: Option<String>,

  pub media_class: MediaFileClass,
  pub media_type: MediaFileType,
  pub engine_category: Option<MediaFileEngineCategory>,
  pub cdn_url: String,

  /// Replace `{WIDTH}` with the desired pixel width to build a thumbnail URL.
  pub thumbnail_url_template: Option<String>,

  pub is_user_upload: bool,
  pub created_at: DateTime<Utc>,
}

/// `GET /v1/media?search=&limit=&cursor=`
pub async fn list_media_handler(
  State(app_handle): State<AppHandle>,
  RawQuery(raw_query): RawQuery,
) -> Response {
  let params = parse_raw_query(raw_query.as_deref());

  let page_limit = match parse_page_limit(params.get("limit").map(String::as_str)) {
    Ok(page_limit) => page_limit,
    Err(message) => {
      return ControlErrorResponse::new(ControlErrorCode::BadRequest, message).into_response();
    }
  };

  let page_index = match parse_page_cursor(params.get("cursor").map(String::as_str)) {
    Ok(page_index) => page_index,
    Err(message) => {
      return ControlErrorResponse::new(ControlErrorCode::BadRequest, message).into_response();
    }
  };

  let Some(app_env_configs) = app_handle.try_state::<AppEnvConfigs>() else {
    warn!("[ControlServer] App env configs are not managed by Tauri.");

    return ControlErrorResponse::new(ControlErrorCode::Internal, APP_STATE_UNAVAILABLE_MESSAGE)
      .into_response();
  };

  let credentials = match read_signed_in_credentials(&app_handle) {
    Ok(credentials) => credentials,
    Err(response) => return response,
  };

  let api_host = &app_env_configs.storyteller_host;

  let maybe_search_term = params.get("search")
      .map(|term| term.trim())
      .filter(|term| !term.is_empty());

  // The search path is session-scoped upstream, so it needs no username; the plain listing does.
  if let Some(search_term) = maybe_search_term {
    let response = match search_session_media_files(api_host, Some(&credentials), search_term).await {
      Ok(response) => response,
      Err(err) => {
        warn!("[ControlServer] Media search failed: {:?}", err);

        return ControlErrorResponse::new(ControlErrorCode::UpstreamApiError, LIST_FAILED_MESSAGE)
          .into_response();
      }
    };

    let summaries: Vec<MediaSummary> = response.results.into_iter()
        .map(to_media_summary_from_search)
        .collect();

    // NB: `next_cursor` is dropped deliberately — upstream cannot serve a second page, and
    // handing back a cursor that returns nothing would make a paging client loop.
    let (media, _unusable_next_cursor) = take_page(summaries, page_index, page_limit);

    return ControlSuccessResponse::new(ListMediaResponse {
      media,
      next_cursor: None,
    }).into_response();
  }

  let username = match read_signed_in_username(api_host, &credentials).await {
    Ok(username) => username,
    Err(response) => return response,
  };

  let response = list_media_files_for_user(
    api_host,
    Some(&credentials),
    ListMediaFilesForUserArgs {
      username: &username,
      page_index,
      page_size: page_limit,
      include_user_uploads: INCLUDE_USER_UPLOADS,
    },
  ).await;

  let response = match response {
    Ok(response) => response,
    Err(err) => {
      warn!("[ControlServer] Media listing failed: {:?}", err);

      return ControlErrorResponse::new(ControlErrorCode::UpstreamApiError, LIST_FAILED_MESSAGE)
        .into_response();
    }
  };

  let next_cursor = next_cursor_from_page_count(
    page_index,
    response.pagination.map(|pagination| pagination.total_page_count),
    response.results.len(),
  );

  let media = response.results.into_iter()
      .map(to_media_summary_from_listing)
      .collect();

  ControlSuccessResponse::new(ListMediaResponse {
    media,
    next_cursor,
  }).into_response()
}

/// Reads the app's stored credentials, requiring a real signed-in session.
///
/// NB: Only the session cookie means "signed in" — the `avt` visitor cookie is present for
/// anonymous users too. This is the single auth path for this endpoint; the webview cookie jar is
/// never parsed. The `Err` arm is the ready-to-return error envelope.
fn read_signed_in_credentials(app_handle: &AppHandle) -> Result<StorytellerCredentialSet, Response> {
  let Some(credential_manager) = app_handle.try_state::<StorytellerCredentialManager>() else {
    warn!("[ControlServer] Storyteller credential manager is not managed by Tauri.");

    return Err(
      ControlErrorResponse::new(ControlErrorCode::Internal, APP_STATE_UNAVAILABLE_MESSAGE)
        .into_response()
    );
  };

  let maybe_credentials = match credential_manager.get_credentials() {
    Ok(maybe_credentials) => maybe_credentials,
    Err(err) => {
      warn!("[ControlServer] Failed to read Storyteller credentials: {:?}", err);
      None
    }
  };

  match maybe_credentials {
    Some(credentials) if credentials.session.is_some() => Ok(credentials),
    _ => Err(
      ControlErrorResponse::new(ControlErrorCode::NotLoggedIn, NOT_LOGGED_IN_MESSAGE)
        .into_response()
    ),
  }
}

/// The library listing is addressed by username, which only the API can resolve from the session.
async fn read_signed_in_username(
  api_host: &artcraft_client::utils::api_host::ApiHost,
  credentials: &StorytellerCredentialSet,
) -> Result<String, Response> {
  let session_info = match get_session_info(api_host, Some(credentials)).await {
    Ok(session_info) => session_info,
    Err(err) => {
      warn!("[ControlServer] Session lookup failed: {:?}", err);

      return Err(
        ControlErrorResponse::new(ControlErrorCode::UpstreamApiError, SESSION_LOOKUP_FAILED_MESSAGE)
          .into_response()
      );
    }
  };

  // A stale-but-present session cookie still reaches here, so the server's verdict wins.
  match session_info.user {
    Some(user) if session_info.logged_in => Ok(user.username),
    _ => Err(
      ControlErrorResponse::new(ControlErrorCode::NotLoggedIn, NOT_LOGGED_IN_MESSAGE)
        .into_response()
    ),
  }
}

/// Upstream paginates by page count, so the next cursor exists only while pages remain.
/// When the page count is missing, an empty page is treated as the end of the listing.
fn next_cursor_from_page_count(
  page_index: usize,
  maybe_total_page_count: Option<usize>,
  returned_row_count: usize,
) -> Option<String> {
  match maybe_total_page_count {
    Some(total_page_count) => {
      (page_index + 1 < total_page_count).then(|| encode_page_cursor(page_index + 1))
    }
    None => {
      (returned_row_count > 0).then(|| encode_page_cursor(page_index + 1))
    }
  }
}

fn to_media_summary_from_listing(item: MediaFileForUserListItem) -> MediaSummary {
  MediaSummary {
    token: item.token,
    name: item.maybe_title,
    media_class: item.media_class,
    media_type: item.media_type,
    engine_category: item.maybe_engine_category,
    cdn_url: item.media_links.cdn_url.to_string(),
    thumbnail_url_template: to_thumbnail_url_template(&item.media_links),
    is_user_upload: item.is_user_upload,
    created_at: item.created_at,
  }
}

fn to_media_summary_from_search(item: SearchSessionMediaFileListItem) -> MediaSummary {
  MediaSummary {
    token: item.token,
    name: item.maybe_title,
    media_class: item.media_class,
    media_type: item.media_type,
    engine_category: item.maybe_engine_category,
    cdn_url: item.media_links.cdn_url.to_string(),
    thumbnail_url_template: to_thumbnail_url_template(&item.media_links),
    is_user_upload: item.is_user_upload,
    created_at: item.created_at,
  }
}

/// Images carry a thumbnail template directly; videos carry theirs on the preview stills instead.
fn to_thumbnail_url_template(media_links: &MediaLinks) -> Option<String> {
  if let Some(template) = &media_links.maybe_thumbnail_template {
    return Some(template.clone());
  }

  media_links.maybe_video_previews
      .as_ref()
      .map(|previews| previews.still_thumbnail_template.clone())
}

#[cfg(test)]
mod tests {
  use super::*;
  use artcraft_api_defs::common::responses::media_links::VideoPreviews;
  use url::Url;

  const CDN_URL: &str = "https://cdn.example.com/files/abc123.png";
  const IMAGE_THUMBNAIL_TEMPLATE: &str = "https://cdn.example.com/t/abc123/{WIDTH}.png";
  const VIDEO_STILL_THUMBNAIL_TEMPLATE: &str = "https://cdn.example.com/t/still/{WIDTH}.jpg";

  mod next_cursor_tests {
    use super::*;

    #[test]
    fn test_more_pages_yields_the_next_page_cursor() {
      assert_eq!(next_cursor_from_page_count(0, Some(3), 50).as_deref(), Some("1"));
      assert_eq!(next_cursor_from_page_count(1, Some(3), 50).as_deref(), Some("2"));
    }

    #[test]
    fn test_last_page_yields_no_cursor() {
      assert_eq!(next_cursor_from_page_count(2, Some(3), 12), None);
      assert_eq!(next_cursor_from_page_count(9, Some(3), 0), None);
    }

    #[test]
    fn test_missing_page_count_walks_until_a_page_comes_back_empty() {
      assert_eq!(next_cursor_from_page_count(0, None, 50).as_deref(), Some("1"));
      assert_eq!(next_cursor_from_page_count(1, None, 0), None);
    }
  }

  mod thumbnail_tests {
    use super::*;

    #[test]
    fn test_image_thumbnail_template_is_preferred() {
      let media_links = media_links_with(
        Some(IMAGE_THUMBNAIL_TEMPLATE.to_string()),
        true,
      );

      assert_eq!(
        to_thumbnail_url_template(&media_links).as_deref(),
        Some(IMAGE_THUMBNAIL_TEMPLATE),
      );
    }

    #[test]
    fn test_video_still_template_is_the_fallback() {
      let media_links = media_links_with(None, true);

      assert_eq!(
        to_thumbnail_url_template(&media_links).as_deref(),
        Some(VIDEO_STILL_THUMBNAIL_TEMPLATE),
      );
    }

    #[test]
    fn test_no_templates_yields_none() {
      let media_links = media_links_with(None, false);

      assert_eq!(to_thumbnail_url_template(&media_links), None);
    }
  }

  fn media_links_with(
    maybe_thumbnail_template: Option<String>,
    with_video_previews: bool,
  ) -> MediaLinks {
    let url = Url::parse(CDN_URL).expect("the test CDN url parses");

    MediaLinks {
      cdn_url: url.clone(),
      maybe_thumbnail_template,
      maybe_video_previews: with_video_previews.then(|| VideoPreviews {
        still: url.clone(),
        animated: url,
        still_thumbnail_template: VIDEO_STILL_THUMBNAIL_TEMPLATE.to_string(),
        animated_thumbnail_template: VIDEO_STILL_THUMBNAIL_TEMPLATE.to_string(),
      }),
    }
  }
}
