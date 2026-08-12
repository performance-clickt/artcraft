use serde::{Serialize, Serializer};

/// The scene operations the control server can ask the webview to perform, i.e. the `{op}` path
/// segment of `POST /v1/scene/{op}`.
///
/// NB: These strings are half of the bridge contract with the frontend `<ControlBridge/>`
/// (HM-921) — the frontend switches on them. Add values, never rename existing ones.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneOp {
  /// Is a 3D scene mounted right now, and what does it hold at a glance?
  Status,
  /// Token-light rows for every object in the scene.
  ListObjects,
  /// The full scene JSON.
  GetScene,
  /// Replace the whole scene with the supplied JSON.
  ApplyScene,
  /// Patch one object (transform / rename / visibility) by uuid.
  UpdateObject,
}

impl SceneOp {
  pub fn to_str(&self) -> &'static str {
    match self {
      Self::Status => "status",
      Self::ListObjects => "list_objects",
      Self::GetScene => "get_scene",
      Self::ApplyScene => "apply_scene",
      Self::UpdateObject => "update_object",
    }
  }

  /// `None` for anything unrecognized — the endpoint turns that into `BAD_REQUEST` rather than
  /// forwarding an op the frontend would not understand.
  pub fn from_str(op: &str) -> Option<Self> {
    match op {
      "status" => Some(Self::Status),
      "list_objects" => Some(Self::ListObjects),
      "get_scene" => Some(Self::GetScene),
      "apply_scene" => Some(Self::ApplyScene),
      "update_object" => Some(Self::UpdateObject),
      _ => None,
    }
  }

  pub fn all_variants() -> [Self; 5] {
    [
      Self::Status,
      Self::ListObjects,
      Self::GetScene,
      Self::ApplyScene,
      Self::UpdateObject,
    ]
  }
}

/// Serialized through `to_str` so the wire format cannot drift from the path segment the caller
/// used or from the string the frontend matches on.
impl Serialize for SceneOp {
  fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(self.to_str())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_round_trip() {
    for op in SceneOp::all_variants() {
      assert_eq!(SceneOp::from_str(op.to_str()), Some(op));
    }
  }

  #[test]
  fn test_unknown_ops_are_rejected() {
    assert_eq!(SceneOp::from_str("unknown_op"), None);
    assert_eq!(SceneOp::from_str(""), None);
    assert_eq!(SceneOp::from_str("Status"), None);
  }

  #[test]
  fn test_serializes_as_the_path_segment() {
    let serialized = serde_json::to_string(&SceneOp::ListObjects).unwrap();

    assert_eq!(serialized, r#""list_objects""#);
  }
}
