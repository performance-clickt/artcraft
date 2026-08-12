/**
 * HTTP client for the ArtCraft app's loopback control server.
 *
 * Every call re-reads the discovery file (see `discovery.ts`), attaches the launch's bearer token,
 * and unwraps the control envelope `{success, data | error:{code, message}}` so callers deal in
 * payloads and `ArtcraftControlError`, never in status codes.
 */

import { z } from "zod";
import {
  ArtcraftControlError,
  asControlErrorCode,
  type ControlErrorCode,
} from "./errors.js";
import {
  readControlServerDiscovery,
  type ControlServerDiscovery,
  type DiscoveryOptions,
} from "./discovery.js";

/** Default per-request deadline. Generous because `/v1/models` retries its upstream call 3x. */
export const DEFAULT_REQUEST_TIMEOUT_MS = 20_000;

/**
 * How long after app start a `logged_in: false` is treated as indeterminate.
 *
 * The app's credential manager is initialized empty and only filled by the main-window cookie-sync
 * task, so a signed-in user reads as signed-out for the first moments of a launch (HM-922 design
 * note from the round-2 merge gate). Reporting that verbatim would tell an agent to ask a
 * already-signed-in user to sign in.
 */
export const LOGIN_INDETERMINATE_WINDOW_MS = 20_000;

/** Delay before the single login re-check inside that window. */
export const LOGIN_RETRY_DELAY_MS = 2_000;

/**
 * Statuses whose meaning survives without an envelope, used only when a response is not a control
 * envelope at all (an axum-level rejection, a proxy).
 *
 * NB: Deliberately excludes 404, 409 and 504. Those statuses are endpoint-specific — the control
 * server only ever emits them enveloped, from a handler that knows what they mean — so an
 * unenveloped one is far more likely to be a missing route than a missing task or an unmounted
 * scene. Mapping a bare 404 to `TASK_NOT_FOUND` would answer a version-skew problem with "call
 * list_tasks", sending the agent somewhere useless.
 */
const HTTP_STATUS_TO_ERROR_CODE: Record<number, ControlErrorCode> = {
  400: "BAD_REQUEST",
  401: "UNAUTHORIZED",
  403: "NOT_LOGGED_IN",
  502: "UPSTREAM_API_ERROR",
};

export const healthSchema = z
  .object({
    app_version: z.string(),
    pid: z.number(),
    logged_in: z.boolean(),
  })
  .passthrough();

export type HealthResponse = z.infer<typeof healthSchema>;

export const creditsSchema = z
  .object({
    free_credits: z.number(),
    monthly_credits: z.number(),
    banked_credits: z.number(),
    sum_total_credits: z.number(),
  })
  .passthrough();

export type CreditsResponse = z.infer<typeof creditsSchema>;

/** A health reading plus what it took to trust the `logged_in` bit. */
export interface HealthProbe {
  health: HealthResponse;
  /** True when the first reading said signed-out inside the startup window and was re-checked. */
  loginRecheckPerformed: boolean;
}

export interface ControlClientOptions {
  discovery?: DiscoveryOptions;
  fetchImpl?: typeof fetch;
  timeoutMs?: number;
  /** Clock injection point, so the startup window is testable without waiting on wall time. */
  now?: () => number;
  sleep?: (milliseconds: number) => Promise<void>;
}

export interface RequestOptions {
  method?: "GET" | "POST";
  /** JSON-encoded as the request body. */
  body?: unknown;
  query?: Record<string, string>;
}

export class ControlClient {
  private readonly options: ControlClientOptions;

  constructor(options: ControlClientOptions = {}) {
    this.options = options;
  }

  /** `GET /v1/health` — app version, pid, and whether an ArtCraft session is present. */
  async getHealth(): Promise<HealthResponse> {
    const discovery = await this.readDiscovery();

    return this.requestParsed(discovery, healthSchema, "/v1/health", {});
  }

