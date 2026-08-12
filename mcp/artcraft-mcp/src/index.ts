#!/usr/bin/env node
/**
 * artcraft-mcp — an MCP server that drives a running ArtCraft desktop app through the loopback
 * control server the app exposes.
 *
 * Transport is stdio, so stdout belongs to the protocol: nothing here may print to it. Diagnostics
 * go to stderr.
 */

import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { pathToFileURL } from "node:url";
import { ControlClient } from "./control-client.js";
import { registerModelTools } from "./tools/models.js";
import { registerStatusTool } from "./tools/status.js";

const SERVER_NAME = "artcraft";
const SERVER_VERSION = "0.1.0";

const SERVER_INSTRUCTIONS = [
  "Tools for driving a running ArtCraft desktop app: inspect its status, list the generation models",
  "it offers, and price a generation before running it.",
  "",
  "Every tool needs the patched ArtCraft app to be running — start any session with get_status. If a",
  "tool reports that ArtCraft is not running, launching the app is the fix; retrying without it will",
  "fail the same way. Generation and scene tools are not part of this build yet.",
].join("\n");

/**
 * Builds the server with every tool registered. Exported so tests can drive it over an in-memory
 * transport instead of spawning a process.
 *
 * One control client for the whole server: it holds no connection state — the discovery file is
 * re-read per call — so an app restart needs no coordination here.
 */
export function createArtcraftMcpServer(client: ControlClient = new ControlClient()): McpServer {
  const server = new McpServer(
    { name: SERVER_NAME, version: SERVER_VERSION },
    { instructions: SERVER_INSTRUCTIONS },
  );

  registerStatusTool(server, client);
  registerModelTools(server, client);

  return server;
}

async function main(): Promise<void> {
  await createArtcraftMcpServer().connect(new StdioServerTransport());
}

// Only when run as the entry point: importing this module (tests, embedders) must not seize stdio.
if (process.argv[1] !== undefined && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error: unknown) => {
    process.stderr.write(
      `artcraft-mcp failed to start: ${error instanceof Error ? (error.stack ?? error.message) : String(error)}\n`,
    );
    process.exit(1);
  });
}
