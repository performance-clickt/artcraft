use crate::core::commands::enqueue::generate_error::{
  BadInputReason, GenerateError, MissingCredentialsReason,
};
use crate::core::control_server::envelope::control_response::{ControlErrorCode, ControlErrorResponse};
use log::error;

const BILLING_MESSAGE: &str = "The generation provider rejected the request for billing or credit reasons.";
const INTERNAL_MESSAGE: &str = "The generation request failed inside the app.";
const NO_PROVIDER_MESSAGE: &str = "No configured provider is available for this request.";
const PROVIDER_FAILURE_MESSAGE: &str = "The generation provider rejected or failed the request.";

/// Maps an enqueue failure onto the control protocol's error envelope.
///
/// NB: The message is curated per class rather than `format!("{:?}", err)`. Provider error values
/// carry upstream response bodies, and those must not be handed to an unattended HTTP client that
/// may log them; the full error is written to the app log instead, where it already belongs.
pub fn generate_error_to_control_response(
  endpoint: &str,
  err: GenerateError,
) -> ControlErrorResponse {
  error!("[ControlServer] {} failed: {:?}", endpoint, err);

  let (code, message) = match &err {
    // ── Caller's fault ──
    GenerateError::BadInput(reason) => (ControlErrorCode::BadRequest, bad_input_message(reason)),
    GenerateError::DecodeError(_) => (
      ControlErrorCode::BadRequest,
      "An image input could not be base64-decoded.".to_string(),
    ),
    GenerateError::BadProviderForModel { provider, model } => (
      ControlErrorCode::BadRequest,
      format!("Model {:?} is not served by provider {:?}.", model, provider),
    ),
    GenerateError::NotYetImplemented(what) => (
      ControlErrorCode::BadRequest,
      format!("Not supported by this build: {}.", what),
    ),
    GenerateError::ArtcraftRouterNotYetSupportedProvider(provider) => (
      ControlErrorCode::BadRequest,
      format!("Provider {} is not supported by this build.", provider),
    ),
    GenerateError::FalNoLongerSupported => (
      ControlErrorCode::BadRequest,
      "The Fal provider is not supported by this build.".to_string(),
    ),
    GenerateError::NoProviderAvailable => (
      ControlErrorCode::BadRequest,
      NO_PROVIDER_MESSAGE.to_string(),
    ),

    // ── Signed-out / missing credentials ──
    GenerateError::MissingCredentials(reason) => (
      ControlErrorCode::NotLoggedIn,
      missing_credentials_message(reason),
    ),

    // ── Upstream ──
    GenerateError::BillingIssue(reason) => (
      ControlErrorCode::UpstreamApiError,
      format!("{} (provider: {:?})", BILLING_MESSAGE, reason.provider),
    ),
    GenerateError::ProviderFailure(_) => (
      ControlErrorCode::UpstreamApiError,
      PROVIDER_FAILURE_MESSAGE.to_string(),
    ),
    GenerateError::ArtcraftRouterDownloadError(_) => (
      ControlErrorCode::UpstreamApiError,
      "An input asset could not be downloaded.".to_string(),
    ),
    GenerateError::ResponseHadNoJobTokens => (
      ControlErrorCode::UpstreamApiError,
      "The provider accepted the job but returned no job identifier.".to_string(),
    ),

    // ── Ours ──
    GenerateError::AnyhowError(_) | GenerateError::IoError(_) => (
      ControlErrorCode::Internal,
      INTERNAL_MESSAGE.to_string(),
    ),
  };

  ControlErrorResponse::new(code, message)
}

fn bad_input_message(reason: &BadInputReason) -> String {
  match reason {
    BadInputReason::Base64DecodeError => "An image input could not be base64-decoded.".to_string(),
    BadInputReason::BothImageMaskMediaTokenAndBytesSupplied => {
      "Supply either the inpainting mask media token or its bytes, not both.".to_string()
    }
    BadInputReason::CannotDetermineImageMimeType => {
      "The image input's format could not be determined.".to_string()
    }
    BadInputReason::InvalidNumberOfInputImages { provided, min, max } => format!(
      "This model takes between {} and {} input images; {} were supplied.",
      min, max, provided,
    ),
    BadInputReason::InvalidNumberOfRequestedImages { requested, min, max } => format!(
      "This model generates between {} and {} images; {} were requested.",
      min, max, requested,
    ),
    BadInputReason::NoModelSpecified => "No model was specified.".to_string(),
    BadInputReason::RequiredSourceImageMaskNotProvided => {
      "This model requires an inpainting mask image.".to_string()
    }
    BadInputReason::RequiredSourceImageNotProvided => {
      "This model requires a source image.".to_string()
    }
    BadInputReason::WrongImageArguments(message) => {
      format!("Invalid image arguments: {}.", message)
    }
  }
}

fn missing_credentials_message(reason: &MissingCredentialsReason) -> String {
  let requirement = match reason {
    MissingCredentialsReason::NeedsFalApiKey => "a Fal API key",
    MissingCredentialsReason::NeedsGrokCredentials => "Grok credentials",
    MissingCredentialsReason::NeedsMidjourneyCredentials => "Midjourney credentials",
    MissingCredentialsReason::NeedsMidjourneyUserId => "a Midjourney user id",
    MissingCredentialsReason::NeedsMidjourneyUserInfo => "Midjourney user info",
    MissingCredentialsReason::NeedsSoraCredentials => "Sora credentials",
    MissingCredentialsReason::NeedsStorytellerCredentials => "an Artcraft sign-in",
    MissingCredentialsReason::NeedsWorldLabsCredentials => "World Labs credentials",
  };

  format!("The app is missing {}; sign in or add the credential in the app.", requirement)
}
