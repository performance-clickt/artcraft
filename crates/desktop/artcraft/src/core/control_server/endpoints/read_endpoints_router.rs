use crate::core::control_server::endpoints::credits::get_credits_handler;
use crate::core::control_server::endpoints::estimate_cost::post_estimate_cost_handler;
use crate::core::control_server::endpoints::models::get_models_handler;
use axum::routing::{get, post};
use axum::Router;
use tauri::AppHandle;

const CONTROL_SERVER_MODELS_PATH: &str = "/v1/models";
const CONTROL_SERVER_CREDITS_PATH: &str = "/v1/credits";
const CONTROL_SERVER_ESTIMATE_COST_PATH: &str = "/v1/estimate_cost";

/// The read-only catalog surface: what can be generated, what it costs, what the account can
/// afford. Grouped into one router so the control server's route chain gains a single line per
/// endpoint group rather than one per path.
///
/// NB: This router is merged into the main chain ABOVE the auth `layer`. Merging it after that
/// call would leave these endpoints unauthenticated.
pub fn read_endpoints_router() -> Router<AppHandle> {
  Router::new()
    .route(CONTROL_SERVER_MODELS_PATH, get(get_models_handler))
    .route(CONTROL_SERVER_CREDITS_PATH, get(get_credits_handler))
    .route(CONTROL_SERVER_ESTIMATE_COST_PATH, post(post_estimate_cost_handler))
}
