/**
 * Discovery of the running app's control server.
 *
 * The ArtCraft app writes `~/Artcraft/state/control_server.json` (0600) on every launch with the
 * port and bearer token for that launch. This module turns that file into a validated descriptor,
 * and treats every "the file does not describe a live server" case — absent, truncated, malformed,
 * or naming a dead process — as the same actionable "app is not running" error.
 *
 * The file is read per call, never cached: the app can restart between two tool calls, and a cached
 * port/token would then point at nothing (or, worse, at whatever else took the port).
 */

import { readFile } from "node:fs/promises";
import { homedir } from "node:os";
import { join } from "node:path";
import { z } from "zod";
import { ArtcraftControlError } from "./errors.js";

/** The discovery-file format this build understands. */
export const SUPPORTED_DISCOVERY_VERSION = 1;

const STATE_FILE_NAME = "control_server.json";
const DEFAULT_DATA_DIR_NAME = "Artcraft";

/** Full-path override, mainly for tests and for a non-default app data directory. */
const FILE_PATH_ENV_VAR = "ARTCRAFT_CONTROL_STATE_FILE";
/** Data-root override, mirroring the app's own `~/Artcraft` default. */
const DATA_DIR_ENV_VAR = "ARTCRAFT_DATA_DIR";

/** A validated description of this launch's control server. */
export interface ControlServerDiscovery {
  version: number;
  pid: number;
  port: number;
  token: string;
  /** App start time, from the file. Drives the login-indeterminate window in the control client. */
  startedAt: Date;
}

export interface DiscoveryOptions {
  /** Overrides path resolution entirely. */
  filePath?: string;
  /** Environment used for path resolution. Defaults to `process.env`. */
  env?: NodeJS.ProcessEnv;
  /** Liveness probe for the recorded pid. Defaults to a signal-0 probe. */
  isProcessAlive?: (pid: number) => boolean;
}

/**
 * NB: `started_at` is validated as a parseable timestamp rather than trusted, because it gates the
 * login retry window — an unparseable value must not silently become epoch 0 (which would disable
 * the retry) or `NaN` (which would enable it forever).
 */
const discoveryFileSchema = z.object({
  version: z.number().int().positive(),
  pid: z.number().int().positive(),
  port: z.number().int().min(1).max(65535),
  token: z.string().min(1),
  started_at: z.string().refine((value) => !Number.isNaN(Date.parse(value)), {
    message: "started_at must be an RFC 3339 timestamp",
  }),
});

/** Reads, validates, and liveness-checks the discovery file. */
export async function readControlServerDiscovery(
  options: DiscoveryOptions = {},
): Promise<ControlServerDiscovery> {
  const path = resolveDiscoveryFilePath(options);

  let raw: string;

  try {
    raw = await readFile(path, "utf8");
  } catch {
    // Absent (app never launched), unreadable, or owned by another user: from the agent's side
    // these are all "no app to drive".
    throw ArtcraftControlError.appNotRunning();
  }

  const discovery = parseDiscoveryFile(raw);
  const isAlive = options.isProcessAlive ?? isProcessAlive;

  // A file left behind by a crashed or quit app still holds a plausible port and token. Without
  // this check the next call would hang or, if the port has been recycled, talk to a stranger.
  if (!isAlive(discovery.pid)) {
    throw ArtcraftControlError.appNotRunning();
  }

  return discovery;
}

/**
 * Parses discovery-file contents. Exported for tests and for callers that already hold the bytes.
 *
 * A version this build does not understand is reported distinctly: relaunching the app cannot fix a
 * format mismatch, so it must not produce the "launch the app" message.
 */
export function parseDiscoveryFile(raw: string): ControlServerDiscovery {
  let parsedJson: unknown;

  try {
    parsedJson = JSON.parse(raw);
  } catch {
    // Most often a partially written file caught mid-launch — retrying after the app finishes
    // starting is exactly the right move.
    throw ArtcraftControlError.appNotRunning();
  }

  const result = discoveryFileSchema.safeParse(parsedJson);

  if (!result.success) {
    throw ArtcraftControlError.appNotRunning();
  }

  const file = result.data;

  if (file.version !== SUPPORTED_DISCOVERY_VERSION) {
    throw new ArtcraftControlError(
      "DISCOVERY_UNSUPPORTED",
      `The ArtCraft control server wrote discovery file version ${file.version}, but this MCP ` +
        `server understands version ${SUPPORTED_DISCOVERY_VERSION}. Update artcraft-mcp (and rebuild ` +
        `it) to match the app.`,
    );
  }

  return {
    version: file.version,
    pid: file.pid,
    port: file.port,
    token: file.token,
    startedAt: new Date(file.started_at),
  };
}

/** Where the discovery file is expected to live, honouring both overrides. */
export function resolveDiscoveryFilePath(options: DiscoveryOptions = {}): string {
  if (options.filePath !== undefined) {
    return options.filePath;
  }

  const env = options.env ?? process.env;
  const explicitPath = env[FILE_PATH_ENV_VAR];

  if (explicitPath !== undefined && explicitPath.length > 0) {
    return explicitPath;
  }

  const dataDir = env[DATA_DIR_ENV_VAR];
  const root = dataDir !== undefined && dataDir.length > 0 ? dataDir : join(homedir(), DEFAULT_DATA_DIR_NAME);

  return join(root, "state", STATE_FILE_NAME);
}

/**
 * Signal 0 performs the permission and existence checks without delivering a signal. `EPERM` means
 * the process exists but belongs to another user, which still counts as alive.
 */
function isProcessAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);

    return true;
  } catch (error) {
    return (error as NodeJS.ErrnoException).code === "EPERM";
  }
}
