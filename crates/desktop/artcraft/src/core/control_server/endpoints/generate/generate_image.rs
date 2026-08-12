use crate::core::commands::enqueue::generate_error::GenerateError;
use crate::core::commands::generate::generate_image::providers::artcraft::handle_artcraft;
use crate::core::commands::generate::generate_image::providers::artcraft_router::handle_router::handle_router;
use crate::core::commands::generate::generate_image::tauri_generate_image_request::TauriGenerateImageRequest;
use crate::core::commands::generate::generate_image::tauri_image_model::TauriImageModel;
use crate::core::control_server::endpoints::generate::common::decode_image_input::{
  decode_optional_base64_image, reject_conflicting_image_input,
};
use crate::core::control_server::endpoints::generate::common::enqueued_task_response::{
  notify_frontend_of_enqueue_success, EnqueuedTaskResponse,
};
use crate::core::control_server::endpoints::generate::common::generate_error_mapping::generate_error_to_control_response;
use crate::core::control_server::endpoints::generate::common::json_body::read_json_body;
use crate::core::control_server::require_tauri_state::require_tauri_state;
use crate::core::control_server::envelope::control_response::{
  ControlErrorCode, ControlErrorResponse, ControlSuccessResponse,
};
use crate::core::providers::credentials::provider_credential_loading_cache::ProviderCredentialLoadingCache;
use crate::core::state::app_env_configs::app_env_configs::AppEnvConfigs;
use crate::core::state::task_database::TaskDatabase;
use crate::services::storyteller::state::storyteller_credential_manager::StorytellerCredentialManager;
use axum::extract::rejection::JsonRejection;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::Json;
use enums::common::generation::common_aspect_ratio::CommonAspectRatio;
use enums::common::generation::common_quality::CommonQuality;
use enums::common::generation::common_resolution::CommonResolution;
use enums::common::generation_provider::GenerationProvider;
use enums::tauri::ux::tauri_command_caller::TauriCommandCaller;
use log::error;
use serde::Deserialize;
use tokens::tokens::media_files::MediaFileToken;

const ENDPOINT: &str = "POST /v1/generate/image";
const MIDJOURNEY_UNSUPPORTED: &str = "Midjourney image generation is not exposed on the control server";

/// Body of `POST /v1/generate/image`.
///
/// Mirrors `TauriGenerateImageRequest` field for field, except that every `*_raw_bytes` field
/// becomes a `*_base64` string: serde decodes `Vec<u8>` from a JSON array of numbers, which is what
/// the Tauri IPC bridge sends but not what an HTTP client should have to send for a whole image.
#[derive(Deserialize, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct ControlGenerateImageRequest {
  pub provider: Option<GenerationProvider>,
  pub model: Option<TauriImageModel>,
  pub prompt: Option<String>,
  pub aspect_ratio: Option<CommonAspectRatio>,
  pub resolution: Option<CommonResolution>,
  pub quality: Option<CommonQuality>,
  pub batch_size: Option<u32>,
  pub image_media_tokens: Option<Vec<MediaFileToken>>,

  // ── Canvas / scene images (token XOR base64) ──
  pub canvas_image_media_token: Option<MediaFileToken>,
  pub canvas_image_base64: Option<String>,
  pub scene_image_media_token: Option<MediaFileToken>,
  pub scene_image_base64: Option<String>,

  // ── Inpainting (token XOR base64) ──
  pub inpainting_mask_image_media_token: Option<MediaFileToken>,
  pub inpainting_mask_image_base64: Option<String>,

  // ── Angle adjustment ──
  pub adjust_horizontal_angle: Option<f64>,
  pub adjust_vertical_angle: Option<f64>,
  pub adjust_zoom: Option<f64>,

  pub enable_system_prompt: Option<bool>,

  // ── Frontend metadata ──
  //
  // NB: There is no control-server variant of `TauriCommandCaller` — every variant names a UI
  // surface, and the enum is persisted in the tasks DB and re-parsed by the frontend, so inventing
  // one would be a schema change with frontend blast radius. A control-started job therefore
  // carries no caller by default (a NULL column the frontend already tolerates), and a client that
  // wants its result routed to a UI surface may set these explicitly.
  pub frontend_caller: Option<TauriCommandCaller>,
  pub frontend_subscriber_id: Option<String>,
  pub frontend_subscriber_payload: Option<String>,
}

