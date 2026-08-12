use crate::core::commands::task_queue::get_task_queue_command::{handle_request as list_task_queue_items, CompletedItemData, FailedItemData, TaskQueueItem};
use crate::core::control_server::endpoints::pagination::{parse_page_cursor, parse_page_limit, parse_raw_query, take_page};
use crate::core::control_server::envelope::control_response::{ControlErrorCode, ControlErrorResponse, ControlSuccessResponse};
use crate::core::control_server::require_tauri_state::require_tauri_state;
use crate::core::state::task_database::TaskDatabase;
use axum::extract::{Path, RawQuery, State};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use enums::common::generation_provider::GenerationProvider;
use enums::tauri::tasks::task_failure_type::TaskFailureType;
use enums::tauri::tasks::task_media_file_class::TaskMediaFileClass;
use enums::tauri::tasks::task_model_type::TaskModelType;
use enums::tauri::tasks::task_status::TaskStatus;
use enums::tauri::tasks::task_type::TaskType;
use log::warn;
use serde_derive::Serialize;
use sqlite_tasks::queries::get_task_by_id::get_task_by_id;
use sqlite_tasks::queries::list_tasks_for_frontend::TaskItem;
use tauri::AppHandle;
use tokens::tokens::media_files::MediaFileToken;
use tokens::tokens::sqlite::tasks::TaskId;

const TASK_DATABASE_READ_FAILED_MESSAGE: &str = "Failed to read the local task database.";
const TASK_NOT_FOUND_MESSAGE: &str = "No task exists with that id.";
const UNKNOWN_STATUS_MESSAGE: &str = "The `status` parameter is not a known task status.";

#[derive(Serialize)]
pub struct ListTasksResponse {
  pub tasks: Vec<TaskSummary>,

  /// Pass back as `?cursor=` to fetch the next page. `null` when this is the last page.
  pub next_cursor: Option<String>,
}

/// `GET /v1/tasks/{id}`
#[derive(Serialize)]
pub struct GetTaskResponse {
  pub task: TaskSummary,
}

/// The concise row an external agent needs to follow a job: what it is, where it is, and — once
/// it lands — how to fetch the result.
#[derive(Serialize)]
pub struct TaskSummary {
  pub id: TaskId,
  pub task_status: TaskStatus,
  pub task_type: TaskType,
  pub model_type: Option<TaskModelType>,
  pub provider: Option<GenerationProvider>,
  pub provider_job_id: Option<String>,

  /// Present once the task has completed successfully and the primary media file is known.
  pub result: Option<TaskResultSummary>,

  /// Present when the task reported a failure, terminal or otherwise.
  pub failure: Option<TaskFailureSummary>,

