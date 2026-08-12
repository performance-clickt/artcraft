import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { ControlClient, unwrapEnvelope } from "../src/control-client.js";
import { APP_NOT_RUNNING_MESSAGE, ArtcraftControlError } from "../src/errors.js";
import { startControlServerStub, writeDiscoveryFile, writeDiscoveryFileAt } from "./helpers.js";

const HEALTH_PAYLOAD = { app_version: "1.2.3", pid: 4242, logged_in: true };

describe("authentication and request shaping", () => {
  it("sends the discovery file's token as a bearer header", async () => {
    const stub = await startControlServerStub(() => ({ body: { success: true, data: HEALTH_PAYLOAD } }));
    const client = await clientFor(stub.port, { token: "s3cret-token" });

    await client.getHealth();
    await stub.close();

    assert.equal(stub.requests.length, 1);
    assert.equal(stub.requests[0]?.authorization, "Bearer s3cret-token");
    assert.equal(stub.requests[0]?.url, "/v1/health");
  });

  it("re-reads the discovery file per call, so a restarted app is picked up", async () => {
    const first = await startControlServerStub(() => ({ body: { success: true, data: HEALTH_PAYLOAD } }));
    const second = await startControlServerStub(() => ({
      body: { success: true, data: { ...HEALTH_PAYLOAD, app_version: "9.9.9" } },
    }));

    // One file path, rewritten between calls: exactly what an app restart does.
    const filePath = await writeDiscoveryFile({ port: first.port, token: "first-token" });
    const client = new ControlClient({ discovery: { filePath } });

    assert.equal((await client.getHealth()).app_version, "1.2.3");

    await writeDiscoveryFileAt(filePath, { port: second.port, token: "second-token" });

    // Same client instance: nothing about the first connection may be cached.
    assert.equal((await client.getHealth()).app_version, "9.9.9");
    assert.equal(second.requests[0]?.authorization, "Bearer second-token");

    await first.close();
    await second.close();
  });

  it("serializes query parameters and JSON bodies", async () => {
    const stub = await startControlServerStub(() => ({ body: { success: true, data: { models: [] } } }));
    const client = await clientFor(stub.port, {});

    await client.listModels("video");
    await client.estimateCost({ kind: "image", model: "flux_1_schnell" });
    await stub.close();

    assert.equal(stub.requests[0]?.url, "/v1/models?kind=video");
    assert.equal(stub.requests[1]?.method, "POST");
    assert.deepEqual(JSON.parse(stub.requests[1]?.body ?? "{}"), {
      kind: "image",
      model: "flux_1_schnell",
    });
  });

  it("reports a refused connection as the app not running", async () => {
    // A port nothing is listening on is exactly the stale-discovery-file situation.
    const stub = await startControlServerStub(() => ({ body: {} }));
    const deadPort = stub.port;
    await stub.close();

    const client = await clientFor(deadPort, {});

    await assert.rejects(client.getHealth(), {
      code: "APP_NOT_RUNNING",
      message: APP_NOT_RUNNING_MESSAGE,
    });
  });
});

describe("envelope unwrapping", () => {
  it("returns the data field of a success envelope", () => {
    assert.deepEqual(unwrapEnvelope(JSON.stringify({ success: true, data: { a: 1 } }), 200), { a: 1 });
  });

  it("maps each error code to its actionable next step", () => {
    const cases: { code: string; status: number; expected: RegExp }[] = [
      { code: "UNAUTHORIZED", status: 401, expected: /restart ArtCraft/i },
      { code: "NOT_LOGGED_IN", status: 403, expected: /Sign in to ArtCraft/i },
      { code: "SCENE_NOT_ACTIVE", status: 409, expected: /Open the 3D scene tab/i },
      { code: "SCENE_BRIDGE_TIMEOUT", status: 504, expected: /did not answer in time/i },
      { code: "TASK_NOT_FOUND", status: 404, expected: /list_tasks/i },
      { code: "UPSTREAM_API_ERROR", status: 502, expected: /retry once/i },
      { code: "BAD_REQUEST", status: 400, expected: /Fix the arguments/i },
      { code: "INTERNAL", status: 500, expected: /bug in the ArtCraft control server/i },
    ];

    for (const testCase of cases) {
      const body = JSON.stringify({
        success: false,
        error: { code: testCase.code, message: "upstream said no" },
      });

      assert.throws(() => unwrapEnvelope(body, testCase.status), (error: unknown) => {
        assert.ok(error instanceof ArtcraftControlError);
        assert.equal(error.code, testCase.code);
        assert.match(error.message, /upstream said no/);
        assert.match(error.message, testCase.expected);

        return true;
      });
    }
  });

  it("passes an unrecognized error code through without inventing guidance", () => {
    const body = JSON.stringify({ success: false, error: { code: "TELEPORT_FAILED", message: "nope" } });

    assert.throws(() => unwrapEnvelope(body, 500), (error: unknown) => {
      assert.ok(error instanceof ArtcraftControlError);
      assert.equal(error.code, "INTERNAL");
      assert.match(error.message, /TELEPORT_FAILED/);
      assert.match(error.message, /nope/);

      return true;
    });
  });

  it("does not read an unenveloped 404 as a missing task", () => {
    // The control server only emits 404 from a handler that means TASK_NOT_FOUND. A bare one is a
    // missing route, and answering it with "call list_tasks" would send the agent nowhere.
    assert.throws(() => unwrapEnvelope("Not Found", 404), (error: unknown) => {
      assert.ok(error instanceof ArtcraftControlError);
      assert.equal(error.code, "MALFORMED_RESPONSE");
      assert.match(error.message, /does not exist in this build/);
      assert.ok(!/list_tasks/.test(error.message));

      return true;
    });
  });

  it("falls back to the HTTP status when the body is not an envelope", () => {
    // axum-level rejections are emitted as bare text, and a stranger on the port answers anything.
    assert.throws(() => unwrapEnvelope("Unauthorized", 401), { code: "UNAUTHORIZED" });
    assert.throws(() => unwrapEnvelope("<html>hello</html>", 200), (error: unknown) => {
      assert.ok(error instanceof ArtcraftControlError);
      assert.equal(error.code, "MALFORMED_RESPONSE");
      assert.match(error.message, /restart ArtCraft/i);

      return true;
    });
  });

  it("surfaces a control error raised over the wire", async () => {
    const stub = await startControlServerStub(() => ({
      status: 403,
      body: { success: false, error: { code: "NOT_LOGGED_IN", message: "No ArtCraft session." } },
    }));
    const client = await clientFor(stub.port, {});

    await assert.rejects(client.getCredits(), { code: "NOT_LOGGED_IN" });
    await stub.close();
  });

  it("rejects a payload whose shape does not match the endpoint", async () => {
    const stub = await startControlServerStub(() => ({ body: { success: true, data: { nope: 1 } } }));
    const client = await clientFor(stub.port, {});

    await assert.rejects(client.getHealth(), { code: "MALFORMED_RESPONSE" });
    await stub.close();
  });
});

async function clientFor(
  port: number,
  fields: { token?: string; started_at?: string },
): Promise<ControlClient> {
  const filePath = await writeDiscoveryFile({ port, ...fields });

  return new ControlClient({ discovery: { filePath } });
}