impl ControlGenerateImageRequest {
  /// Validates and lowers the HTTP body onto the Tauri command's request struct.
  pub fn into_command_request(self) -> Result<TauriGenerateImageRequest, ControlErrorResponse> {
    if self.model.is_none() {
      return Err(ControlErrorResponse::new(
        ControlErrorCode::BadRequest,
        "Field `model` is required.",
      ));
    }

    reject_conflicting_image_input(
      "canvas_image_media_token",
      "canvas_image_base64",
      self.canvas_image_media_token.is_some(),
      self.canvas_image_base64.is_some(),
    )?;
    reject_conflicting_image_input(
      "scene_image_media_token",
      "scene_image_base64",
      self.scene_image_media_token.is_some(),
      self.scene_image_base64.is_some(),
    )?;
    reject_conflicting_image_input(
      "inpainting_mask_image_media_token",
      "inpainting_mask_image_base64",
      self.inpainting_mask_image_media_token.is_some(),
      self.inpainting_mask_image_base64.is_some(),
    )?;

    let canvas_image_raw_bytes =
      decode_optional_base64_image("canvas_image_base64", self.canvas_image_base64.as_deref())?;
    let scene_image_raw_bytes =
      decode_optional_base64_image("scene_image_base64", self.scene_image_base64.as_deref())?;
    let inpainting_mask_image_raw_bytes = decode_optional_base64_image(
      "inpainting_mask_image_base64",
      self.inpainting_mask_image_base64.as_deref(),
    )?;

    Ok(TauriGenerateImageRequest {
      provider: self.provider,
      model: self.model,
      prompt: self.prompt,
      aspect_ratio: self.aspect_ratio,
      resolution: self.resolution,
      quality: self.quality,
      batch_size: self.batch_size,
      image_media_tokens: self.image_media_tokens,
      canvas_image_media_token: self.canvas_image_media_token,
      canvas_image_raw_bytes,
      scene_image_media_token: self.scene_image_media_token,
      scene_image_raw_bytes,
      inpainting_mask_image_media_token: self.inpainting_mask_image_media_token,
      inpainting_mask_image_raw_bytes,
      adjust_horizontal_angle: self.adjust_horizontal_angle,
      adjust_vertical_angle: self.adjust_vertical_angle,
      adjust_zoom: self.adjust_zoom,
      enable_system_prompt: self.enable_system_prompt,
      frontend_caller: self.frontend_caller,
      frontend_subscriber_id: self.frontend_subscriber_id,
      frontend_subscriber_payload: self.frontend_subscriber_payload,
    })
  }
}

pub async fn post_generate_image_handler(
  State(app_handle): State<tauri::AppHandle>,
  body: Result<Json<ControlGenerateImageRequest>, JsonRejection>,
) -> Response {
  match handle_generate_image(&app_handle, body).await {
    Ok(response) => response.into_response(),
    Err(error) => error.into_response(),
  }
}

/// NB: This mirrors `generate_image_command`'s provider dispatch rather than calling it. That
/// command is the only one of the five with no inner `handle_request` to call, and its response
/// type carries no identifier — it inserts the tasks-DB row and discards the `TaskId`. Doing the
/// insert here is what lets the endpoint answer with the task id the issue requires. The dispatch
/// arms below must stay in step with that command.
async fn handle_generate_image(
  app_handle: &tauri::AppHandle,
  body: Result<Json<ControlGenerateImageRequest>, JsonRejection>,
) -> Result<ControlSuccessResponse<EnqueuedTaskResponse>, ControlErrorResponse> {
  let request = read_json_body(body)?.into_command_request()?;

  let app_env_configs = require_tauri_state::<AppEnvConfigs>(app_handle)?;
  let credential_cache = require_tauri_state::<ProviderCredentialLoadingCache>(app_handle)?;
  let storyteller_creds_manager = require_tauri_state::<StorytellerCredentialManager>(app_handle)?;
  let task_database = require_tauri_state::<TaskDatabase>(app_handle)?;

  let provider = request.provider.unwrap_or(GenerationProvider::Artcraft);

  let result = match provider {
    GenerationProvider::Artcraft => {
      handle_artcraft(&request, &app_env_configs, &storyteller_creds_manager).await
    }
    // Midjourney uses its own legacy command path, not this one.
    GenerationProvider::Midjourney => {
      Err(GenerateError::NotYetImplemented(MIDJOURNEY_UNSUPPORTED.to_string()))
    }
    other => {
      handle_router(
        &request,
        other,
        &app_env_configs,
        &credential_cache,
        &storyteller_creds_manager,
      ).await
    }
  };

  let success = result.map_err(|err| generate_error_to_control_response(ENDPOINT, err))?;

  let insert_result = success
    .insert_into_task_database_with_frontend_payload(
      &task_database,
      request.frontend_caller,
      request.frontend_subscriber_id.as_deref(),
      request.frontend_subscriber_payload.as_deref(),
    )
    .await;

  // NB: Fail open, exactly as the command does — the provider already holds the job, so a failed
  // local bookkeeping insert must not be reported as a failed generation.
  let task_id = match insert_result {
    Ok(task_id) => Some(task_id),
    Err(err) => {
      error!("[ControlServer] Failed to create task in database: {:?}", err);
      None
    }
  };

  notify_frontend_of_enqueue_success(app_handle, &success);

  Ok(ControlSuccessResponse::new(
    EnqueuedTaskResponse::from_enqueue_success_with_task_id(&success, task_id),
  ))
}

