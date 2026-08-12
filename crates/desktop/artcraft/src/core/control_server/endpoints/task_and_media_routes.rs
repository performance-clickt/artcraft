use crate::core::control_server::endpoints::media_download::download_media_handler;
use crate::core::control_server::endpoints::media_list::list_media_handler;
use crate::core::control_server::endpoints::tasks::{get_task_handler, list_tasks_handler};
use axum::routing::{get, post};
use axum::Router;
use tauri::AppHandle;

const TASKS_PATH: &str = "/v1/tasks";
const TASK_BY_ID_PATH: &str = "/v1/tasks/{id}";
const MEDIA_PATH: &str = "/v1/media";
const MEDIA_DOWNLOAD_PATH: &str = "/v1/media/download";

/// The task and media surface, mounted as one unit.
///
/// NB: This is merged into the control router ABOVE its auth `.layer(...)` call. A route merged
/// after that layer would not be authenticated.
pub fn build_task_and_media_router() -> Router<AppHandle> {
  Router::new()
    .route(TASKS_PATH, get(list_tasks_handler))
    .route(TASK_BY_ID_PATH, get(get_task_handler))
    .route(MEDIA_PATH, get(list_media_handler))
    .route(MEDIA_DOWNLOAD_PATH, post(download_media_handler))
}
