use log::warn;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;
use tokio::sync::oneshot;
use uuid::Uuid;

/// Correlates in-flight scene requests with the replies the webview sends back through
/// `control_bridge_reply_command`.
///
/// One entry exists per HTTP request that is currently waiting on the frontend. Entries are
/// removed on EVERY exit path — reply, timeout, and failure to emit — so a webview that never
/// answers (or answers twice) cannot grow the map.
///
/// NB: The lock is a std `Mutex` on purpose: no `.await` ever happens while it is held, so the
/// async `tokio::sync::Mutex` would only add overhead and a cancellation hazard.
pub struct ControlBridgeState {
  pending: Mutex<HashMap<Uuid, oneshot::Sender<ControlBridgeReply>>>,
}

impl ControlBridgeState {
  pub fn new() -> Self {
    Self {
      pending: Mutex::new(HashMap::new()),
    }
  }

  /// Reserves `request_id` and hands back the receiver the HTTP side waits on.
  pub fn register(&self, request_id: Uuid) -> oneshot::Receiver<ControlBridgeReply> {
    let (sender, receiver) = oneshot::channel();
    self.lock_pending().insert(request_id, sender);

    receiver
  }

  /// Delivers a reply. `false` means the id was unknown — a late reply for a request that
  /// already timed out, or a frontend bug. Either way it is dropped, never an error.
  pub fn complete(&self, request_id: &Uuid, reply: ControlBridgeReply) -> bool {
    let Some(sender) = self.lock_pending().remove(request_id) else {
      return false;
    };

    // An `Err` here means the HTTP task went away between the timeout check and this send; the
    // request is already answered, so there is nothing left to do but drop the reply.
    sender.send(reply).is_ok()
  }

  /// Drops a reservation without delivering anything (timeout, or the event failed to emit).
  pub fn cancel(&self, request_id: &Uuid) {
    self.lock_pending().remove(request_id);
  }

  /// Only used for logging and tests — proof that the map drains.
  pub fn pending_count(&self) -> usize {
    self.lock_pending().len()
  }

  /// A poisoned lock means some other task panicked mid-mutation. The map is a plain
  /// `HashMap` of senders with no cross-entry invariant, so recovering the inner value keeps
  /// the bridge working instead of poisoning every later scene request.
  fn lock_pending(&self) -> std::sync::MutexGuard<'_, HashMap<Uuid, oneshot::Sender<ControlBridgeReply>>> {
    self.pending.lock().unwrap_or_else(|poisoned| {
      warn!("[ControlServer] Scene bridge pending map lock was poisoned; recovering.");
      poisoned.into_inner()
    })
  }
}

impl Default for ControlBridgeState {
  fn default() -> Self {
    Self::new()
  }
}

/// What the webview sent back for one scene request.
#[derive(Clone, Debug)]
pub struct ControlBridgeReply {
  pub success: bool,
  pub maybe_data: Option<Value>,
  pub maybe_error: Option<ControlBridgeReplyError>,
}

#[derive(Clone, Debug)]
pub struct ControlBridgeReplyError {
  /// A protocol error code string chosen by the frontend (e.g. `SCENE_NOT_ACTIVE`). Optional
  /// because the frontend may only manage a message; the endpoint then falls back to `INTERNAL`.
  pub maybe_code: Option<String>,
  pub message: String,
}

#[cfg(test)]
mod tests {
  use super::*;

  fn success_reply() -> ControlBridgeReply {
    ControlBridgeReply {
      success: true,
      maybe_data: Some(Value::Bool(true)),
      maybe_error: None,
    }
  }

  #[test]
  fn test_register_then_complete_delivers_and_drains() {
    let state = ControlBridgeState::new();
    let request_id = Uuid::new_v4();

    let mut receiver = state.register(request_id);
    assert_eq!(state.pending_count(), 1);

    assert!(state.complete(&request_id, success_reply()));
    assert_eq!(state.pending_count(), 0);

    let reply = receiver.try_recv().expect("reply should have been delivered");
    assert!(reply.success);
  }

  #[test]
  fn test_unknown_request_id_is_dropped() {
    let state = ControlBridgeState::new();

    assert!(!state.complete(&Uuid::new_v4(), success_reply()));
    assert_eq!(state.pending_count(), 0);
  }

  #[test]
  fn test_second_reply_for_one_request_is_dropped() {
    let state = ControlBridgeState::new();
    let request_id = Uuid::new_v4();

    let _receiver = state.register(request_id);

    assert!(state.complete(&request_id, success_reply()));
    assert!(!state.complete(&request_id, success_reply()));
    assert_eq!(state.pending_count(), 0);
  }

  #[test]
  fn test_cancel_drains_the_entry_and_disarms_later_replies() {
    let state = ControlBridgeState::new();
    let request_id = Uuid::new_v4();

    let _receiver = state.register(request_id);
    state.cancel(&request_id);

    assert_eq!(state.pending_count(), 0);
    assert!(!state.complete(&request_id, success_reply()));
  }

  #[test]
  fn test_requests_are_independent() {
    let state = ControlBridgeState::new();
    let first_id = Uuid::new_v4();
    let second_id = Uuid::new_v4();

    let mut first_receiver = state.register(first_id);
    let mut second_receiver = state.register(second_id);
    assert_eq!(state.pending_count(), 2);

    assert!(state.complete(&second_id, success_reply()));

    assert!(second_receiver.try_recv().is_ok());
    assert!(first_receiver.try_recv().is_err());
    assert_eq!(state.pending_count(), 1);
  }
}
