/**
 * `get_status` — the first call an agent should make: is the app there, is a user signed in, and
 * is there enough balance to be worth planning a generation.
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import type { ControlClient } from "../control-client.js";
import { ArtcraftControlError } from "../errors.js";
import { capText, formatCredits } from "../format.js";
import { runTool, type ToolResponse } from "../tool-response.js";

const DESCRIPTION = [
  "Check that the ArtCraft desktop app is running and reachable, whether a user is signed in, and",
  "the current credit balance.",
  "",
  "Call this first when a session starts, or whenever another ArtCraft tool reports that the app is",
  "not running. Requires the patched ArtCraft app to be running; nothing here spends credits or",
  "changes app state.",
  "",
  "Returns: app version and pid, sign-in state, and the credit balance broken into free, monthly,",
  "and banked. When no user is signed in the balance is unavailable — sign in inside the ArtCraft",
  "window and call again.",
].join("\n");

export function registerStatusTool(server: McpServer, client: ControlClient): void {
  server.registerTool(
    "get_status",
    {
      title: "Get ArtCraft status",
      description: DESCRIPTION,
      inputSchema: z.object({}).strict(),
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: true,
      },
    },
    async (): Promise<ToolResponse> => runTool(async () => buildStatusReport(client)),
  );
}

/** Exported for tests: the full status text, including the startup login re-check behaviour. */
export async function buildStatusReport(client: ControlClient): Promise<string> {
  const probe = await client.getHealthWithLoginRetry();
  const { health } = probe;
  const lines = [`ArtCraft ${health.app_version} is running (pid ${health.pid}).`];

  if (!health.logged_in) {
    lines.push(
      "Signed in: no." +
        (probe.loginRecheckPerformed
          ? " (Re-checked after the app-start delay, so this is not a startup false negative.)"
          : ""),
      "Credits: unavailable while signed out. Sign in to ArtCraft in the app window, then call get_status again.",
    );

    return capText(lines.join("\n"));
  }

  lines.push("Signed in: yes.");

  try {
    lines.push(`Credits: ${formatCredits(await client.getCredits())}.`);
  } catch (error) {
    // A NOT_LOGGED_IN here contradicts the health reading — the session was lost between the two
    // calls, or the cookie sync had not finished. Report the contradiction rather than a balance.
    if (error instanceof ArtcraftControlError && error.code === "NOT_LOGGED_IN") {
      lines[lines.length - 1] = "Signed in: no (the credits endpoint reports no ArtCraft session).";
      lines.push(
        "Credits: unavailable while signed out. Sign in to ArtCraft in the app window, then call get_status again.",
      );

      return capText(lines.join("\n"));
    }

    if (error instanceof ArtcraftControlError) {
      lines.push(`Credits: unavailable. ${error.message}`);

      return capText(lines.join("\n"));
    }

    throw error;
  }

  return capText(lines.join("\n"));
}