  /**
   * Health, with the startup false-negative handled: a `logged_in: false` read within
   * `LOGIN_INDETERMINATE_WINDOW_MS` of app start is re-checked once after a short delay before any
   * caller is allowed to report "not signed in".
   *
   * Only one retry, and only inside the window: a genuinely signed-out app must still answer
   * promptly rather than stalling every call behind a pointless wait.
   */
  async getHealthWithLoginRetry(): Promise<HealthProbe> {
    const discovery = await this.readDiscovery();
    const health = await this.requestParsed(discovery, healthSchema, "/v1/health", {});

    if (health.logged_in || !this.isWithinStartupWindow(discovery)) {
      return { health, loginRecheckPerformed: false };
    }

    const sleep = this.options.sleep ?? defaultSleep;
    await sleep(LOGIN_RETRY_DELAY_MS);

    // Re-read discovery: if the app restarted during the delay, the port and token both changed.
    const recheckDiscovery = await this.readDiscovery();
    const recheck = await this.requestParsed(recheckDiscovery, healthSchema, "/v1/health", {});

    return { health: recheck, loginRecheckPerformed: true };
  }

  /** `GET /v1/credits` — the balance to check before committing to a generation. */
  async getCredits(): Promise<CreditsResponse> {
    const discovery = await this.readDiscovery();

    return this.requestParsed(discovery, creditsSchema, "/v1/credits", {});
  }

  /**
   * `GET /v1/models?kind=` — returned unparsed. The catalog's shape is owned by the backend and
   * grows new fields regularly; the formatters read what they recognize and pass the rest through
   * rather than this client rejecting a payload it merely does not know about yet.
   */
  async listModels(kind: "image" | "video"): Promise<unknown> {
    const discovery = await this.readDiscovery();

    return this.request(discovery, "/v1/models", { query: { kind } });
  }

  /** `POST /v1/estimate_cost` — body is the kind-tagged upstream estimate request. */
  async estimateCost(body: Record<string, unknown>): Promise<unknown> {
    const discovery = await this.readDiscovery();

    return this.request(discovery, "/v1/estimate_cost", { method: "POST", body });
  }

  /** Issues a request and validates the unwrapped payload against `schema`. */
  private async requestParsed<T>(
    discovery: ControlServerDiscovery,
    schema: z.ZodType<T>,
    path: string,
    options: RequestOptions,
  ): Promise<T> {
    const payload = await this.request(discovery, path, options);
    const result = schema.safeParse(payload);

    if (!result.success) {
      throw new ArtcraftControlError(
        "MALFORMED_RESPONSE",
        `ArtCraft answered ${path} with an unexpected payload shape. The app and this MCP server ` +
          `are probably built from different revisions — rebuild both from the same checkout.`,
      );
    }

    return result.data;
  }

  /** Issues one authenticated request and returns the unwrapped `data`. */
  private async request(
    discovery: ControlServerDiscovery,
    path: string,
    options: RequestOptions,
  ): Promise<unknown> {
    const fetchImpl = this.options.fetchImpl ?? fetch;
    const timeoutMs = this.options.timeoutMs ?? DEFAULT_REQUEST_TIMEOUT_MS;
    const url = new URL(`http://127.0.0.1:${discovery.port}${path}`);

    for (const [key, value] of Object.entries(options.query ?? {})) {
      url.searchParams.set(key, value);
    }

    const headers: Record<string, string> = {
      // The token is per launch and only ever travels over loopback.
      authorization: `Bearer ${discovery.token}`,
      accept: "application/json",
    };

    if (options.body !== undefined) {
      headers["content-type"] = "application/json";
    }

    let response: Response;

    try {
      response = await fetchImpl(url, {
        method: options.method ?? "GET",
        headers,
        ...(options.body === undefined ? {} : { body: JSON.stringify(options.body) }),
        signal: AbortSignal.timeout(timeoutMs),
      });
    } catch (error) {
      throw describeTransportFailure(error, timeoutMs);
    }

    let text: string;

    try {
      text = await response.text();
    } catch (error) {
      // The app quit mid-response: the connection is gone the same way a refused connect is.
      throw describeTransportFailure(error, timeoutMs);
    }

    return unwrapEnvelope(text, response.status);
  }

  private async readDiscovery(): Promise<ControlServerDiscovery> {
    return readControlServerDiscovery(this.options.discovery ?? {});
  }

