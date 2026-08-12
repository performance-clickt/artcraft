use crate::core::control_server::state::control_bridge_state::{ControlBridgeReply, ControlBridgeState};
use log::debug;
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::time::timeout;
use uuid::Uuid;

/// How long an HTTP caller waits for the webview to answer. Long enough for a scene to be
/// serialized on a busy render thread, short enough that a closed window does not hang a client.
pub const SCENE_BRIDGE_TIMEOUT_SECONDS: u64 = 10;

/// Waits for the webview's reply to `request_id`, releasing the correlation-map entry on every
/// exit path so a silent frontend cannot leak reservations.
pub async fn await_bridge_reply(
  bridge_state: &ControlBridgeState,
  request_id: &Uuid,
  receiver: oneshot::Receiver<ControlBridgeReply>,
) -> Result<ControlBridgeReply, AwaitBridgeReplyError> {
  let timeout_duration = Duration::from_secs(SCENE_BRIDGE_TIMEOUT_SECONDS);

  await_bridge_reply_with_timeout(bridge_state, request_id, receiver, timeout_duration).await
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
  bridge_state: &ControlBridgeState,
  request_id: &Uuid,
  receiver: oneshot::Receiver<ControlBridgeReply>,
  timeout_duration: Duration,
) -> Result<ControlBridgeReply, AwaitBridgeReplyError> {
  match timeout(timeout_duration, receiver).await {
    Ok(Ok(reply)) => {
      // The reply path already removed the entry; this keeps the invariant true even if a
      // future caller delivers a reply some other way.
      bridge_state.cancel(request_id);
      Ok(reply)
    }
    Ok(Err(_recv_error)) => {
      bridge_state.cancel(request_id);
      Err(AwaitBridgeReplyError::SenderDropped)
    }
    Err(_elapsed) => {
      debug!(
        "[ControlServer] Scene bridge request {} timed out after {:?}; releasing it.",
        request_id,
        timeout_duration,
      );
      bridge_state.cancel(request_id);
      Err(AwaitBridgeReplyError::TimedOut)
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::Value;

  const TEST_TIMEOUT: Duration = Duration::from_millis(20);

  #[tokio::test]
  async fn test_reply_before_timeout_is_returned_and_map_drains() {
    let bridge_state = ControlBridgeState::new();
    let request_id = Uuid::new_v4();

    let receiver = bridge_state.register(request_id);
    assert!(bridge_state.complete(&request_id, success_reply()));

    let reply = await_bridge_reply_with_timeout(&bridge_state, &request_id, receiver, TEST_TIMEOUT)
      .await
      .expect("a delivered reply should be returned");

    assert!(reply.success);
    assert_eq!(bridge_state.pending_count(), 0);
  }

  #[tokio::test]
  async fn test_silent_frontend_times_out_and_map_drains() {
    let bridge_state = ControlBridgeState::new();
    let request_id = Uuid::new_v4();

    let receiver = bridge_state.register(request_id);

    let result = await_bridge_reply_with_timeout(&bridge_state, &request_id, receiver, TEST_TIMEOUT).await;

    assert_eq!(result.err(), Some(AwaitBridgeReplyError::TimedOut));
    assert_eq!(bridge_state.pending_count(), 0);
  }

  #[tokio::test]
  async fn test_reply_arriving_after_a_timeout_is_dropped() {
    let bridge_state = ControlBridgeState::new();
    let request_id = Uuid::new_v4();

    let receiver = bridge_state.register(request_id);
    let _ = await_bridge_reply_with_timeout(&bridge_state, &request_id, receiver, TEST_TIMEOUT).await;

    // The late reply finds no reservation, is dropped, and leaves the map empty.
    assert!(!bridge_state.complete(&request_id, success_reply()));
    assert_eq!(bridge_state.pending_count(), 0);
  }

  #[tokio::test]
  async fn test_dropped_sender_is_reported_separately_from_a_timeout() {
    let bridge_state = ControlBridgeState::new();
    let request_id = Uuid::new_v4();

    let receiver = bridge_state.register(request_id);
    // Dropping the reservation drops the sender without ever delivering a reply.
    bridge_state.cancel(&request_id);

    let result = await_bridge_reply_with_timeout(&bridge_state, &request_id, receiver, TEST_TIMEOUT).await;

    assert_eq!(result.err(), Some(AwaitBridgeReplyError::SenderDropped));
    assert_eq!(bridge_state.pending_count(), 0);
  }

  #[tokio::test]
  async fn test_repeated_requests_do_not_accumulate() {
    let bridge_state = ControlBridgeState::new();

    for _ in 0..3 {
      let request_id = Uuid::new_v4();
      let receiver = bridge_state.register(request_id);
      let _ = await_bridge_reply_with_timeout(&bridge_state, &request_id, receiver, TEST_TIMEOUT).await;
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
