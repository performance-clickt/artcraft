/**
 * The startup `logged_in` false-negative window (HM-922 design note from the round-2 merge gate):
 * the app's credential manager starts empty and is filled by the main-window cookie-sync task, so a
 * signed-in user reads as signed-out for the first moments after launch. A client that reports that
 * verbatim tells an already-signed-in user to sign in.
 */

import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { ControlClient, LOGIN_INDETERMINATE_WINDOW_MS, LOGIN_RETRY_DELAY_MS } from "../src/control-client.js";
import { buildStatusReport } from "../src/tools/status.js";
import { startControlServerStub, writeDiscoveryFile, type StubResponse } from "./helpers.js";

const CREDITS = { free_credits: 10, monthly_credits: 1_000, banked_credits: 224, sum_total_credits: 1_234 };

const APP_STARTED_AT = "2026-08-12T10:00:00Z";
const APP_START_MS = Date.parse(APP_STARTED_AT);

describe("login re-check inside the startup window", () => {
  it("re-checks once and reports the second, truthful reading", async () => {
    const slept: number[] = [];
    const stub = await startControlServerStub((_request, callIndex) =>
      healthResponse({ logged_in: callIndex > 0 }),
    );
    const client = await clientFor(stub.port, {
      // Two seconds after launch: inside the indeterminate window.
      now: () => APP_START_MS + 2_000,
      sleep: async (ms) => {
        slept.push(ms);
      },
    });

    const probe = await client.getHealthWithLoginRetry();
    await stub.close();

    assert.equal(probe.health.logged_in, true);
    assert.equal(probe.loginRecheckPerformed, true);
    assert.deepEqual(slept, [LOGIN_RETRY_DELAY_MS]);
    assert.equal(stub.requests.length, 2, "health should be polled exactly twice");
  });

  it("re-checks at most once, then reports signed out", async () => {
    const stub = await startControlServerStub(() => healthResponse({ logged_in: false }));
    const client = await clientFor(stub.port, {
      now: () => APP_START_MS + 1_000,
      sleep: async () => undefined,
    });

    const probe = await client.getHealthWithLoginRetry();
    await stub.close();

    assert.equal(probe.health.logged_in, false);
    assert.equal(probe.loginRecheckPerformed, true);
    assert.equal(stub.requests.length, 2, "a genuinely signed-out app must not be polled forever");
  });

  it("does not re-check once the window has passed", async () => {
    const slept: number[] = [];
    const stub = await startControlServerStub(() => healthResponse({ logged_in: false }));
    const client = await clientFor(stub.port, {
      now: () => APP_START_MS + LOGIN_INDETERMINATE_WINDOW_MS + 1,
      sleep: async (ms) => {
        slept.push(ms);
      },
    });

    const probe = await client.getHealthWithLoginRetry();
    await stub.close();

    assert.equal(probe.loginRecheckPerformed, false);
    assert.deepEqual(slept, [], "a long-running signed-out app must answer without delay");
    assert.equal(stub.requests.length, 1);
  });

  it("does not re-check when the first reading is already signed in", async () => {
    const stub = await startControlServerStub(() => healthResponse({ logged_in: true }));
    const client = await clientFor(stub.port, {
      now: () => APP_START_MS + 1_000,
      sleep: async () => undefined,
    });

    const probe = await client.getHealthWithLoginRetry();
    await stub.close();

    assert.equal(probe.loginRecheckPerformed, false);
    assert.equal(stub.requests.length, 1);
  });
});

describe("get_status report", () => {
  it("reports version, sign-in state, and the credit breakdown", async () => {
    const stub = await startControlServerStub((request) =>
      request.url === "/v1/credits" ? { body: { success: true, data: CREDITS } } : healthResponse({ logged_in: true }),
    );
    const client = await clientFor(stub.port, { now: () => APP_START_MS + 60_000, sleep: async () => undefined });

    const report = await buildStatusReport(client);
    await stub.close();

    assert.match(report, /ArtCraft 1\.2\.3 is running \(pid 4242\)\./);
    assert.match(report, /Signed in: yes\./);
    assert.match(report, /1234 credits total \(free 10, monthly 1000, banked 224\)/);
  });

  it("says the signed-out reading survived the re-check, and skips credits", async () => {
    const stub = await startControlServerStub((request) =>
      request.url === "/v1/credits"
        ? { status: 403, body: { success: false, error: { code: "NOT_LOGGED_IN", message: "no session" } } }
        : healthResponse({ logged_in: false }),
    );
    const client = await clientFor(stub.port, { now: () => APP_START_MS + 1_000, sleep: async () => undefined });

    const report = await buildStatusReport(client);
    await stub.close();

    assert.match(report, /Signed in: no\./);
    assert.match(report, /Re-checked after the app-start delay/);
    assert.match(report, /Sign in to ArtCraft in the app window/);
    assert.ok(
      !stub.requests.some((request) => request.url === "/v1/credits"),
      "credits must not be requested while signed out",
    );
  });

  it("reports a credits failure without claiming a balance", async () => {
    const stub = await startControlServerStub((request) =>
      request.url === "/v1/credits"
        ? { status: 502, body: { success: false, error: { code: "UPSTREAM_API_ERROR", message: "backend down" } } }
        : healthResponse({ logged_in: true }),
    );
    const client = await clientFor(stub.port, { now: () => APP_START_MS + 60_000, sleep: async () => undefined });

    const report = await buildStatusReport(client);
    await stub.close();

    assert.match(report, /Signed in: yes\./);
    assert.match(report, /Credits: unavailable\. backend down/);
  });

  it("contradicting endpoints resolve to signed out, not to a balance", async () => {
    // Health can say yes while the session is lost a moment later; the credits endpoint is the
    // authority because it is the one that actually needed the session.
    const stub = await startControlServerStub((request) =>
      request.url === "/v1/credits"
        ? { status: 403, body: { success: false, error: { code: "NOT_LOGGED_IN", message: "no session" } } }
        : healthResponse({ logged_in: true }),
    );
    const client = await clientFor(stub.port, { now: () => APP_START_MS + 60_000, sleep: async () => undefined });

    const report = await buildStatusReport(client);
    await stub.close();

    assert.match(report, /Signed in: no \(the credits endpoint reports no ArtCraft session\)\./);
    assert.ok(!/credits total/.test(report));
  });
});

function healthResponse(fields: { logged_in: boolean }): StubResponse {
  return { body: { success: true, data: { app_version: "1.2.3", pid: 4242, logged_in: fields.logged_in } } };
}

async function clientFor(
  port: number,
  options: { now: () => number; sleep: (ms: number) => Promise<void> },
): Promise<ControlClient> {
  const filePath = await writeDiscoveryFile({ port, started_at: APP_STARTED_AT });

  return new ControlClient({ discovery: { filePath }, now: options.now, sleep: options.sleep });
}
