import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  CHARACTER_LIMIT,
  capText,
  formatCostEstimate,
  formatCredits,
  formatModelsConcise,
  formatModelsDetailed,
} from "../src/format.js";
import { buildEstimateBody } from "../src/tools/models.js";

const CATALOG = {
  success: true,
  models: [
    {
      model: "flux_1_schnell",
      full_name: "FLUX.1 [schnell]",
      model_creator: "black_forest_labs",
      aspect_ratio_options: ["1_1", "16_9", "9_16"],
      aspect_ratio_default: "1_1",
      resolution_options: ["1k", "2k"],
      batch_size_min: 1,
      batch_size_max: 4,
      text_prompt_max_length: 2_048,
    },
    { model: "retired_model", is_disabled: true },
  ],
  providers: [
    { provider: "artcraft", models: [{ model: "flux_1_schnell" }] },
    { provider: "fal", models: [{ model: "flux_1_schnell" }, { model: "retired_model" }] },
  ],
};

describe("capText", () => {
  it("leaves text under the limit untouched", () => {
    assert.equal(capText("short", CHARACTER_LIMIT), "short");
  });

  it("truncates to the limit and names the next action", () => {
    const capped = capText("x".repeat(1_000), 200);

    assert.ok(capped.length <= 200, `expected <= 200 characters, got ${capped.length}`);
    assert.match(capped, /Truncated/);
    assert.match(capped, /response_format="concise"/);
    assert.match(capped, /of 1000 characters/);
  });
});

describe("formatModelsConcise", () => {
  it("leads with the identifier and stays compact", () => {
    const output = formatModelsConcise(CATALOG, "image");
    const modelLine = output.split("\n").find((line) => line.startsWith("- flux_1_schnell"));

    assert.ok(modelLine !== undefined);
    assert.match(modelLine, /^- flux_1_schnell · FLUX\.1 \[schnell\] · black_forest_labs/);
    assert.match(modelLine, /via artcraft, fal/);
    assert.match(modelLine, /default ratio 1_1/);
    assert.match(modelLine, /batch 1-4/);
    // ~40 tokens per model is the budget; 4 chars/token is the usual English ratio.
    assert.ok(modelLine.length < 160, `concise row too long: ${modelLine.length} characters`);
    // Option lists belong to the detailed format only.
    assert.ok(!modelLine.includes("16_9"));
  });

  it("marks disabled models and counts the catalog", () => {
    const output = formatModelsConcise(CATALOG, "image");

    assert.match(output, /2 image model\(s\) available/);
    assert.match(output, /- retired_model · via fal · DISABLED/);
  });

  it("falls back to raw JSON when the catalog shape is unrecognized", () => {
    const output = formatModelsConcise({ unexpected: true }, "image");

    assert.match(output, /does not recognize/);
    assert.match(output, /"unexpected": true/);
  });
});

describe("formatModelsDetailed", () => {
  it("includes the option lists an agent needs to pass parameters", () => {
    const output = formatModelsDetailed(CATALOG, "image");

    assert.match(output, /## flux_1_schnell/);
    assert.match(output, /aspect ratios: 1_1, 16_9, 9_16/);
    assert.match(output, /resolutions: 1k, 2k/);
    assert.match(output, /providers: artcraft, fal/);
    assert.match(output, /## retired_model \(DISABLED\)/);
  });
});

describe("formatCostEstimate", () => {
  it("leads with credits and adds the dollar figure", () => {
    const output = formatCostEstimate(
      { success: true, cost_in_credits: 120, cost_in_usd_cents: 36, is_free: false, has_watermark: false },
      "flux_1_schnell",
      "artcraft",
    );

    assert.equal(output, "flux_1_schnell via artcraft: 120 credits · about $0.36");
  });

  it("calls out the conditions that change what a generation would actually do", () => {
    const rateLimited = formatCostEstimate({ cost_in_credits: 10, is_rate_limited: true }, "m", "p");
    const watermarked = formatCostEstimate({ cost_in_credits: 10, has_watermark: true }, "m", "p");
    const unlimited = formatCostEstimate({ is_unlimited: true }, "m", "p");

    assert.match(rateLimited, /RATE LIMITED/);
    assert.match(watermarked, /watermarked/);
    assert.match(unlimited, /does not draw down credits/);
  });
});

describe("formatCredits", () => {
  it("renders the total first, then the breakdown", () => {
    const line = formatCredits({
      free_credits: 10,
      monthly_credits: 1_000,
      banked_credits: 224,
      sum_total_credits: 1_234,
    });

    assert.equal(line, "1234 credits total (free 10, monthly 1000, banked 224)");
  });
});

describe("buildEstimateBody", () => {
  it("tags the body with the kind and defaults the generation mode", () => {
    assert.deepEqual(buildEstimateBody("image", "flux_1_schnell", "artcraft", { image_batch_count: 2 }), {
      image_batch_count: 2,
      kind: "image",
      model: "flux_1_schnell",
      provider: "artcraft",
      generation_mode: { type: "text_to_image" },
    });

    assert.deepEqual(buildEstimateBody("video", "veo_3", "fal", undefined), {
      kind: "video",
      model: "veo_3",
      provider: "fal",
      generation_mode: { type: "text_to_video" },
    });
  });

  it("keeps an explicit generation mode and adds none for splats", () => {
    const edit = buildEstimateBody("image", "m", "p", { generation_mode: { type: "image_edit", count: 2 } });
    const splat = buildEstimateBody("splat", "marble_1p0", "artcraft", undefined);

    assert.deepEqual(edit["generation_mode"], { type: "image_edit", count: 2 });
    assert.equal(splat["generation_mode"], undefined);
  });

  it("rejects params that restate a top-level field instead of silently overriding it", () => {
    // Two values disagreeing about what is being priced is worse than an error.
    assert.throws(() => buildEstimateBody("image", "m", "p", { model: "other" }), (error: unknown) => {
      assert.match((error as Error).message, /params may not contain model/);

      return true;
    });
  });
});
