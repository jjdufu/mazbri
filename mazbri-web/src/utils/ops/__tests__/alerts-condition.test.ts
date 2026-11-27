import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { formatRuleCondition } from "../alerts-format";

const zhMap: Record<string, string> = {
  "ops.alerts.rules.conditionImmediate": "立即触发",
  "ops.alerts.rules.conditionFor": "持续 {duration}",
  "ops.alerts.rules.conditionSloBurn": "燃烧触发",
};

const tZh = (key: string, params?: Record<string, unknown>) => {
  let out = zhMap[key] ?? key;
  for (const [k, v] of Object.entries(params ?? {})) {
    out = out.replace(`{${k}}`, String(v));
  }
  return out;
};

describe("formatRuleCondition", () => {
  it("renders SLO boolean burn rules without percentage misinterpretation", () => {
    const out = formatRuleCondition(
      {
        expr: 'mazbri_slo_burn_page{slo="gateway"}',
        op: ">",
        threshold: 0.5,
        for_seconds: 0,
        labels: { category: "slo" },
      },
      tZh,
      "zh",
    );
    assert.equal(out, "燃烧触发 · 立即触发");
    assert.ok(!out.includes("50%"));
  });

  it("renders threshold with labels.unit and human duration", () => {
    const out = formatRuleCondition(
      {
        expr: "gateway_5xx_ratio",
        op: ">",
        threshold: 5,
        for_seconds: 120,
        labels: { unit: "%" },
      },
      tZh,
      "en",
    );
    assert.equal(out, "> 5% · 持续 2m");
  });
});
