import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  parseDiscoveryFile,
  readControlServerDiscovery,
  resolveDiscoveryFilePath,
} from "../src/discovery.js";
import { APP_NOT_RUNNING_MESSAGE, ArtcraftControlError } from "../src/errors.js";
import { missingDiscoveryFilePath, writeDiscoveryFile, writeRawDiscoveryFile } from "./helpers.js";

const DEAD_PID = 4_194_303;

describe("parseDiscoveryFile", () => {
  it("reads a well-formed file", () => {
    const discovery = parseDiscoveryFile(
      JSON.stringify({
        version: 1,
        pid: 4242,
        port: 51234,
        token: "abc123",
        started_at: "2026-08-12T10:00:00Z",
      }),
    );

    assert.equal(discovery.pid, 4242);
    assert.equal(discovery.port, 51234);
    assert.equal(discovery.token, "abc123");
    assert.equal(discovery.startedAt.toISOString(), "2026-08-12T10:00:00.000Z");
  });

  it("treats a partially written file as the app not running", () => {
    // The app writes this file during launch; a reader can catch it mid-write.
    const truncated = '{"version": 1, "pid": 4242, "po';

    assert.throws(() => parseDiscoveryFile(truncated), (error: unknown) => {
      assert.ok(error instanceof ArtcraftControlError);
      assert.equal(error.code, "APP_NOT_RUNNING");
      assert.equal(error.message, APP_NOT_RUNNING_MESSAGE);

      return true;
    });
  });

  it("treats a file missing required fields as the app not running", () => {
    const missingToken = JSON.stringify({
      version: 1,
      pid: 4242,
      port: 51234,
      started_at: "2026-08-12T10:00:00Z",
    });

    assert.throws(() => parseDiscoveryFile(missingToken), { code: "APP_NOT_RUNNING" });
  });

  it("rejects an out-of-range port and an unparseable timestamp", () => {
    const badPort = JSON.stringify({
      version: 1,
      pid: 1,
      port: 70_000,
      token: "t",
      started_at: "2026-08-12T10:00:00Z",
    });
    const badTimestamp = JSON.stringify({
      version: 1,
      pid: 1,
      port: 51234,
      token: "t",
      started_at: "whenever",
    });

    assert.throws(() => parseDiscoveryFile(badPort), { code: "APP_NOT_RUNNING" });
    assert.throws(() => parseDiscoveryFile(badTimestamp), { code: "APP_NOT_RUNNING" });
  });

  it("reports a newer file format distinctly from a missing app", () => {
    // Relaunching the app cannot fix a format mismatch, so it must not say "launch the app".
    const newer = JSON.stringify({
      version: 2,
      pid: 1,
      port: 51234,
      token: "t",
      started_at: "2026-08-12T10:00:00Z",
    });

    assert.throws(() => parseDiscoveryFile(newer), (error: unknown) => {
      assert.ok(error instanceof ArtcraftControlError);
      assert.equal(error.code, "DISCOVERY_UNSUPPORTED");
      assert.match(error.message, /version 2/);
      assert.match(error.message, /Update artcraft-mcp/);

      return true;
    });
  });
});

describe("readControlServerDiscovery", () => {
  it("reads a live file", async () => {
    const filePath = await writeDiscoveryFile({ port: 51234, token: "live-token" });
    const discovery = await readControlServerDiscovery({ filePath });

    assert.equal(discovery.port, 51234);
    assert.equal(discovery.token, "live-token");
  });

  it("reports a missing file as the app not running", async () => {
    const filePath = await missingDiscoveryFilePath();

    await assert.rejects(readControlServerDiscovery({ filePath }), {
      code: "APP_NOT_RUNNING",
      message: APP_NOT_RUNNING_MESSAGE,
    });
  });

  it("rejects a stale file whose process is gone", async () => {
    // A quit app leaves a plausible-looking file behind; without the pid check the next request
    // would hang, or reach whatever recycled the port.
    const filePath = await writeDiscoveryFile({ pid: DEAD_PID, port: 51234 });

    await assert.rejects(
      readControlServerDiscovery({ filePath, isProcessAlive: () => false }),
      { code: "APP_NOT_RUNNING" },
    );
  });

  it("rejects an empty file", async () => {
    const filePath = await writeRawDiscoveryFile("");

    await assert.rejects(readControlServerDiscovery({ filePath }), { code: "APP_NOT_RUNNING" });
  });
});

describe("resolveDiscoveryFilePath", () => {
  it("prefers the explicit file override, then the data dir, then the home default", () => {
    assert.equal(
      resolveDiscoveryFilePath({ env: { ARTCRAFT_CONTROL_STATE_FILE: "/tmp/x.json", ARTCRAFT_DATA_DIR: "/data" } }),
      "/tmp/x.json",
    );
    assert.equal(
      resolveDiscoveryFilePath({ env: { ARTCRAFT_DATA_DIR: "/data" } }),
      "/data/state/control_server.json",
    );
    assert.match(resolveDiscoveryFilePath({ env: {} }), /Artcraft\/state\/control_server\.json$/);
  });
});
