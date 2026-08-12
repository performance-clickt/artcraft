use crate::credentials::storyteller_credential_set::StorytellerCredentialSet;
use crate::error::storyteller_error::StorytellerError;
use crate::utils::api_host::ApiHost;
use crate::utils::basic_json_get_request::basic_json_get_request;
use serde_derive::Deserialize;
use tokens::tokens::users::UserToken;

pub const SESSION_INFO_URL_PATH: &str = "/v1/session";

/// Reads the session identity for the supplied credentials.
///
/// NB: The canonical response type lives in `artcraft_api_defs::users::session_info`, but it is
/// `Serialize`-only (it is authored server-side), so this module declares the small
/// `Deserialize` subset the client actually consumes. Serde ignores unknown fields, so the
/// server is free to keep adding to the payload.
pub async fn get_session_info(
  api_host: &ApiHost,
  maybe_creds: Option<&StorytellerCredentialSet>,
) -> Result<GetSessionInfoSuccessResponse, StorytellerError> {
  Ok(basic_json_get_request(
    api_host,
    SESSION_INFO_URL_PATH,
    maybe_creds,
  ).await?)
}

#[derive(Deserialize, Debug)]
pub struct GetSessionInfoSuccessResponse {
  pub success: bool,

  /// False for anonymous visitors — the `visitor` cookie alone is not a signed-in session.
  pub logged_in: bool,

  /// Only present when `logged_in` is true.
  pub user: Option<SessionInfoUser>,
}

#[derive(Deserialize, Debug)]
pub struct SessionInfoUser {
  pub user_token: UserToken,

  /// The username, which is also the path segment for the user's own media-file listing.
  pub username: String,

  pub display_name: String,
}
