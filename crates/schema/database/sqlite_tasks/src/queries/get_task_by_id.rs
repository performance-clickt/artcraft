use crate::connection::TaskDbConnection;
use crate::error::SqliteTasksError;
use crate::queries::list_tasks_for_frontend::TaskItem;
use chrono::{DateTime, TimeZone, Utc};
use enums::common::generation_provider::GenerationProvider;
use enums::tauri::tasks::task_failure_type::TaskFailureType;
use enums::tauri::tasks::task_media_file_class::TaskMediaFileClass;
use enums::tauri::tasks::task_model_type::TaskModelType;
use enums::tauri::tasks::task_status::TaskStatus;
use enums::tauri::tasks::task_type::TaskType;
use enums::tauri::ux::tauri_command_caller::TauriCommandCaller;
use sqlx::Row;
use tokens::tokens::batch_generations::BatchGenerationToken;
use tokens::tokens::media_files::MediaFileToken;
use tokens::tokens::sqlite::tasks::TaskId;

/// Fetches one task by its id, dismissed or not.
///
/// NB: `list_tasks_for_frontend` filters `is_dismissed_by_user == 0` because the app's task list
/// hides dismissed cards. A caller holding a task id is following a specific job, so hiding the
/// row from *it* turns "the user cleared the list" into "that task never existed" while the
/// generation is still running. This query deliberately has no dismissal filter.
///
/// NB: This is a runtime-checked query, not `sqlx::query!`. The compile-time macros need a
/// prepared-query cache entry, and generating one requires a live database this build does not
/// have; every column is read explicitly below instead. `created_at`/`updated_at`/`completed_at`
/// are `INTEGER` unix seconds in the schema, so they are read as `i64` and converted here.
pub async fn get_task_by_id(
  db: &TaskDbConnection,
  task_id: &str,
) -> Result<Option<TaskItem>, SqliteTasksError> {
  let maybe_row = sqlx::query(r#"
    SELECT
      id,
      task_status,
      task_type,
      model_type,
      provider,
      provider_job_id,
      frontend_caller,
      frontend_subscriber_id,
      frontend_subscriber_payload,
      on_complete_primary_media_file_token,
      on_complete_primary_media_file_class,
      on_complete_batch_token,
      on_complete_primary_media_file_cdn_url,
      on_complete_primary_media_file_thumbnail_url_template,
      on_failure_type,
      on_failure_message,
      created_at,
      updated_at,
      completed_at
    FROM tasks
    WHERE id = ?
  "#)
      .bind(task_id)
      .fetch_optional(db.get_pool())
      .await?;

  let Some(row) = maybe_row else {
    return Ok(None);
  };

  let status: String = row.try_get("task_status")?;
  let task_type: String = row.try_get("task_type")?;
  let model_type: Option<String> = row.try_get("model_type")?;
  let provider: Option<String> = row.try_get("provider")?;
  let frontend_caller: Option<String> = row.try_get("frontend_caller")?;
  let media_file_class: Option<String> = row.try_get("on_complete_primary_media_file_class")?;
  let failure_type: Option<String> = row.try_get("on_failure_type")?;

  let media_file_token: Option<String> = row.try_get("on_complete_primary_media_file_token")?;
  let batch_token: Option<String> = row.try_get("on_complete_batch_token")?;

  Ok(Some(TaskItem {
    id: TaskId::new(row.try_get("id")?),
    status: TaskStatus::from_str(&status)?,
    task_type: TaskType::from_str(&task_type)?,
    model_type: model_type
        .map(|model| TaskModelType::from_str(&model))
        .transpose()?,
    provider: provider
        .map(|provider| GenerationProvider::from_str(&provider))
        .transpose()?,
    provider_job_id: row.try_get("provider_job_id")?,
    frontend_caller: frontend_caller
        .map(|caller| TauriCommandCaller::from_str(&caller))
        .transpose()?,
    frontend_subscriber_id: row.try_get("frontend_subscriber_id")?,
    frontend_subscriber_payload: row.try_get("frontend_subscriber_payload")?,
    on_complete_primary_media_file_token: media_file_token
        .map(|token| MediaFileToken::new_from_str(&token)),
    on_complete_primary_media_file_class: media_file_class
        .map(|class| TaskMediaFileClass::from_str(&class))
        .transpose()?,
    on_complete_batch_token: batch_token
        .map(|token| BatchGenerationToken::new_from_str(&token)),
    on_complete_primary_media_file_cdn_url: row.try_get("on_complete_primary_media_file_cdn_url")?,
    on_complete_primary_media_file_thumbnail_url_template:
        row.try_get("on_complete_primary_media_file_thumbnail_url_template")?,
    on_failure_type: failure_type
        .map(|failure| TaskFailureType::from_str(&failure))
        .transpose()?,
    on_failure_message: row.try_get("on_failure_message")?,
    created_at: to_timestamp(row.try_get("created_at")?),
    updated_at: to_timestamp(row.try_get("updated_at")?),
    completed_at: row.try_get::<Option<i64>, _>("completed_at")?.map(to_timestamp),
  }))
}

/// NB: Falls back to the epoch rather than failing the lookup — a row with an out-of-range
/// timestamp is still a task the caller is entitled to see the status of.
fn to_timestamp(unix_seconds: i64) -> DateTime<Utc> {
  Utc.timestamp_opt(unix_seconds, 0)
      .single()
      .unwrap_or_else(|| Utc.timestamp_opt(0, 0).single().expect("the epoch is a valid timestamp"))
}
