use crate::core::commands::enqueue::task_enqueue_success::TaskEnqueueSuccess;
use crate::core::events::basic_sendable_event_trait::BasicSendableEvent;
use crate::core::events::functional_events::credits_balance_changed_event::CreditsBalanceChangedEvent;
use crate::core::events::generation_events::common::GenerationModel;
use crate::core::events::generation_events::generation_enqueue_success_event::GenerationEnqueueSuccessEvent;
use crate::core::state::task_database::TaskDatabase;
use log::{error, warn};
use serde::Serialize;
use sqlite_tasks::queries::get_task_by_provider_and_provider_job_id::{
  get_task_by_provider_and_provider_job_id, GetTaskByProviderAndProviderJobIdArgs,
};
use tauri::AppHandle;
use tokens::tokens::sqlite::tasks::TaskId;

/// What every `/v1/generate/*` endpoint returns on success: enough for a client to poll the task
/// queue (HM-919) for this specific job.
#[derive(Serialize)]
pub struct EnqueuedTaskResponse {
  /// The tasks-DB row id. `None` only when the row could not be located (see `resolve_task_id`).
  pub task_id: Option<String>,
  pub task_type: &'static str,
  pub provider: &'static str,
  pub provider_job_id: Option<String>,
  pub model: Option<GenerationModel>,
}

impl EnqueuedTaskResponse {
  /// Builds the response for a job whose tasks-DB row was inserted by the command's own
  /// `handle_request`, which discards the `TaskId` — so it is recovered by lookup.
  pub async fn from_enqueue_success(
    task_database: &TaskDatabase,
    success: &TaskEnqueueSuccess,
  ) -> Self {
    let task_id = resolve_task_id(task_database, success).await;

    Self::from_enqueue_success_with_task_id(success, task_id)
  }

  /// Builds the response when the caller performed the tasks-DB insert itself and therefore
  /// already holds the authoritative `TaskId`.
  pub fn from_enqueue_success_with_task_id(
    success: &TaskEnqueueSuccess,
    task_id: Option<TaskId>,
  ) -> Self {
    Self {
      task_id: task_id.map(|id| id.as_str().to_string()),
      task_type: success.task_type.to_str(),
      provider: success.provider.to_str(),
      provider_job_id: success.provider_job_id.clone(),
      model: success.model,
    }
  }
}

/// Re-emits the two frontend events the Tauri commands emit on a successful enqueue, so a job
/// started over HTTP lands in the running app's task queue and credit display exactly like one
/// started from the UI.
pub fn notify_frontend_of_enqueue_success(
  app_handle: &AppHandle,
  success: &TaskEnqueueSuccess,
) {
  let event = GenerationEnqueueSuccessEvent {
    action: success.to_frontend_event_action(),
    service: success.to_frontend_event_service(),
    model: success.model,
  };

  // NB: Fail open — the job is already enqueued, so a failed UI notification must not turn a
  // successful enqueue into an error response.
  if let Err(err) = event.send(app_handle) {
    error!("[ControlServer] Failed to emit generation enqueue event: {:?}", err);
  }

  CreditsBalanceChangedEvent {}.send_infallible(app_handle);
}

/// Recovers the tasks-DB id for a just-enqueued job.
///
/// NB: `insert_into_task_database_with_frontend_payload` returns the `TaskId`, but every command's
/// `handle_request` drops it, and widening those signatures is out of scope for this issue. The
/// `(provider, provider_job_id)` pair is unique per enqueue, so the row is looked up by it.
/// Everything here fails open to `None`: the job IS enqueued by this point, and a missing id
/// degrades the response rather than the generation.
async fn resolve_task_id(
  task_database: &TaskDatabase,
  success: &TaskEnqueueSuccess,
) -> Option<TaskId> {
  let Some(provider_job_id) = success.provider_job_id.as_deref() else {
    warn!("[ControlServer] Enqueued job has no provider job id; cannot report a task id.");
    return None;
  };

  let result = get_task_by_provider_and_provider_job_id(GetTaskByProviderAndProviderJobIdArgs {
    db: task_database.get_connection(),
    provider: success.provider,
    provider_job_id,
  }).await;

  match result {
    Ok(Some(task)) => Some(task.id),
    Ok(None) => {
      warn!("[ControlServer] Enqueued job has no tasks database row; cannot report a task id.");
      None
    }
    Err(err) => {
      warn!("[ControlServer] Failed to read back the enqueued task: {:?}", err);
      None
    }
  }
}