#[cfg(test)]
mod tests {
  use super::*;

  const PNG_PIXEL_BASE64: &str = "iVBORw0KGgo=";

  mod mapping_tests {
    use super::*;

    #[test]
    fn test_minimal_body_maps_onto_the_command_request() {
      let body: ControlGenerateImageRequest = serde_json::from_str(r#"{
        "provider": "artcraft",
        "model": "nano_banana",
        "prompt": "a red cube on white background",
        "batch_size": 1
      }"#).expect("body should deserialize");

      let request = expect_mapped(body.into_command_request());

      assert_eq!(request.provider, Some(GenerationProvider::Artcraft));
      assert_eq!(request.prompt.as_deref(), Some("a red cube on white background"));
      assert_eq!(request.batch_size, Some(1));
      assert!(request.model.is_some());
      assert!(request.canvas_image_raw_bytes.is_none());
      assert!(request.frontend_caller.is_none());
    }

    #[test]
    fn test_base64_image_fields_decode_into_raw_bytes() {
      let body = ControlGenerateImageRequest {
        model: Some(TauriImageModel::NanoBanana),
        canvas_image_base64: Some(PNG_PIXEL_BASE64.to_string()),
        scene_image_base64: Some(format!("data:image/png;base64,{}", PNG_PIXEL_BASE64)),
        ..Default::default()
      };

      let request = expect_mapped(body.into_command_request());

      assert_eq!(request.canvas_image_raw_bytes.as_deref(), Some([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A].as_slice()));
      assert_eq!(request.scene_image_raw_bytes, request.canvas_image_raw_bytes);
    }

    #[test]
    fn test_frontend_routing_fields_are_passed_through_untouched() {
      let body = ControlGenerateImageRequest {
        model: Some(TauriImageModel::NanoBanana),
        frontend_caller: Some(TauriCommandCaller::Canvas),
        frontend_subscriber_id: Some("sub-1".to_string()),
        frontend_subscriber_payload: Some("{\"k\":1}".to_string()),
        ..Default::default()
      };

      let request = expect_mapped(body.into_command_request());

      assert_eq!(request.frontend_caller, Some(TauriCommandCaller::Canvas));
      assert_eq!(request.frontend_subscriber_id.as_deref(), Some("sub-1"));
      assert_eq!(request.frontend_subscriber_payload.as_deref(), Some("{\"k\":1}"));
    }
  }

  mod validation_tests {
    use super::*;

    #[test]
    fn test_empty_body_is_rejected_for_a_missing_model() {
      let body: ControlGenerateImageRequest =
        serde_json::from_str("{}").expect("an empty object should deserialize");

      assert_bad_request(body.into_command_request());
    }

    #[test]
    fn test_a_token_and_base64_for_the_same_image_are_rejected() {
      let body = ControlGenerateImageRequest {
        model: Some(TauriImageModel::NanoBanana),
        canvas_image_media_token: Some(MediaFileToken::new_from_str("mf_test")),
        canvas_image_base64: Some(PNG_PIXEL_BASE64.to_string()),
        ..Default::default()
      };

      assert_bad_request(body.into_command_request());
    }

    #[test]
    fn test_malformed_base64_is_rejected() {
      let body = ControlGenerateImageRequest {
        model: Some(TauriImageModel::NanoBanana),
        canvas_image_base64: Some("not base64!!".to_string()),
        ..Default::default()
      };

      assert_bad_request(body.into_command_request());
    }
  }

  // NB: `ControlErrorResponse` is not `Debug` (it is a wire type), so these stand in for
  // `unwrap`/`expect` rather than widening the envelope's derives for tests alone.
  fn expect_mapped(
    result: Result<TauriGenerateImageRequest, ControlErrorResponse>,
  ) -> TauriGenerateImageRequest {
    match result {
      Ok(request) => request,
      Err(error) => panic!("expected success, got {}", error.error.code.to_str()),
    }
  }

  fn assert_bad_request(result: Result<TauriGenerateImageRequest, ControlErrorResponse>) {
    match result {
      Ok(_) => panic!("expected a rejection"),
      Err(error) => assert_eq!(error.error.code, ControlErrorCode::BadRequest),
    }
  }
}
