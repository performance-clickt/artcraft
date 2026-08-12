use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Serialize, Serializer};
use std::fmt;

/// Success envelope for every control server endpoint: `{"success": true, "data": ..}`.
#[derive(Serialize)]
pub struct ControlSuccessResponse<T> {
  pub success: bool,
  pub data: T,
}

impl<T> ControlSuccessResponse<T> {
  pub fn new(data: T) -> Self {
    Self {
      success: true,
      data,
    }
  }
}

impl<T: Serialize> IntoResponse for ControlSuccessResponse<T> {
  fn into_response(self) -> Response {
    (StatusCode::OK, Json(self)).into_response()
  }
}

/// Failure envelope: `{"success": false, "error": {"code", "message"}}`.
#[derive(Serialize)]
pub struct ControlErrorResponse {
  pub success: bool,
  pub error: ControlErrorBody,
}

impl ControlErrorResponse {
  pub fn new<M: Into<String>>(code: ControlErrorCode, message: M) -> Self {
    Self {
      success: false,
      error: ControlErrorBody {
        code,
        message: message.into(),
      },
    }
  }
}

impl IntoResponse for ControlErrorResponse {
  fn into_response(self) -> Response {
    let status = self.error.code.http_status();
    (status, Json(self)).into_response()
  }
}

#[derive(Serialize)]
pub struct ControlErrorBody {
  pub code: ControlErrorCode,
  pub message: String,
}

/// The complete set of error codes the control protocol may return.
/// NB: Variants beyond `Unauthorized` are mounted by the endpoints that land in later issues.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlErrorCode {
  Unauthorized,
  BadRequest,
  NotLoggedIn,
  SceneNotActive,
  SceneBridgeTimeout,
  TaskNotFound,
  UpstreamApiError,
  Internal,
}

impl ControlErrorCode {
  pub fn to_str(&self) -> &'static str {
    match self {
      Self::Unauthorized => "UNAUTHORIZED",
      Self::BadRequest => "BAD_REQUEST",
      Self::NotLoggedIn => "NOT_LOGGED_IN",
      Self::SceneNotActive => "SCENE_NOT_ACTIVE",
      Self::SceneBridgeTimeout => "SCENE_BRIDGE_TIMEOUT",
      Self::TaskNotFound => "TASK_NOT_FOUND",
      Self::UpstreamApiError => "UPSTREAM_API_ERROR",
      Self::Internal => "INTERNAL",
    }
  }

  pub fn http_status(&self) -> StatusCode {
    match self {
      Self::Unauthorized => StatusCode::UNAUTHORIZED,
      Self::BadRequest => StatusCode::BAD_REQUEST,
      Self::NotLoggedIn => StatusCode::FORBIDDEN,
      Self::SceneNotActive => StatusCode::CONFLICT,
      Self::SceneBridgeTimeout => StatusCode::GATEWAY_TIMEOUT,
      Self::TaskNotFound => StatusCode::NOT_FOUND,
      Self::UpstreamApiError => StatusCode::BAD_GATEWAY,
      Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
    }
  }
}

impl Serialize for ControlErrorCode {
  fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(self.to_str())
  }
}

impl fmt::Display for ControlErrorCode {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", self.to_str())
  }
}
