use crate::core::control_server::envelope::control_response::{ControlErrorCode, ControlErrorResponse};
use crate::core::control_server::state::control_server_settings::ControlServerSettings;
use axum::extract::{Request, State};
use axum::http::header::AUTHORIZATION;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

const BEARER_SCHEME: &str = "bearer";
const UNAUTHORIZED_MESSAGE: &str = "Missing or invalid bearer token.";

/// Rejects every request that does not carry `Authorization: Bearer <token>` for this launch's
/// token. NB: The failure message is deliberately identical for a missing and a wrong token.
pub async fn bearer_auth_layer(
  State(settings): State<ControlServerSettings>,
  request: Request,
  next: Next,
) -> Response {
  let is_authorized = match extract_bearer_token(&request) {
    None => false,
    Some(provided_token) => is_matching_token(provided_token, settings.token()),
  };

  if !is_authorized {
    return ControlErrorResponse::new(ControlErrorCode::Unauthorized, UNAUTHORIZED_MESSAGE)
      .into_response();
  }

  next.run(request).await
}

fn extract_bearer_token(request: &Request) -> Option<&str> {
  let header_value = request.headers().get(AUTHORIZATION)?;
  let header_str = header_value.to_str().ok()?;
  parse_bearer_header(header_str)
}

/// RFC 7235 makes the auth-scheme case-insensitive and allows padding around the credentials, so
/// `bearer <token>` from a client or proxy must not be rejected as if the token were wrong.
fn parse_bearer_header(header_str: &str) -> Option<&str> {
  let (scheme, credentials) = header_str.trim().split_once(' ')?;

  if !scheme.eq_ignore_ascii_case(BEARER_SCHEME) {
    return None;
  }

  Some(credentials.trim())
}

/// Compares in constant time with respect to the token contents so a caller cannot probe the
/// token byte by byte. Length is allowed to leak — the token length is a fixed, public constant.
fn is_matching_token(provided: &str, expected: &str) -> bool {
  if provided.len() != expected.len() {
    return false;
  }

  let mut difference: u8 = 0;
  for (provided_byte, expected_byte) in provided.bytes().zip(expected.bytes()) {
    difference |= provided_byte ^ expected_byte;
  }

  difference == 0
}

#[cfg(test)]
mod tests {
  use super::*;

  const TOKEN: &str = "0123456789abcdef";

  #[test]
  fn test_matching_token_is_accepted() {
    assert!(is_matching_token(TOKEN, TOKEN));
  }

  #[test]
  fn test_wrong_and_wrong_length_tokens_are_rejected() {
    assert!(!is_matching_token("0123456789abcdee", TOKEN));
    assert!(!is_matching_token("", TOKEN));
    assert!(!is_matching_token("0123456789abcdef0", TOKEN));
  }

  #[test]
  fn test_bearer_scheme_is_case_and_whitespace_insensitive() {
    assert_eq!(parse_bearer_header("Bearer 0123456789abcdef"), Some(TOKEN));
    assert_eq!(parse_bearer_header("bearer 0123456789abcdef"), Some(TOKEN));
    assert_eq!(parse_bearer_header("BEARER  0123456789abcdef "), Some(TOKEN));
  }

  #[test]
  fn test_other_schemes_and_bare_tokens_are_rejected() {
    assert_eq!(parse_bearer_header("Basic 0123456789abcdef"), None);
    assert_eq!(parse_bearer_header("0123456789abcdef"), None);
    assert_eq!(parse_bearer_header(""), None);
  }
}
