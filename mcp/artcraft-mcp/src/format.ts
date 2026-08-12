/**
 * Response formatting for the tools: the character cap every tool response passes through, and the
 * concise/detailed renderings of the model catalog.
 *
 * The catalog is parsed permissively on purpose. It is the backend's shape, it gains fields
 * regularly, and a model whose payload this build only half-recognizes is still worth listing —
 * so unknown fields are ignored rather than rejected, and a payload that is not recognizable at all
 * degrades to pretty JSON instead of an error.
 */

import { z } from "zod";

/** Cap on any single tool response, per the MCP response-size guidance. */
export const CHARACTER_LIMIT = 25_000;

/**
 * Truncates to `limit`, replacing the tail with a notice.
 *
 * The notice states the next action, because an agent that cannot see the rest of a list needs to
 * know a narrower call exists — a bare "[truncated]" invites a blind retry of the same call.
 */
export function capText(text: string, limit: number = CHARACTER_LIMIT): string {
  if (text.length <= limit) {
    return text;
  }

  const notice =
    `\n\n[Truncated: showing the first {shown} of ${text.length} characters. ` +
    `Re-run with response_format="concise", or narrow the request, to see the rest.]`;
  const budget = Math.max(0, limit - notice.length - 8);
  const shown = text.slice(0, budget);

  return `${shown}${notice.replace("{shown}", String(shown.length))}`;
}

/** Renders a credit balance as a single agent-readable line. */
export function formatCredits(credits: {
  free_credits: number;
  monthly_credits: number;
  banked_credits: number;
  sum_total_credits: number;
}): string {
  return (
    `${credits.sum_total_credits} credits total ` +
    `(free ${credits.free_credits}, monthly ${credits.monthly_credits}, banked ${credits.banked_credits})`
  );
}

/**
 * Renders a cost estimate. Credits lead because credits are what the balance in `get_status` is
 * denominated in; the USD figure is a secondary sanity check for a human reading over the agent's
 * shoulder.
 */
export function formatCostEstimate(payload: unknown, model: string, provider: string): string {
  const result = costEstimateSchema.safeParse(payload);

  if (!result.success) {
    return capText(
      "ArtCraft returned a cost estimate in a shape this MCP server does not recognize. " +
        "Raw payload follows.\n\n" +
        JSON.stringify(payload, null, 2),
    );
  }

  const estimate = result.data;
  const parts: string[] = [];

  if (estimate.is_unlimited === true) {
    parts.push("unlimited plan: this generation does not draw down credits");
  } else if (estimate.is_free === true) {
    parts.push("free for this account");
  }

  if (estimate.cost_in_credits !== undefined) {
    parts.unshift(`${estimate.cost_in_credits} credits`);
  }

  if (estimate.cost_in_usd_cents !== undefined) {
    parts.push(`about $${(estimate.cost_in_usd_cents / 100).toFixed(2)}`);
  }

  if (estimate.is_rate_limited === true) {
    parts.push("RATE LIMITED: this generation would be refused right now");
  }

  if (estimate.has_watermark === true) {
    parts.push("output will be watermarked");
  }

  const summary = parts.length > 0 ? parts.join(" · ") : "no cost information returned";

  return capText(`${model} via ${provider}: ${summary}`);
}

const costEstimateSchema = z
  .object({
    cost_in_credits: z.number().optional(),
    cost_in_usd_cents: z.number().optional(),
    is_free: z.boolean().optional(),
    is_unlimited: z.boolean().optional(),
    is_rate_limited: z.boolean().optional(),
    has_watermark: z.boolean().optional(),
  })
  .passthrough();

/**
 * One model row, concise: identifier first (that is what other tools take), then the human name,
 * the creator, which providers can route it, and the two facts that decide a generation call —
 * default aspect ratio and batch range.
 */
export function formatModelsConcise(payload: unknown, kind: ModelKind): string {
  const catalog = parseCatalog(payload);

  if (catalog === undefined) {
    return formatUnrecognizedCatalog(payload);
  }

  const providersByModel = indexProvidersByModel(catalog);
  const lines = catalog.models.map((model) => {
    const parts: string[] = [model.model];

    if (model.full_name !== undefined && model.full_name !== model.model) {
      parts.push(model.full_name);
    }

    if (model.model_creator !== undefined) {
      parts.push(model.model_creator);
    }

    const providers = providersByModel.get(model.model);

    if (providers !== undefined && providers.length > 0) {
      parts.push(`via ${providers.join(", ")}`);
    }

    if (model.aspect_ratio_default !== undefined) {
      parts.push(`default ratio ${model.aspect_ratio_default}`);
    }

    const batch = formatBatchRange(model);

    if (batch !== undefined) {
      parts.push(batch);
    }

    if (model.is_disabled === true) {
      parts.push("DISABLED");
    }

    return `- ${parts.join(" · ")}`;
  });

  const header =
    `${catalog.models.length} ${kind} model(s) available. ` +
    `Pass the first field of a row as \`model\` to estimate_cost. ` +
    `Use response_format="detailed" for aspect ratio, resolution, and quality options.`;

  return capText([header, "", ...lines].join("\n"));
}

