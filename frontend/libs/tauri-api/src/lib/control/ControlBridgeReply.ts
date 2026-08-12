import { invoke } from "@tauri-apps/api/core";
import { CommandResult } from "../common/CommandStatus";

// Error codes the frontend is allowed to raise. Anything else is reported to the
// HTTP caller as `INTERNAL` by
// `crates/desktop/artcraft/src/core/control_server/endpoints/scene.rs`.
export type ControlBridgeErrorCode = "SCENE_NOT_ACTIVE" | "BAD_REQUEST";

export interface ControlBridgeReplyError {
  code?: ControlBridgeErrorCode;
  message: string;
}

export interface ControlBridgeReplyRequest {
  // Echoed from the `control_scene_request_event` this answers.
  request_id: string;
  success: boolean;
  data?: unknown;
  error?: ControlBridgeReplyError;
}

// Answers one scene request. The Rust command never fails from the frontend's
// point of view: an unknown `request_id` (already timed out, or a double reply)
// is dropped silently. See
// `crates/desktop/artcraft/src/core/commands/control/control_bridge_reply_command.rs`.
export const ControlBridgeReply = async (
  request: ControlBridgeReplyRequest
): Promise<CommandResult> => {
  const result = await invoke("control_bridge_reply_command", {
    request: request,
  });

  return result as CommandResult;
};
