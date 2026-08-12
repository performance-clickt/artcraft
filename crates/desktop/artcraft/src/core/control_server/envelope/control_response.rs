use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Serialize, Serializer};

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

/// The error codes the control protocol returns today. NB: Each later issue adds the variants
/// its own endpoints raise (`NOT_LOGGED_IN`, `SCENE_NOT_ACTIVE`, …) rather than pre-landing them
/// dead, so the compiler keeps flagging any variant that loses its last call site.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlErrorCode {
  Unauthorized,
  BadRequest,
  SceneNotActive,
  SceneBridgeTimeout,
  Internal,
}

impl ControlErrorCode {
  pub fn to_str(&self) -> &'static str {
    match self {
      Self::Unauthorized => "UNAUTHORIZED",
      Self::BadRequest => "BAD_REQUEST",
      Self::SceneNotActive => "SCENE_NOT_ACTIVE",
      Self::SceneBridgeTimeout => "SCENE_BRIDGE_TIMEOUT",
      Self::Internal => "INTERNAL",
    }
  }

  pub fn http_status(&self) -> StatusCode {
    match self {
      Self::Unauthorized => StatusCode::UNAUTHORIZED,
      Self::BadRequest => StatusCode::BAD_REQUEST,
      // NB: The request was well-formed; the app just has no 3D scene mounted to run it against.
      Self::SceneNotActive => StatusCode::CONFLICT,
      // NB: We are the gateway to the webview, and the webview never answered.
      Self::SceneBridgeTimeout => StatusCode::GATEWAY_TIMEOUT,
      Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
    }
  }
}

impl Serialize for ControlErrorCode {
  fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(self.to_str())
  }
}
