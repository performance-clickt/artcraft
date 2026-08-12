use crate::core::control_server::auth::bearer_auth_layer::bearer_auth_layer;
use crate::core::control_server::endpoints::generate::generation_router::build_generation_router;
use crate::core::control_server::endpoints::health::get_health_handler;
use crate::core::control_server::state::control_server_settings::ControlServerSettings;
use crate::core::control_server::state_file::write_control_state_file::write_control_state_file;
use crate::core::state::data_dir::app_data_root::AppDataRoot;
use axum::routing::get;
use axum::{middleware, Router};
use errors::AnyhowResult;
use log::{error, info};
use tauri::AppHandle;
use tokio::net::TcpListener;

const CONTROL_SERVER_BIND_ADDRESS: &str = "127.0.0.1:0";
const CONTROL_SERVER_HEALTH_PATH: &str = "/v1/health";

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

/// NB: `layer` (not `route_layer`) is deliberate — it authenticates unmatched paths too, so an
/// unauthenticated caller cannot probe which routes exist. Every future endpoint must be added
/// to the `route` chain ABOVE this `layer` call; a route mounted after it is NOT authenticated.
fn build_control_router(
  app_handle: AppHandle,
  settings: &ControlServerSettings,
) -> Router {
  Router::new()
    .route(CONTROL_SERVER_HEALTH_PATH, get(get_health_handler))
    .merge(build_generation_router()) // HM-918
    .layer(middleware::from_fn_with_state(settings.clone(), bearer_auth_layer))
    .with_state(app_handle)
}