/** The same catalog with every option list an agent might need to pick valid generation params. */
export function formatModelsDetailed(payload: unknown, kind: ModelKind): string {
  const catalog = parseCatalog(payload);

  if (catalog === undefined) {
    return formatUnrecognizedCatalog(payload);
  }

  const providersByModel = indexProvidersByModel(catalog);
  const blocks = catalog.models.map((model) => {
    const lines: string[] = [`## ${model.model}${model.is_disabled === true ? " (DISABLED)" : ""}`];

    appendField(lines, "name", model.full_name);
    appendField(lines, "creator", model.model_creator);

    const providers = providersByModel.get(model.model);

    if (providers !== undefined && providers.length > 0) {
      lines.push(`providers: ${providers.join(", ")}`);
    }

    appendField(lines, "text prompt supported", model.text_prompt_supported);
    appendField(lines, "text prompt max length", model.text_prompt_max_length);
    appendField(lines, "image refs supported", model.image_refs_supported);
    appendField(lines, "image refs max", model.image_refs_max);
    appendList(lines, "aspect ratios", model.aspect_ratio_options);
    appendField(lines, "aspect ratio default", model.aspect_ratio_default);
    appendList(lines, "resolutions", model.resolution_options);
    appendField(lines, "resolution default", model.resolution_default);
    appendList(lines, "qualities", model.quality_options);
    appendField(lines, "quality default", model.default_quality);
    appendField(lines, "duration seconds default", model.duration_seconds_default);
    appendList(lines, "duration seconds options", model.duration_seconds_options);

    const batch = formatBatchRange(model);

    if (batch !== undefined) {
      lines.push(batch);
    }

    return lines.join("\n");
  });

  const header = `${catalog.models.length} ${kind} model(s) available.`;

  return capText([header, "", ...blocks].join("\n\n"));
}

export type ModelKind = "image" | "video";

/**
 * Deliberately loose: only `model` is required, since that is the field other tools consume. Every
 * other field is optional because the catalog omits what a given model does not support, and
 * unknown fields pass through untouched.
 */
const modelDetailsSchema = z
  .object({
    model: z.string(),
    model_creator: z.string().optional(),
    full_name: z.string().optional(),
    text_prompt_supported: z.boolean().optional(),
    text_prompt_max_length: z.number().optional(),
    image_refs_supported: z.boolean().optional(),
    image_refs_max: z.number().optional(),
    aspect_ratio_options: z.array(z.string()).optional(),
    aspect_ratio_default: z.string().optional(),
    resolution_options: z.array(z.string()).optional(),
    resolution_default: z.string().optional(),
    quality_options: z.array(z.string()).optional(),
    default_quality: z.string().optional(),
    duration_seconds_options: z.array(z.number()).optional(),
    duration_seconds_default: z.number().optional(),
    batch_size_min: z.number().optional(),
    batch_size_max: z.number().optional(),
    batch_size_default: z.number().optional(),
    is_disabled: z.boolean().optional(),
  })
  .passthrough();

type ModelDetails = z.infer<typeof modelDetailsSchema>;

const catalogSchema = z
  .object({
    models: z.array(modelDetailsSchema),
    providers: z
      .array(
        z
          .object({
            provider: z.string(),
            models: z.array(z.object({ model: z.string() }).passthrough()),
          })
          .passthrough(),
      )
      .optional(),
  })
  .passthrough();

type Catalog = z.infer<typeof catalogSchema>;

function parseCatalog(payload: unknown): Catalog | undefined {
  const result = catalogSchema.safeParse(payload);

  return result.success ? result.data : undefined;
}

/** Which providers can route each model, inverted from the catalog's provider-major listing. */
function indexProvidersByModel(catalog: Catalog): Map<string, string[]> {
  const index = new Map<string, string[]>();

  for (const provider of catalog.providers ?? []) {
    for (const entry of provider.models) {
      const existing = index.get(entry.model);

      if (existing === undefined) {
        index.set(entry.model, [provider.provider]);
      } else if (!existing.includes(provider.provider)) {
        existing.push(provider.provider);
      }
    }
  }

  return index;
}

function formatBatchRange(model: ModelDetails): string | undefined {
  const { batch_size_min: min, batch_size_max: max } = model;

  if (min === undefined && max === undefined) {
    return undefined;
  }

  return `batch ${min ?? 1}-${max ?? min ?? 1}`;
}

/**
 * The catalog did not match even the loose schema. Returning the payload as JSON keeps the tool
 * useful (an agent can still read it) and makes the mismatch obvious to whoever is debugging.
 */
function formatUnrecognizedCatalog(payload: unknown): string {
  return capText(
    "ArtCraft returned a model catalog in a shape this MCP server does not recognize. " +
      "Raw payload follows.\n\n" +
      JSON.stringify(payload, null, 2),
  );
}

function appendField(lines: string[], label: string, value: string | number | boolean | undefined): void {
  if (value !== undefined) {
    lines.push(`${label}: ${String(value)}`);
  }
}

function appendList(
  lines: string[],
  label: string,
  values: readonly (string | number)[] | undefined,
): void {
  if (values !== undefined && values.length > 0) {
    lines.push(`${label}: ${values.join(", ")}`);
  }
}
