import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { BasicEventWrapper } from "../../common/BasicEventWrapper";
import { useEffect } from "react";

// Hand-mirrored from the Rust enum variant
// `TauriEventName::ControlSceneRequestEvent` in
// `crates/schema/public/enums/src/tauri/ux/tauri_event_name.rs`. The emitter is
// `crates/desktop/artcraft/src/core/events/control_scene_request_event.rs`; it
// wraps the payload in the usual `{status, data}` envelope.
const EVENT_NAME: string = "control_scene_request_event";

// The op strings are the other half of the bridge contract, mirrored from
// `SceneOp` in
// `crates/desktop/artcraft/src/core/control_server/scene_bridge/scene_op.rs`.
// Values are added there, never renamed.
export type ControlSceneOp =
  | "status"
  | "list_objects"
  | "get_scene"
  | "apply_scene"
  | "update_object";

export interface ControlSceneRequestEvent {
  // Correlation id; must be echoed back through `control_bridge_reply_command`.
  request_id: string;
  op: ControlSceneOp;
  // Op-specific arguments, verbatim from the HTTP request body. `null` when the
  // caller sent no body (e.g. `status`).
  payload: unknown;
}

export const useControlSceneRequestEvent = (
  asyncCallback: (event: ControlSceneRequestEvent) => Promise<void>
) => {
  useEffect(() => {
    let isUnmounted = false;
    let unlisten: Promise<UnlistenFn>;

    const setup = async () => {
      unlisten = listen<BasicEventWrapper<ControlSceneRequestEvent>>(
        EVENT_NAME,
        async (wrappedEvent) => {
          await asyncCallback(wrappedEvent.payload.data);
        }
      );

      if (isUnmounted) {
        unlisten.then((f) => f()); // Unsubscribe if unmounted early.
      }
    };

    setup();

    return () => {
      isUnmounted = true;
      unlisten.then((f) => f());
    };
  }, []);
};
