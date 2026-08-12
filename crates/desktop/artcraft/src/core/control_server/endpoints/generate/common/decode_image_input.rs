use crate::core::control_server::envelope::control_response::{ControlErrorCode, ControlErrorResponse};
use web_base64::web_base64_decode::web_base64_decode;

/// Decodes an optional base64 image field from a control request body into the raw bytes the
/// Tauri command structs expect.
///
/// NB: The command structs take `Vec<u8>`, and serde deserializes that from a JSON *array of
/// numbers* — which is what the Tauri IPC bridge sends. An HTTP client sends base64 instead, so
/// the control server accepts a string here and decodes it. `web_base64_decode` is reused rather
/// than a bare engine call because it also tolerates a `data:image/png;base64,` prefix, exactly as
/// the app's own base64 image paths do.
pub fn decode_optional_base64_image(
  field_name: &str,
  maybe_base64: Option<&str>,
) -> Result<Option<Vec<u8>>, ControlErrorResponse> {
  let Some(base64_image) = maybe_base64 else {
    return Ok(None);
  };

  if base64_image.is_empty() {
    return Err(base64_error(field_name, "the value is empty"));
  }

  match web_base64_decode(base64_image) {
    Ok(bytes) => Ok(Some(bytes)),
    // NB: The decode error text describes the encoding fault only (offset, invalid byte). It
    // never contains the payload, so it is safe to return.
    Err(err) => Err(base64_error(field_name, &err.to_string())),
  }
}

/// Rejects a request that supplies the same image twice — once as an uploaded media token and once
/// inline. The command structs document these as XOR pairs and silently prefer one over the other,
/// which would make an HTTP caller's mistake invisible.
pub fn reject_conflicting_image_input(
  token_field_name: &str,
  bytes_field_name: &str,
  has_token: bool,
  has_bytes: bool,
) -> Result<(), ControlErrorResponse> {
  if has_token && has_bytes {
    return Err(ControlErrorResponse::new(
      ControlErrorCode::BadRequest,
      format!("Supply either `{}` or `{}`, not both.", token_field_name, bytes_field_name),
    ));
  }

  Ok(())
}

fn base64_error(field_name: &str, detail: &str) -> ControlErrorResponse {
  ControlErrorResponse::new(
    ControlErrorCode::BadRequest,
    format!("Field `{}` is not valid base64: {}.", field_name, detail),
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  const FIELD: &str = "canvas_image_base64";
  const PNG_PIXEL_BASE64: &str = "iVBORw0KGgo=";
  const PNG_PIXEL_BYTES: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

  mod decode_tests {
    use super::*;

    #[test]
    fn test_absent_field_decodes_to_none() {
      let decoded = expect_decoded(decode_optional_base64_image(FIELD, None));

      assert!(decoded.is_none());
    }

    #[test]
    fn test_plain_base64_decodes_to_bytes() {
      let decoded = expect_decoded(decode_optional_base64_image(FIELD, Some(PNG_PIXEL_BASE64)));

      assert_eq!(decoded.as_deref(), Some(PNG_PIXEL_BYTES));
    }

    #[test]
    fn test_data_url_prefix_is_stripped_before_decoding() {
      let data_url = format!("data:image/png;base64,{}", PNG_PIXEL_BASE64);

      let decoded = expect_decoded(decode_optional_base64_image(FIELD, Some(&data_url)));

      assert_eq!(decoded.as_deref(), Some(PNG_PIXEL_BYTES));
    }

    #[test]
    fn test_empty_and_malformed_values_are_bad_requests() {
      assert_bad_request(decode_optional_base64_image(FIELD, Some("")));
      assert_bad_request(decode_optional_base64_image(FIELD, Some("not base64!!")));
    }
  }

  mod conflicting_input_tests {
    use super::*;

    #[test]
    fn test_one_or_neither_input_is_accepted() {
      assert!(reject_conflicting_image_input("token", "bytes", false, false).is_ok());
      assert!(reject_conflicting_image_input("token", "bytes", true, false).is_ok());
      assert!(reject_conflicting_image_input("token", "bytes", false, true).is_ok());
    }

    #[test]
    fn test_both_inputs_together_are_rejected() {
      let result = reject_conflicting_image_input("token", "bytes", true, true);

      assert_bad_request(result.map(|_| None));
    }
  }

  // NB: `ControlErrorResponse` is not `Debug` (it is a wire type), so these stand in for
  // `unwrap`/`expect` rather than widening the envelope's derives for tests alone.
  fn expect_decoded(result: Result<Option<Vec<u8>>, ControlErrorResponse>) -> Option<Vec<u8>> {
    match result {
      Ok(decoded) => decoded,
      Err(error) => panic!("expected success, got {}", error.error.code.to_str()),
    }
  }

  fn assert_bad_request(result: Result<Option<Vec<u8>>, ControlErrorResponse>) {
    match result {
      Ok(_) => panic!("expected a rejection"),
      Err(error) => assert_eq!(error.error.code, ControlErrorCode::BadRequest),
    }
  }
}
