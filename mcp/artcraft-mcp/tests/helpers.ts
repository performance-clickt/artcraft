/**
 * Test doubles: a real loopback HTTP server standing in for the app's control server, and a real
 * discovery file pointing at it.
 *
 * A real server and a real file rather than a stubbed `fetch`, because the things most likely to
 * break — the bearer header actually being sent, a refused connection being classified correctly,
 * the file being re-read after a restart — only exist at those boundaries.
 */

import { mkdtemp, writeFile } from "node:fs/promises";
import { createServer, type IncomingMessage, type Server, type ServerResponse } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { AddressInfo } from "node:net";

export interface RecordedRequest {
  method: string;
  url: string;
  authorization: string | undefined;
  body: string;
}

export interface StubResponse {
  status?: number;
  /** Sent verbatim; objects are JSON-encoded. */
  body: unknown;
}

export type StubHandler = (request: RecordedRequest, callIndex: number) => StubResponse;

export interface ControlServerStub {
  port: number;
  requests: RecordedRequest[];
  close: () => Promise<void>;
}

/** Starts a loopback server that answers every request via `handler`. */
export async function startControlServerStub(handler: StubHandler): Promise<ControlServerStub> {
  const requests: RecordedRequest[] = [];
  const server: Server = createServer((request: IncomingMessage, response: ServerResponse) => {
    const chunks: Buffer[] = [];

    request.on("data", (chunk: Buffer) => chunks.push(chunk));
    request.on("end", () => {
      const recorded: RecordedRequest = {
        method: request.method ?? "GET",
        url: request.url ?? "/",
        authorization: request.headers.authorization,
        body: Buffer.concat(chunks).toString("utf8"),
      };

      requests.push(recorded);

      const stub = handler(recorded, requests.length - 1);
      const body = typeof stub.body === "string" ? stub.body : JSON.stringify(stub.body);

      response.writeHead(stub.status ?? 200, { "content-type": "application/json" });
      response.end(body);
    });
  });

  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));

  const address = server.address() as AddressInfo;

  return {
    port: address.port,
    requests,
    close: () =>
      new Promise<void>((resolve, reject) => {
        server.close((error) => (error ? reject(error) : resolve()));
      }),
  };
}

export interface DiscoveryFileFields {
  version?: number;
  pid?: number;
  port?: number;
  token?: string;
  started_at?: string;
}

/** Writes a discovery file in a fresh temp directory and returns its path. */
export async function writeDiscoveryFile(fields: DiscoveryFileFields): Promise<string> {
  const directory = await mkdtemp(join(tmpdir(), "artcraft-mcp-test-"));
  const path = join(directory, "control_server.json");

  await writeDiscoveryFileAt(path, fields);

  return path;
}

/** Rewrites an existing discovery-file path in place, as an app relaunch does. */
export async function writeDiscoveryFileAt(path: string, fields: DiscoveryFileFields): Promise<void> {
  const contents = {
    version: fields.version ?? 1,
    // The current process is a pid guaranteed to be alive, which is what the staleness check wants.
    pid: fields.pid ?? process.pid,
    port: fields.port ?? 1,
    token: fields.token ?? "test-token",
    started_at: fields.started_at ?? new Date().toISOString(),
  };

  await writeFile(path, JSON.stringify(contents), "utf8");
}

/** Writes arbitrary bytes to a discovery-file path (malformed, truncated, empty…). */
export async function writeRawDiscoveryFile(raw: string): Promise<string> {
  const directory = await mkdtemp(join(tmpdir(), "artcraft-mcp-test-"));
  const path = join(directory, "control_server.json");

  await writeFile(path, raw, "utf8");

  return path;
}

/** A path inside a temp directory where no file exists. */
export async function missingDiscoveryFilePath(): Promise<string> {
  const directory = await mkdtemp(join(tmpdir(), "artcraft-mcp-test-"));

  return join(directory, "control_server.json");
}
