use crate::core::control_server::envelope::control_response::{ControlErrorCode, ControlErrorResponse};
use axum::extract::rejection::JsonRejection;
use axum::Json;

/// Turns axum's JSON extractor rejection into the control protocol's `BAD_REQUEST` envelope.
///
/// NB: Handlers take `Result<Json<T>, JsonRejection>` rather than `Json<T>` precisely so this can
/// run. A bare `Json<T>` extractor answers a rejection with axum's own plain-text 4xx body, which
/// is neither the success nor the failure envelope a control client parses — an empty or malformed
/// body would look like a protocol violation instead of a bad request.
pub fn read_json_body<T>(
  body: Result<Json<T>, JsonRejection>,
) -> Result<T, ControlErrorResponse> {
  match body {
    Ok(Json(request)) => Ok(request),
    Err(rejection) => Err(ControlErrorResponse::new(
      ControlErrorCode::BadRequest,
      format!("Invalid request body: {}", rejection.body_text()),
    )),
  }
}
