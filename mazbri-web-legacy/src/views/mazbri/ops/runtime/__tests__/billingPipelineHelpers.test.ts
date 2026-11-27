import assert from "node:assert/strict";
import { test } from "node:test";
import {
  buildDeadLettersQueueQuery,
  buildFailuresQueueQuery,
  formatPipelineStageLabel,
  resolvePipelineStageI18nKey,
} from "../modules/billingPipelineHelpers";

const ZH_STAGE_LABELS: Record<string, string> = {
  "ops.billing.stages.outbox": "网关用量采集队列",
  "ops.billing.stages.queue": "实时计费待扣队列",
  "ops.billing.stages.ledger": "账本落库流水",
};

test("resolvePipelineStageI18nKey desensitizes physical database table names", () => {
  assert.equal(
    resolvePipelineStageI18nKey("usage_outbox"),
    "ops.billing.stages.outbox",
  );
  assert.equal(
    resolvePipelineStageI18nKey("usage_charge_queue"),
    "ops.billing.stages.queue",
  );
  assert.equal(
    resolvePipelineStageI18nKey("ledger_entries"),
    "ops.billing.stages.ledger",
  );
  assert.equal(
    formatPipelineStageLabel("usage_outbox", (k) => ZH_STAGE_LABELS[k] ?? k),
    "网关用量采集队列",
  );
  assert.equal(
    formatPipelineStageLabel(
      "usage_charge_queue",
      (k) => ZH_STAGE_LABELS[k] ?? k,
    ),
    "实时计费待扣队列",
  );
  assert.equal(
    formatPipelineStageLabel("ledger_entries", (k) => ZH_STAGE_LABELS[k] ?? k),
    "账本落库流水",
  );
});

test("buildFailuresQueueQuery and buildDeadLettersQueueQuery isolate queue pagination", () => {
  assert.deepEqual(buildFailuresQueueQuery("6h", 2, 20), {
    range: "6h",
    limit: 20,
    failed_offset: 20,
    queue: "failures",
  });
  assert.deepEqual(buildDeadLettersQueueQuery("24h", 3, 20), {
    range: "24h",
    limit: 20,
    dlq_offset: 40,
    queue: "dead_letters",
  });
});
