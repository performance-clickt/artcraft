use crate::core::control_server::endpoints::generate::bg_removal::post_bg_removal_handler;
use crate::core::control_server::endpoints::generate::generate_image::post_generate_image_handler;
use crate::core::control_server::endpoints::generate::generate_object::post_generate_object_handler;
use crate::core::control_server::endpoints::generate::generate_video::post_generate_video_handler;
use crate::core::control_server::endpoints::generate::generate_world::post_generate_world_handler;
use axum::extract::DefaultBodyLimit;
use axum::routing::post;
use axum::Router;
use tauri::AppHandle;

const GENERATE_IMAGE_PATH: &str = "/v1/generate/image";
const GENERATE_VIDEO_PATH: &str = "/v1/generate/video";
const GENERATE_OBJECT_PATH: &str = "/v1/generate/object";
const GENERATE_WORLD_PATH: &str = "/v1/generate/world";
const GENERATE_BG_REMOVAL_PATH: &str = "/v1/generate/bg_removal";

/// Base64-encoded images inflate by ~4/3, and the canvas, scene, and inpainting-mask inputs can
/// all appear in one image request. Axum's default limit is 2 MB, which a single screen-sized PNG
/// already exceeds.
const GENERATION_BODY_LIMIT_BYTES: usize = 64 * 1024 * 1024;

/// The five generation entry points, mounted as one unit so `build_control_router` gains a single
/// line.
///
/// NB: The body limit is applied here rather than to the whole control router: it is these
/// endpoints that legitimately carry image payloads, and leaving the small default in place
/// everywhere else keeps the rest of the surface cheap to abuse.
pub fn build_generation_router() -> Router<AppHandle> {
  Router::new()
    .route(GENERATE_IMAGE_PATH, post(post_generate_image_handler))
    .route(GENERATE_VIDEO_PATH, post(post_generate_video_handler))
    .route(GENERATE_OBJECT_PATH, post(post_generate_object_handler))
    .route(GENERATE_WORLD_PATH, post(post_generate_world_handler))
    .route(GENERATE_BG_REMOVAL_PATH, post(post_bg_removal_handler))
    .layer(DefaultBodyLimit::max(GENERATION_BODY_LIMIT_BYTES))
}
