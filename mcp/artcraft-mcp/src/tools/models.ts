/**
 * `list_models` and `estimate_cost` — the two calls that precede any generation: pick a model, then
 * price it before spending credits.
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import type { ControlClient } from "../control-client.js";
import { ArtcraftControlError } from "../errors.js";
import { formatCostEstimate, formatModelsConcise, formatModelsDetailed } from "../format.js";
import { runTool, type ToolResponse } from "../tool-response.js";

/** Fields the estimate body owns itself; `params` may not restate them. */
const RESERVED_PARAM_KEYS = ["kind", "model", "provider"] as const;

/**
 * The upstream estimate request requires a tagged `generation_mode`, and omitting it fails the
 * whole call. Text-to-X is the overwhelmingly common case and the one an agent pricing a model
 * means by default, so it is filled in when absent — an explicit `generation_mode` in `params`
 * always wins.
 */
const DEFAULT_GENERATION_MODE: Record<"image" | "video", { type: string }> = {
  image: { type: "text_to_image" },
  video: { type: "text_to_video" },
};

const LIST_MODELS_DESCRIPTION = [
  "List the image or video models this ArtCraft install can generate with, including which",
  "providers route each model and the parameter options each model accepts.",
  "",
  "Use this before estimate_cost or any generation call, to get an exact model identifier — model",
  'names are snake_case identifiers such as "flux_1_schnell", not display names.',
  "",
  'response_format="concise" (default) gives one line per model: identifier, display name, creator,',
  'providers, default aspect ratio, and batch range. response_format="detailed" adds the full',
  "aspect ratio, resolution, quality, and duration option lists — use it when you need to pass",
  "those parameters and must know the valid values.",
  "",
  "Requires the patched ArtCraft app to be running. Read-only; spends no credits. Models marked",
  "DISABLED are listed but cannot be generated with.",
].join("\n");

const ESTIMATE_COST_DESCRIPTION = [
  "Estimate what a generation would cost in credits, before running it.",
  "",
  "Always call this before a generation whose cost you have not already checked: it is the only way",
  "to know whether the account's balance (see get_status) covers the request.",
  "",
  'Pass the model identifier exactly as list_models reports it. kind selects the request family:',
  '"image", "video", or "splat" (3D objects and worlds). Extra generation parameters go in params,',
  "which is passed through to ArtCraft verbatim — for example",
  '{"aspect_ratio": "16_9", "image_batch_count": 4} for an image, or {"duration_seconds": 5} for a',
  'video. When params omits generation_mode, text-to-image / text-to-video is assumed.',
  "",
  "Requires the patched ArtCraft app to be running and a signed-in user. Read-only; estimating",
  "spends no credits. An unknown model or an invalid parameter comes back as a BAD_REQUEST or",
  "upstream error naming the offending field — fix it and call again.",
].join("\n");

export function registerModelTools(server: McpServer, client: ControlClient): void {
  server.registerTool(
    "list_models",
    {
      title: "List ArtCraft generation models",
      description: LIST_MODELS_DESCRIPTION,
      inputSchema: z
        .object({
          kind: z.enum(["image", "video"]).describe("Which catalog to list."),
          response_format: z
            .enum(["concise", "detailed"])
            .default("concise")
            .describe("concise: one line per model. detailed: adds every parameter option list."),
        })
        .strict(),
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: true,
      },
    },
    async ({ kind, response_format }): Promise<ToolResponse> =>
      runTool(async () => {
        const payload = await client.listModels(kind);

        return response_format === "detailed"
          ? formatModelsDetailed(payload, kind)
          : formatModelsConcise(payload, kind);
      }),
  );

  server.registerTool(
    "estimate_cost",
    {
      title: "Estimate generation cost",
      description: ESTIMATE_COST_DESCRIPTION,
      inputSchema: z
        .object({
          kind: z
            .enum(["image", "video", "splat"])
            .describe("Which generation family to price. splat covers 3D objects and worlds."),
          model: z
            .string()
            .min(1)
            .describe('Model identifier from list_models, e.g. "flux_1_schnell".'),
          provider: z
            .string()
            .min(1)
            .default("artcraft")
            .describe("Provider to route through, from the model's providers in list_models."),
          params: z
            .record(z.unknown())
            .optional()
            .describe(
              "Generation parameters passed through to ArtCraft, e.g. " +
                '{"aspect_ratio": "16_9", "image_batch_count": 2}.',
            ),
        })
        .strict(),
      annotations: {
        readOnlyHint: true,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: true,
      },
    },
    async ({ kind, model, provider, params }): Promise<ToolResponse> =>
      runTool(async () => {
        const body = buildEstimateBody(kind, model, provider, params);
        const payload = await client.estimateCost(body);

        return formatCostEstimate(payload, model, provider);
      }),
  );
}

/**
 * Assembles the `{kind, ...upstream request}` body the control server expects.
 *
 * Exported for tests. A `params` key that restates `kind`, `model`, or `provider` is rejected
 * rather than silently overridden: the two values would disagree about what is being priced, and an
 * estimate for the wrong model is worse than an error.
 */
export function buildEstimateBody(
  kind: "image" | "video" | "splat",
  model: string,
  provider: string,
  params: Record<string, unknown> | undefined,
): Record<string, unknown> {
  const extra = params ?? {};
  const conflicts = RESERVED_PARAM_KEYS.filter((key) => key in extra);

  if (conflicts.length > 0) {
    throw new ArtcraftControlError(
      "BAD_REQUEST",
      `params may not contain ${conflicts.join(", ")} — pass ${conflicts.length > 1 ? "those" : "that"} ` +
        `as top-level argument${conflicts.length > 1 ? "s" : ""} instead. ` +
        `Remove ${conflicts.length > 1 ? "them" : "it"} from params and call again.`,
    );
  }

  const body: Record<string, unknown> = { ...extra, kind, model, provider };

  if (kind !== "splat" && body["generation_mode"] === undefined) {
    body["generation_mode"] = DEFAULT_GENERATION_MODE[kind];
  }

  return body;
}
