/**
 * Error vocabulary shared by the control client and the tools.
 *
 * Every message here is written for an agent that must decide what to do next, not for a log
 * reader: it names the condition and the one action that clears it. Stack traces never reach the
 * model — tool handlers render `ControlClientError.message` and nothing else.
 */

/** Error codes the control server itself returns inside its failure envelope. */
export const CONTROL_ERROR_CODES = [
  "UNAUTHORIZED",
  "BAD_REQUEST",
  "NOT_LOGGED_IN",
  "SCENE_NOT_ACTIVE",
  "SCENE_BRIDGE_TIMEOUT",
  "TASK_NOT_FOUND",
  "UPSTREAM_API_ERROR",
  "INTERNAL",
] as const;

export type ControlErrorCode = (typeof CONTROL_ERROR_CODES)[number];

/**
 * Codes raised by this client rather than by the control server. They are kept distinct from the
 * protocol codes so a reader can tell "the app never answered" from "the app answered with an
 * error".
 */
export type ClientErrorCode =
  /** No usable discovery file, a dead pid in it, or nothing listening on the recorded port. */
  | "APP_NOT_RUNNING"
  /** The discovery file exists but this build cannot read its format. */
  | "DISCOVERY_UNSUPPORTED"
  /** The control server answered, but not with a control envelope. */
  | "MALFORMED_RESPONSE"
  /** The request outlived its deadline. */
  | "REQUEST_TIMEOUT";

export type ArtcraftErrorCode = ControlErrorCode | ClientErrorCode;

/**
 * The one message every "cannot reach the app" path collapses to. A stale discovery file, a dead
 * pid and a refused connection are the same situation from the agent's side — the app it wants to
 * drive is not there — and the same single action fixes all three.
 */
export const APP_NOT_RUNNING_MESSAGE =
  "ArtCraft is not running (or was restarted). Launch the patched ArtCraft app, then retry.";

/** Guidance appended to a control-server error so the agent knows the next move, not just the code. */
const NEXT_STEP_BY_CODE: Record<ControlErrorCode, string> = {
  UNAUTHORIZED:
    "The control server rejected the token in ~/Artcraft/state/control_server.json. " +
    "That file is rewritten on every launch, so restart ArtCraft, then retry.",
  BAD_REQUEST: "Fix the arguments described above and call the tool again.",
  NOT_LOGGED_IN: "Sign in to ArtCraft in the app window, then retry.",
  SCENE_NOT_ACTIVE: "Open the 3D scene tab in ArtCraft first, then retry.",
  SCENE_BRIDGE_TIMEOUT:
    "The ArtCraft window did not answer in time. Make sure the 3D scene tab is open and " +
    "responsive, then retry.",
  TASK_NOT_FOUND:
    "That task id is not in this app session's queue. Call list_tasks to get current task ids.",
  UPSTREAM_API_ERROR:
    "ArtCraft reached its backend and the backend failed. This is usually transient — retry once, " +
    "and if it persists check the ArtCraft window for a sign-in or billing prompt.",
  INTERNAL: "This is a bug in the ArtCraft control server. Check the ArtCraft app log for details.",
};

/**
 * The only error type tools are expected to surface. `message` is already agent-facing: tools
 * render it verbatim rather than wrapping it in more prose.
 */
export class ArtcraftControlError extends Error {
  public readonly code: ArtcraftErrorCode;

  constructor(code: ArtcraftErrorCode, message: string) {
    super(message);
    this.name = "ArtcraftControlError";
    this.code = code;
  }

  /** The app is unreachable — every cause maps to the same message and the same fix. */
  static appNotRunning(): ArtcraftControlError {
    return new ArtcraftControlError("APP_NOT_RUNNING", APP_NOT_RUNNING_MESSAGE);
  }

  /** A failure envelope from the control server, with its next step appended. */
  static fromEnvelope(code: ControlErrorCode, message: string): ArtcraftControlError {
    const detail = message.trim().length > 0 ? message.trim() : code;

    return new ArtcraftControlError(code, `${detail} ${NEXT_STEP_BY_CODE[code]}`);
  }
}

/** Narrows an arbitrary string to a known control error code. */
export function asControlErrorCode(value: string): ControlErrorCode | undefined {
  return (CONTROL_ERROR_CODES as readonly string[]).includes(value)
    ? (value as ControlErrorCode)
    : undefined;
}

/**
 * Renders any thrown value as the text a tool returns. Non-`ArtcraftControlError` throws are a bug
 * in this server, so they are labelled as such instead of leaking a bare runtime message that an
 * agent would misread as an ArtCraft-side problem.
 */
export function toToolErrorText(error: unknown): string {
  if (error instanceof ArtcraftControlError) {
    return error.message;
  }

  const detail = error instanceof Error ? error.message : String(error);

  return `Unexpected artcraft-mcp failure: ${detail}. This is a bug in the MCP server, not in ArtCraft.`;
}
