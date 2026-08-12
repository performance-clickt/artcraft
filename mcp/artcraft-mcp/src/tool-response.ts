/**
 * The single place tool results are constructed, so every tool answers in the same shape and every
 * failure reaches the model as an actionable sentence rather than a stack trace.
 */

import { toToolErrorText } from "./errors.js";

export interface ToolResponse {
  content: { type: "text"; text: string }[];
  isError?: boolean;
  [key: string]: unknown;
}

export function textResponse(text: string): ToolResponse {
  return { content: [{ type: "text", text }] };
}

export function errorResponse(text: string): ToolResponse {
  return { content: [{ type: "text", text }], isError: true };
}

/**
 * Runs a tool body, converting any throw into an error response. Tools never let an exception
 * escape: an MCP protocol-level error is opaque to the agent, whereas an `isError` result carries
 * the sentence that tells it what to do next.
 */
export async function runTool(body: () => Promise<string>): Promise<ToolResponse> {
  try {
    return textResponse(await body());
  } catch (error) {
    return errorResponse(toToolErrorText(error));
  }
}
