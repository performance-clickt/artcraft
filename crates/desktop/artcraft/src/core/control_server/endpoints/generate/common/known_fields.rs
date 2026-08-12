use crate::core::control_server::endpoints::generate::common::json_body::read_json_body;
use crate::core::control_server::envelope::control_response::{ControlErrorCode, ControlErrorResponse};
use axum::extract::rejection::JsonRejection;
use axum::Json;
use serde::de::DeserializeOwned;
use serde_json::Value;

const NOT_AN_OBJECT_MESSAGE: &str = "Request body must be a JSON object.";

/// Reads a generation body, rejecting any field the command does not understand.
///
/// NB: This exists because the generate commands' own request structs have every field `Option`
/// and carry no `deny_unknown_fields` — a body with `promt` instead of `prompt` deserializes
/// cleanly, enqueues, and spends credits on a generation the caller never asked for. Only
/// `/v1/generate/image` has a mirrored body it can annotate; the rest deserialize the Tauri
/// structs directly, which are upstream and shared with the IPC bridge, so the check is done here
/// against an explicit field list instead.
///
/// The list is per endpoint and next to the handler that owns it. If upstream adds a field, the
/// endpoint rejects it with a named `BAD_REQUEST` until the list is updated — a loud, cheap
/// failure, unlike the silent drop this replaces.
pub fn read_json_body_with_known_fields<T: DeserializeOwned>(
  body: Result<Json<Value>, JsonRejection>,
  known_fields: &[&str],
) -> Result<T, ControlErrorResponse> {
  let body = read_json_body::<Value>(body)?;

  let Value::Object(fields) = &body else {
    return Err(ControlErrorResponse::new(
      ControlErrorCode::BadRequest,
      NOT_AN_OBJECT_MESSAGE,
    ));
  };

  let unknown_fields = collect_unknown_fields(fields.keys().map(String::as_str), known_fields);

  if !unknown_fields.is_empty() {
    return Err(ControlErrorResponse::new(
      ControlErrorCode::BadRequest,
      format!("Unknown field(s): {}.", unknown_fields.join(", ")),
    ));
  }

  serde_json::from_value::<T>(body).map_err(|err| {
    ControlErrorResponse::new(
      ControlErrorCode::BadRequest,
      format!("Invalid request body: {}", err),
    )
  })
}

/// Sorted so the message is stable regardless of the order the client wrote the fields in.
fn collect_unknown_fields<'a>(
  body_fields: impl Iterator<Item = &'a str>,
  known_fields: &[&str],
) -> Vec<String> {
  let mut unknown_fields: Vec<String> = body_fields
      .filter(|field| !known_fields.contains(field))
      .map(|field| format!("`{}`", field))
      .collect();

  unknown_fields.sort();

  unknown_fields
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_derive::Deserialize;

  const KNOWN_FIELDS: &[&str] = &["model", "prompt"];

  #[derive(Deserialize)]
  struct TestRequest {
    prompt: Option<String>,
  }

  #[test]
  fn test_a_known_body_is_accepted() {
    let request: TestRequest = expect_accepted(read_json_body_with_known_fields(
      json_body(r#"{"model": "veo_3_fast", "prompt": "a red cube"}"#),
      KNOWN_FIELDS,
    ));

    assert_eq!(request.prompt.as_deref(), Some("a red cube"));
  }

  #[test]
  fn test_a_misspelled_field_is_rejected() {
    let result = read_json_body_with_known_fields::<TestRequest>(
      json_body(r#"{"model": "veo_3_fast", "promt": "a red cube"}"#),
      KNOWN_FIELDS,
    );

    assert_eq!(expect_rejected(result), "Unknown field(s): `promt`.");
  }

  #[test]
  fn test_every_unknown_field_is_named_in_one_message() {
    let result = read_json_body_with_known_fields::<TestRequest>(
      json_body(r#"{"zzz": 1, "aaa": 2}"#),
      KNOWN_FIELDS,
    );

    assert_eq!(expect_rejected(result), "Unknown field(s): `aaa`, `zzz`.");
  }

  #[test]
  fn test_a_non_object_body_is_rejected() {
    let result = read_json_body_with_known_fields::<TestRequest>(json_body("[]"), KNOWN_FIELDS);

    assert_eq!(expect_rejected(result), NOT_AN_OBJECT_MESSAGE);
  }

  fn json_body(body: &str) -> Result<Json<Value>, JsonRejection> {
    Ok(Json(serde_json::from_str(body).expect("the test body is valid JSON")))
  }

  fn expect_accepted<T>(result: Result<T, ControlErrorResponse>) -> T {
    match result {
      Ok(value) => value,
      Err(error) => panic!("expected success, got {}", error.error.message),
    }
  }

  // NB: `ControlErrorResponse` is not `Debug` (it is a wire type), so the message is compared
  // rather than the response.
  fn expect_rejected<T>(result: Result<T, ControlErrorResponse>) -> String {
    match result {
      Ok(_) => panic!("expected a rejection"),
      Err(error) => {
        assert_eq!(error.error.code, ControlErrorCode::BadRequest);
        error.error.message
      }
    }
  }
}
