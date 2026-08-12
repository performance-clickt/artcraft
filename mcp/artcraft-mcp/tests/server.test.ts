/**
 * Protocol-level checks: what a client actually sees when it connects. Driven over the SDK's
 * in-memory transport, so the assertions cover real `tools/list` and `tools/call` round trips
 * without spawning a process or needing the inspector.
 */

import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { InMemoryTransport } from "@modelcontextprotocol/sdk/inMemory.js";
import { ControlClient } from "../src/control-client.js";
import { APP_NOT_RUNNING_MESSAGE } from "../src/errors.js";
import { createArtcraftMcpServer } from "../src/index.js";
import { missingDiscoveryFilePath, startControlServerStub, writeDiscoveryFile } from "./helpers.js";

const EXPECTED_TOOLS = ["estimate_cost", "get_status", "list_models"];

describe("tool surface", () => {
  it("exposes exactly the three tools of this milestone, all marked read-only", async () => {
    const client = await connect(new ControlClient());
    const { tools } = await client.listTools();

    assert.deepEqual(tools.map((tool) => tool.name).sort(), EXPECTED_TOOLS);

    for (const tool of tools) {
      assert.equal(tool.annotations?.readOnlyHint, true, `${tool.name} must be read-only`);
      assert.equal(tool.annotations?.destructiveHint, false);
      assert.ok((tool.description ?? "").length > 0, `${tool.name} must document itself`);
    }
  });

  it("advertises the response_format choice on the list tool", async () => {
    const client = await connect(new ControlClient());
    const { tools } = await client.listTools();
    const listModels = tools.find((tool) => tool.name === "list_models");

    assert.ok(listModels !== undefined);
    assert.deepEqual(
      (listModels.inputSchema.properties as { response_format?: { enum?: string[] } }).response_format?.enum,
      ["concise", "detailed"],
    );
  });

  it("rejects unknown arguments rather than ignoring them", async () => {
    // The strict schemas exist so a misspelled argument fails loudly instead of silently changing
    // nothing about the call.
    const client = await connect(new ControlClient());
    const result = await client.callTool({
      name: "list_models",
      arguments: { kind: "image", limitt: 5 },
    });

    assert.equal(result.isError, true);
    assert.match(textOf(result), /Unrecognized key\(s\) in object: 'limitt'/);
  });
});

describe("with ArtCraft closed", () => {
  it("answers every tool with the actionable message and no stack trace", async () => {
    const filePath = await missingDiscoveryFilePath();
    const client = await connect(new ControlClient({ discovery: { filePath } }));

    const calls = [
      { name: "get_status", arguments: {} },
      { name: "list_models", arguments: { kind: "image" } },
      { name: "estimate_cost", arguments: { kind: "image", model: "flux_1_schnell" } },
    ];

    for (const call of calls) {
      const result = await client.callTool(call);
      const text = textOf(result);

      assert.equal(result.isError, true, `${call.name} should report an error result`);
      assert.equal(text, APP_NOT_RUNNING_MESSAGE, `${call.name} should say how to fix it`);
      assert.ok(!text.includes("at "), "no stack frames may reach the model");
    }
  });
});

describe("with ArtCraft running", () => {
  it("renders status and the model catalog through the protocol", async () => {
    const stub = await startControlServerStub((request) => {
      if (request.url === "/v1/credits") {
        return {
          body: {
            success: true,
            data: { free_credits: 0, monthly_credits: 500, banked_credits: 0, sum_total_credits: 500 },
          },
        };
      }

      if (request.url.startsWith("/v1/models")) {
        return {
          body: {
            success: true,
            data: { models: [{ model: "flux_1_schnell", full_name: "FLUX.1 [schnell]" }], providers: [] },
          },
        };
      }

      return { body: { success: true, data: { app_version: "1.2.3", pid: 4242, logged_in: true } } };
    });
    const filePath = await writeDiscoveryFile({ port: stub.port });
    const client = await connect(new ControlClient({ discovery: { filePath } }));

    const status = await client.callTool({ name: "get_status", arguments: {} });
    const models = await client.callTool({
      name: "list_models",
      arguments: { kind: "image", response_format: "concise" },
    });

    await stub.close();

    assert.notEqual(status.isError, true);
    assert.match(textOf(status), /Signed in: yes\./);
    assert.match(textOf(status), /500 credits total/);
    assert.match(textOf(models), /- flux_1_schnell · FLUX\.1 \[schnell\]/);
  });
});

async function connect(controlClient: ControlClient): Promise<Client> {
  const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
  const client = new Client({ name: "test-client", version: "0.0.0" });

  await Promise.all([
    createArtcraftMcpServer(controlClient).connect(serverTransport),
    client.connect(clientTransport),
  ]);

  return client;
}

function textOf(result: unknown): string {
  const content = (result as { content?: { type: string; text?: string }[] }).content ?? [];

  return content
    .filter((entry) => entry.type === "text")
    .map((entry) => entry.text ?? "")
    .join("\n");
}