  private isWithinStartupWindow(discovery: ControlServerDiscovery): boolean {
    const now = this.options.now ?? Date.now;
    const elapsed = now() - discovery.startedAt.getTime();

    // A negative elapsed time means the app's clock is ahead of ours; the app has, by its own
    // account, only just started, so treat it as inside the window rather than outside it.
    return elapsed < LOGIN_INDETERMINATE_WINDOW_MS;
  }
}

const successEnvelopeSchema = z.object({
  success: z.literal(true),
  data: z.unknown(),
});

const errorEnvelopeSchema = z.object({
  success: z.literal(false),
  error: z.object({
    code: z.string(),
    message: z.string(),
  }),
});

/**
 * Turns a raw response body into either the payload or a thrown `ArtcraftControlError`.
 *
 * The status code is only a fallback: the control server puts the authoritative code in the
 * envelope, and an envelope is expected even on failures.
 */
export function unwrapEnvelope(body: string, status: number): unknown {
  let parsed: unknown;

  try {
    parsed = JSON.parse(body);
  } catch {
    // Not JSON at all. The control server's own handlers and its auth layer always answer with an
    // envelope, but the layers around them do not: an unknown route or a body-size rejection comes
    // back as axum's bare text. A meaningful status is still worth honouring there.
    throw statusFallbackError(status, body);
  }

  const failure = errorEnvelopeSchema.safeParse(parsed);

  if (failure.success) {
    const code = asControlErrorCode(failure.data.error.code);

    if (code === undefined) {
      // A code this build does not know about: pass the app's own message through rather than
      // inventing guidance for a condition we cannot characterize.
      throw new ArtcraftControlError(
        "INTERNAL",
        `ArtCraft returned an unrecognized error code "${failure.data.error.code}": ` +
          `${failure.data.error.message}`,
      );
    }

    throw ArtcraftControlError.fromEnvelope(code, failure.data.error.message);
  }

  const success = successEnvelopeSchema.safeParse(parsed);

  if (success.success) {
    return success.data.data;
  }

  // Valid JSON, but not an envelope.
  throw statusFallbackError(status, body);
}

/**
 * Classifies a response that is not a control envelope: by status when the status carries a known
 * meaning, otherwise as a shape mismatch.
 */
function statusFallbackError(status: number, body: string): ArtcraftControlError {
  const statusCode = HTTP_STATUS_TO_ERROR_CODE[status];

  if (statusCode !== undefined) {
    return ArtcraftControlError.fromEnvelope(statusCode, truncateForMessage(body));
  }

  // 404 gets its own sentence: an unenveloped "not found" means the route is absent from this
  // build of the app, which is a version mismatch, not anything the agent did wrong.
  const cause =
    status === 404
      ? "That endpoint does not exist in this build of ArtCraft. The app and this MCP server are " +
        "built from different revisions — rebuild both from the same checkout."
      : "Something other than the ArtCraft control server may be listening on that port — " +
        "restart ArtCraft, then retry.";

  return new ArtcraftControlError(
    "MALFORMED_RESPONSE",
    `ArtCraft answered with HTTP ${status} and a body that is not a control envelope: ` +
      `${truncateForMessage(body)}. ${cause}`,
  );
}

/**
 * Classifies a `fetch` rejection. A refused or reset connection means the discovery file outlived
 * the app that wrote it, which is the same situation as no file at all.
 */
function describeTransportFailure(error: unknown, timeoutMs: number): ArtcraftControlError {
  if (error instanceof Error && (error.name === "TimeoutError" || error.name === "AbortError")) {
    return new ArtcraftControlError(
      "REQUEST_TIMEOUT",
      `ArtCraft did not answer within ${Math.round(timeoutMs / 1000)}s. The app may be busy or ` +
        `stuck — check the ArtCraft window, then retry.`,
    );
  }

  return ArtcraftControlError.appNotRunning();
}

function truncateForMessage(body: string): string {
  const collapsed = body.replace(/\s+/g, " ").trim();

  return collapsed.length > 200 ? `${collapsed.slice(0, 200)}…` : collapsed;
}

function defaultSleep(milliseconds: number): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, milliseconds);
  });
}
