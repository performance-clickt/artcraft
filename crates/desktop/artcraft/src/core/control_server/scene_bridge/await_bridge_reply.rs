use crate::core::control_server::state::control_bridge_state::{ControlBridgeReply, PendingSceneRequest};
use log::debug;
use std::time::Duration;
use tokio::time::timeout;

/// How long an HTTP caller waits for the webview to answer. Long enough for a scene to be
/// serialized on a busy render thread, short enough that a closed window does not hang a client.
pub const SCENE_BRIDGE_TIMEOUT_SECONDS: u64 = 10;

/// Waits for the webview's reply to a registered scene request.
///
/// The `pending` guard is taken by value and dropped when this returns, which is what releases
/// the correlation-map entry — on a reply, on a timeout, and on the task being dropped out from
/// under us because the HTTP client hung up.
pub async fn await_bridge_reply(
  pending: PendingSceneRequest<'_>,
) -> Result<ControlBridgeReply, AwaitBridgeReplyError> {
  let timeout_duration = Duration::from_secs(SCENE_BRIDGE_TIMEOUT_SECONDS);

  await_bridge_reply_with_timeout(pending, timeout_duration).await
}

/// The failure modes the endpoint has to turn into envelopes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AwaitBridgeReplyError {
  /// Nobody answered within the timeout — usually no webview listener at all.
  TimedOut,
  /// The sender was dropped without a reply. Only reachable if the map entry is removed by
  /// something other than a reply, so it means a bug on our side, not a quiet frontend.
  SenderDropped,
}

/// Split from the public entry point purely so tests can drive the timeout path in milliseconds
/// instead of waiting out the real ten seconds.
async fn await_bridge_reply_with_timeout(
  mut pending: PendingSceneRequest<'_>,
  timeout_duration: Duration,
) -> Result<ControlBridgeReply, AwaitBridgeReplyError> {
  let request_id = pending.request_id();

  match timeout(timeout_duration, pending.receiver_mut()).await {
    Ok(Ok(reply)) => Ok(reply),
    Ok(Err(_recv_error)) => Err(AwaitBridgeReplyError::SenderDropped),
    Err(_elapsed) => {
      debug!(
        "[ControlServer] Scene bridge request {} timed out after {:?}; releasing it.",
        request_id,
        timeout_duration,
      );

      Err(AwaitBridgeReplyError::TimedOut)
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::control_server::state::control_bridge_state::ControlBridgeState;
  use serde_json::Value;
  use std::future::Future;
  use uuid::Uuid;

  const TEST_TIMEOUT: Duration = Duration::from_millis(20);

  #[tokio::test]
  async fn test_reply_before_timeout_is_returned_and_map_drains() {
    let bridge_state = ControlBridgeState::new();
    let request_id = Uuid::new_v4();

    let pending = bridge_state.register(request_id);
    assert!(bridge_state.complete(&request_id, success_reply()));

    let reply = await_bridge_reply_with_timeout(pending, TEST_TIMEOUT)
      .await
      .expect("a delivered reply should be returned");

    assert!(reply.success);
    assert_eq!(bridge_state.pending_count(), 0);
  }

  #[tokio::test]
  async fn test_silent_frontend_times_out_and_map_drains() {
    let bridge_state = ControlBridgeState::new();
    let request_id = Uuid::new_v4();

    let pending = bridge_state.register(request_id);

    let result = await_bridge_reply_with_timeout(pending, TEST_TIMEOUT).await;

    assert_eq!(result.err(), Some(AwaitBridgeReplyError::TimedOut));
    assert_eq!(bridge_state.pending_count(), 0);
  }

  #[tokio::test]
  async fn test_reply_arriving_after_a_timeout_is_dropped() {
    let bridge_state = ControlBridgeState::new();
    let request_id = Uuid::new_v4();

    let pending = bridge_state.register(request_id);
    let _ = await_bridge_reply_with_timeout(pending, TEST_TIMEOUT).await;

    // The late reply finds no reservation, is dropped, and leaves the map empty.
    assert!(!bridge_state.complete(&request_id, success_reply()));
    assert_eq!(bridge_state.pending_count(), 0);
  }

  #[tokio::test]
  async fn test_dropped_sender_is_reported_separately_from_a_timeout() {
    let bridge_state = ControlBridgeState::new();
    let request_id = Uuid::new_v4();

    let pending = bridge_state.register(request_id);
    // Dropping the map entry drops the sender without ever delivering a reply.
    bridge_state.cancel(&request_id);

    let result = await_bridge_reply_with_timeout(pending, TEST_TIMEOUT).await;

    assert_eq!(result.err(), Some(AwaitBridgeReplyError::SenderDropped));
    assert_eq!(bridge_state.pending_count(), 0);
  }

  #[tokio::test]
  async fn test_repeated_requests_do_not_accumulate() {
    let bridge_state = ControlBridgeState::new();

    for _ in 0..3 {
      let pending = bridge_state.register(Uuid::new_v4());
      let _ = await_bridge_reply_with_timeout(pending, TEST_TIMEOUT).await;
    }

    assert_eq!(bridge_state.pending_count(), 0);
  }

  /// The client-disconnect path: axum drops the handler future outright, so nothing downstream
  /// of the `.await` ever runs and only the guard's `Drop` can release the reservation.
  #[tokio::test]
  async fn test_abandoned_wait_still_drains_the_map() {
    let bridge_state = ControlBridgeState::new();

    {
      let pending = bridge_state.register(Uuid::new_v4());
      let mut wait_future = Box::pin(await_bridge_reply_with_timeout(pending, TEST_TIMEOUT));

      // Poll once so the future is genuinely mid-await, then throw it away.
      let poll_once = futures::future::poll_fn(|context| {
        let _ = wait_future.as_mut().poll(context);
        std::task::Poll::Ready(())
      });
      poll_once.await;

      assert_eq!(bridge_state.pending_count(), 1);
    }

    assert_eq!(bridge_state.pending_count(), 0);
  }

  fn success_reply() -> ControlBridgeReply {
    ControlBridgeReply {
      success: true,
      maybe_data: Some(Value::Null),
      maybe_error: None,
    }
  }
}
