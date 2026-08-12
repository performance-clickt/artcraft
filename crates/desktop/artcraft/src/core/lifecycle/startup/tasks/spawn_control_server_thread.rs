use crate::core::control_server::endpoints::generate::generation_router::build_generation_router;
use crate::core::control_server::endpoints::health::get_health_handler;
use crate::core::control_server::endpoints::read_endpoints_router::read_endpoints_router; // HM-917
use crate::core::control_server::endpoints::task_and_media_routes::build_task_and_media_router;
use crate::core::control_server::endpoints::scene::post_scene_handler;
use crate::core::control_server::enveloped_fallback::seal_control_router; // HM-934
use crate::core::control_server::state::control_server_settings::ControlServerSettings;
use crate::core::control_server::state_file::write_control_state_file::write_control_state_file;
use crate::core::state::data_dir::app_data_root::AppDataRoot;
use axum::routing::{get, post};
use axum::Router;
use errors::AnyhowResult;
use log::{error, info};
use tauri::AppHandle;
use tokio::net::TcpListener;

const CONTROL_SERVER_BIND_ADDRESS: &str = "127.0.0.1:0";
const CONTROL_SERVER_HEALTH_PATH: &str = "/v1/health";
const CONTROL_SERVER_SCENE_PATH: &str = "/v1/scene/{op}"; // HM-920

pub fn spawn_control_server_thread(
  app: &AppHandle,
  root: &AppDataRoot,
) -> AnyhowResult<()> {

  tauri::async_runtime::spawn(control_server_thread(
    app.clone(),
    root.clone(),
  ));

  Ok(())
}

/// Log-and-continue: the control server is an optional integration surface, so a bind or serve
/// failure must never take the app down with it.
async fn control_server_thread(
  app_handle: AppHandle,
  app_data_root: AppDataRoot,
) {
  if let Err(err) = run_control_server(app_handle, &app_data_root).await {
    error!("[ControlServer] Control server stopped: {:?}", err);
  }
}

async fn run_control_server(
  app_handle: AppHandle,
  app_data_root: &AppDataRoot,
) -> AnyhowResult<()> {
  // Bind on an ephemeral port first: the assigned port is part of what we publish.
  let listener = TcpListener::bind(CONTROL_SERVER_BIND_ADDRESS).await?;
  let port = listener.local_addr()?.port();

  let settings = ControlServerSettings::new_with_generated_token(port);
  let state_file_path = write_control_state_file(app_data_root, &settings)?;

  info!(
    "[ControlServer] Listening on 127.0.0.1:{}, discovery file: {:?}",
    port,
    state_file_path,
  );

  let router = build_control_router(app_handle, &settings);
  axum::serve(listener, router).await?;

  Ok(())
}

/// NB: Every future endpoint must be added to the `route` chain ABOVE the `seal_control_router`
/// call; a route mounted after it is neither authenticated nor covered by the 405 fallback.
fn build_control_router(
  app_handle: AppHandle,
  settings: &ControlServerSettings,
) -> Router {
  let routes = Router::new()
    .route(CONTROL_SERVER_HEALTH_PATH, get(get_health_handler))
    .merge(read_endpoints_router()) // HM-917
    .merge(build_generation_router()) // HM-918
    .merge(build_task_and_media_router()) // HM-919
    .route(CONTROL_SERVER_SCENE_PATH, post(post_scene_handler)); // HM-920

  // HM-934: Attaches the enveloped 404/405 catch-alls and the bearer auth layer together. They
  // are one call because their order is a security boundary — see `seal_control_router`.
  seal_control_router(routes, settings)
    .with_state(app_handle)
}