  pub created_at: DateTime<Utc>,
  pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
pub struct TaskResultSummary {
  /// Feed this to `POST /v1/media/download` to pull the file to disk.
  pub media_token: MediaFileToken,
  pub cdn_url: String,
  pub media_class: Option<TaskMediaFileClass>,
  pub thumbnail_url_template: Option<String>,
}

#[derive(Serialize)]
pub struct TaskFailureSummary {
  pub failure_type: TaskFailureType,
  pub message: Option<String>,
}

/// `GET /v1/tasks?limit=&cursor=&status=`
pub async fn list_tasks_handler(
  State(app_handle): State<AppHandle>,
  RawQuery(raw_query): RawQuery,
) -> Response {
  let params = parse_raw_query(raw_query.as_deref());

  let page_limit = match parse_page_limit(params.get("limit").map(String::as_str)) {
    Ok(page_limit) => page_limit,
    Err(message) => {
      return ControlErrorResponse::new(ControlErrorCode::BadRequest, message).into_response();
    }
  };

  let page_index = match parse_page_cursor(params.get("cursor").map(String::as_str)) {
    Ok(page_index) => page_index,
    Err(message) => {
      return ControlErrorResponse::new(ControlErrorCode::BadRequest, message).into_response();
    }
  };

  let maybe_status = params.get("status")
      .map(|status| status.trim())
      .filter(|status| !status.is_empty());

  let maybe_status_filter = match maybe_status {
    None => None,
    Some(status) => match TaskStatus::from_str(status) {
      Ok(status) => Some(status),
      Err(_) => {
        return ControlErrorResponse::new(ControlErrorCode::BadRequest, UNKNOWN_STATUS_MESSAGE)
          .into_response();
      }
    },
  };

  let summaries = match read_task_summaries(&app_handle).await {
    Ok(summaries) => summaries,
    Err(response) => return response,
  };

  let summaries = match maybe_status_filter {
    None => summaries,
    Some(status) => summaries.into_iter()
        .filter(|summary| summary.task_status == status)
        .collect(),
  };

  let (tasks, next_cursor) = take_page(summaries, page_index, page_limit);

  ControlSuccessResponse::new(ListTasksResponse {
    tasks,
    next_cursor,
  }).into_response()
}

/// `GET /v1/tasks/{id}`
///
/// NB: This reads the row by id rather than scanning the visible task list. The list query hides
/// tasks the user dismissed in the app, which would turn a running job into a permanent 404 the
/// moment its card is cleared — indistinguishable, to a polling caller, from an id that never
/// existed. It is also the difference between one indexed row read and a full-table read plus a
/// sort on every poll.
pub async fn get_task_handler(
  State(app_handle): State<AppHandle>,
  Path(task_id): Path<String>,
) -> Response {
  let task_database = match require_tauri_state::<TaskDatabase>(&app_handle) {
    Ok(state) => state,
    Err(error) => return error.into_response(),
  };

  let maybe_item = match get_task_by_id(task_database.get_connection(), &task_id).await {
    Ok(maybe_item) => maybe_item,
    Err(err) => {
      warn!("[ControlServer] Failed to read task {}: {:?}", task_id, err);

      return ControlErrorResponse::new(ControlErrorCode::Internal, TASK_DATABASE_READ_FAILED_MESSAGE)
        .into_response();
    }
  };

  let Some(item) = maybe_item else {
    return ControlErrorResponse::new(ControlErrorCode::TaskNotFound, TASK_NOT_FOUND_MESSAGE)
      .into_response();
  };

  ControlSuccessResponse::new(GetTaskResponse {
    task: to_task_summary_from_item(item),
  }).into_response()
}

/// Reads every visible task, newest first.
///
/// NB: This delegates to the Tauri task-queue command's own `handle_request` rather than
/// re-querying the SQLite database, so the control surface can never drift from the task list the
/// app itself shows the user. That also means dismissed tasks are excluded here exactly as they
/// are in the app.
///
/// The `Err` arm is the ready-to-return error envelope, so callers just forward it.
async fn read_task_summaries(app_handle: &AppHandle) -> Result<Vec<TaskSummary>, Response> {
  let task_database = require_tauri_state::<TaskDatabase>(app_handle)
      .map_err(IntoResponse::into_response)?;

  let items = match list_task_queue_items(&task_database).await {
    Ok(items) => items,
    Err(err) => {
      warn!("[ControlServer] Failed to list tasks: {:?}", err);

      return Err(
        ControlErrorResponse::new(ControlErrorCode::Internal, TASK_DATABASE_READ_FAILED_MESSAGE)
          .into_response()
      );
    }
  };

  Ok(to_ordered_task_summaries(items))
}

/// NB: The underlying query has no `ORDER BY`, so ordering is imposed here. Paging over an
/// unordered result set would silently repeat and skip rows between pages.
fn to_ordered_task_summaries(items: Vec<TaskQueueItem>) -> Vec<TaskSummary> {
  let mut summaries: Vec<TaskSummary> = items.into_iter()
      .map(to_task_summary)
      .collect();

  // Newest first, with the id as a tiebreaker so equal timestamps still order deterministically.
  summaries.sort_by(|left, right| {
    right.created_at.cmp(&left.created_at)
        .then_with(|| right.id.as_str().cmp(left.id.as_str()))
  });

  summaries
}

fn to_task_summary(item: TaskQueueItem) -> TaskSummary {
  TaskSummary {
    id: item.id,
    task_status: item.task_status,
    task_type: item.task_type,
    model_type: item.model_type,
    provider: item.provider,
    provider_job_id: item.provider_job_id,
    result: item.completed_item.map(to_task_result_summary),
    failure: item.failure_reason.map(to_task_failure_summary),
    created_at: item.created_at,
    completed_at: item.completed_at,
  }
}

/// The by-id path reads the database row directly, so the completed/failed shaping the task-queue
/// command does for the list path is repeated here.
///
/// NB: `completed_item` is only populated for a successful task that carries both a media token
/// and a CDN URL — the same rule the command applies — so a half-written completion row never
/// surfaces as a fetchable result on either path.
fn to_task_summary_from_item(item: TaskItem) -> TaskSummary {
  let is_complete_success = item.status == TaskStatus::CompleteSuccess;

  let mut result = None;
  let mut failure = None;

  if is_complete_success {
    let token_and_url = item.on_complete_primary_media_file_token
        .zip(item.on_complete_primary_media_file_cdn_url);

    if let Some((media_token, cdn_url)) = token_and_url {
      result = Some(TaskResultSummary {
        media_token,
        cdn_url,
        media_class: item.on_complete_primary_media_file_class,
        thumbnail_url_template: item.on_complete_primary_media_file_thumbnail_url_template,
      });
    }
  } else if item.on_failure_type.is_some() || item.on_failure_message.is_some() {
    failure = Some(TaskFailureSummary {
      failure_type: item.on_failure_type.unwrap_or(TaskFailureType::Unknown),
      message: item.on_failure_message,
    });
  }

  TaskSummary {
    id: item.id,
    task_status: item.status,
    task_type: item.task_type,
    model_type: item.model_type,
    provider: item.provider,
    provider_job_id: item.provider_job_id,
    result,
    failure,
    created_at: item.created_at,
    completed_at: item.completed_at,
  }
}

/// NB: The upstream item only carries a `completed_item` for a successfully completed task that
/// has both a media token and a CDN URL, so a half-written completion row never surfaces here as
/// a fetchable result.
fn to_task_result_summary(completed: CompletedItemData) -> TaskResultSummary {
  TaskResultSummary {
    media_token: completed.primary_media_file.token,
    cdn_url: completed.primary_media_file.cdn_url,
    media_class: completed.media_file_class,
    thumbnail_url_template: completed.primary_media_file.maybe_thumbnail_url_template,
  }
}

fn to_task_failure_summary(failure: FailedItemData) -> TaskFailureSummary {
  TaskFailureSummary {
    failure_type: failure.failure_type,
    message: failure.failure_message,
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::commands::task_queue::get_task_queue_command::MediaFileData;
  use chrono::TimeZone;

  const CDN_URL: &str = "https://cdn.example.com/files/abc123.png";
  const MEDIA_TOKEN: &str = "m_abc123";
  const THUMBNAIL_TEMPLATE: &str = "https://cdn.example.com/t/abc123/{WIDTH}.png";

  mod ordering_tests {
    use super::*;

    #[test]
    fn test_summaries_are_newest_first() {
      let summaries = to_ordered_task_summaries(vec![
        task_queue_item("task_older", 100),
        task_queue_item("task_newest", 300),
        task_queue_item("task_middle", 200),
      ]);

      let ids: Vec<&str> = summaries.iter()
          .map(|summary| summary.id.as_str())
          .collect();

      assert_eq!(ids, vec!["task_newest", "task_middle", "task_older"]);
    }

    #[test]
    fn test_equal_timestamps_order_deterministically_by_id() {
      let summaries = to_ordered_task_summaries(vec![
        task_queue_item("task_aaa", 100),
        task_queue_item("task_ccc", 100),
        task_queue_item("task_bbb", 100),
      ]);

      let ids: Vec<&str> = summaries.iter()
          .map(|summary| summary.id.as_str())
          .collect();

      assert_eq!(ids, vec!["task_ccc", "task_bbb", "task_aaa"]);
    }
  }

  mod mapping_tests {
    use super::*;

    #[test]
    fn test_completed_item_becomes_a_fetchable_result() {
      let mut item = task_queue_item("task_done", 100);
      item.task_status = TaskStatus::CompleteSuccess;
      item.completed_item = Some(CompletedItemData {
        primary_media_file: MediaFileData {
          token: MediaFileToken::new_from_str(MEDIA_TOKEN),
          cdn_url: CDN_URL.to_string(),
          maybe_thumbnail_url_template: Some(THUMBNAIL_TEMPLATE.to_string()),
          created_at: timestamp(100),
        },
        media_file_class: Some(TaskMediaFileClass::Image),
        maybe_batch_token: None,
      });

      let summary = to_task_summary(item);
      let result = summary.result.expect("a completed item yields a result");

      assert_eq!(result.media_token.as_str(), MEDIA_TOKEN);
      assert_eq!(result.cdn_url, CDN_URL);
      assert_eq!(result.thumbnail_url_template.as_deref(), Some(THUMBNAIL_TEMPLATE));
      assert!(summary.failure.is_none());
    }

    #[test]
    fn test_pending_task_has_neither_result_nor_failure() {
      let summary = to_task_summary(task_queue_item("task_pending", 100));

      assert!(summary.result.is_none());
      assert!(summary.failure.is_none());
    }

    #[test]
    fn test_failure_reason_is_carried_through() {
      let mut item = task_queue_item("task_failed", 100);
      item.task_status = TaskStatus::CompleteFailure;
      item.failure_reason = Some(FailedItemData {
        failure_type: TaskFailureType::Unknown,
        failure_message: Some("boom".to_string()),
      });

      let summary = to_task_summary(item);
      let failure = summary.failure.expect("a failed item yields a failure");

      assert_eq!(failure.failure_type, TaskFailureType::Unknown);
      assert_eq!(failure.message.as_deref(), Some("boom"));
      assert!(summary.result.is_none());
    }
  }

  mod by_id_mapping_tests {
    use super::*;

    #[test]
    fn test_a_completed_row_becomes_a_fetchable_result() {
      let mut item = task_item("task_done");
      item.status = TaskStatus::CompleteSuccess;
      item.on_complete_primary_media_file_token = Some(MediaFileToken::new_from_str(MEDIA_TOKEN));
      item.on_complete_primary_media_file_cdn_url = Some(CDN_URL.to_string());

      let summary = to_task_summary_from_item(item);
      let result = summary.result.expect("a completed row yields a result");

      assert_eq!(result.media_token.as_str(), MEDIA_TOKEN);
      assert_eq!(result.cdn_url, CDN_URL);
      assert!(summary.failure.is_none());
    }

    #[test]
    fn test_a_completed_row_without_a_cdn_url_yields_no_result() {
      let mut item = task_item("task_half_written");
      item.status = TaskStatus::CompleteSuccess;
      item.on_complete_primary_media_file_token = Some(MediaFileToken::new_from_str(MEDIA_TOKEN));

      let summary = to_task_summary_from_item(item);

      assert!(summary.result.is_none());
      assert!(summary.failure.is_none());
    }

    #[test]
    fn test_a_failure_row_carries_its_reason() {
      let mut item = task_item("task_failed");
      item.status = TaskStatus::CompleteFailure;
      item.on_failure_type = Some(TaskFailureType::Unknown);
      item.on_failure_message = Some("boom".to_string());

      let summary = to_task_summary_from_item(item);
      let failure = summary.failure.expect("a failed row yields a failure");

      assert_eq!(failure.failure_type, TaskFailureType::Unknown);
      assert_eq!(failure.message.as_deref(), Some("boom"));
      assert!(summary.result.is_none());
    }

    #[test]
    fn test_a_pending_row_has_neither_result_nor_failure() {
      let summary = to_task_summary_from_item(task_item("task_pending"));

      assert!(summary.result.is_none());
      assert!(summary.failure.is_none());
    }

    fn task_item(id: &str) -> TaskItem {
      TaskItem {
        id: TaskId::new_from_str(id),
        status: TaskStatus::Pending,
        task_type: TaskType::ImageGeneration,
        model_type: None,
        provider: None,
        provider_job_id: None,
        frontend_caller: None,
        frontend_subscriber_id: None,
        frontend_subscriber_payload: None,
        on_complete_primary_media_file_token: None,
        on_complete_primary_media_file_class: None,
        on_complete_batch_token: None,
        on_complete_primary_media_file_cdn_url: None,
        on_complete_primary_media_file_thumbnail_url_template: None,
        on_failure_type: None,
        on_failure_message: None,
        created_at: timestamp(100),
        updated_at: timestamp(100),
        completed_at: None,
      }
    }
  }

  fn task_queue_item(id: &str, created_at_seconds: i64) -> TaskQueueItem {
    TaskQueueItem {
      id: TaskId::new_from_str(id),
      task_status: TaskStatus::Pending,
      task_type: TaskType::ImageGeneration,
      model_type: None,
      provider: None,
      provider_job_id: None,
      completed_item: None,
      failure_reason: None,
      created_at: timestamp(created_at_seconds),
      updated_at: timestamp(created_at_seconds),
      completed_at: None,
    }
  }

  fn timestamp(seconds: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(seconds, 0)
        .single()
        .expect("the test timestamp is valid")
  }
}
