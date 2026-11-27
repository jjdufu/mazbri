import type { BillingFailuresQuery, OpsRange } from "@/types/api/billing-ops";

export type PipelineStageI18nKey =
  | "ops.billing.stages.outbox"
  | "ops.billing.stages.queue"
  | "ops.billing.stages.ledger";

const STAGE_KEY_MAP: Record<string, PipelineStageI18nKey> = {
  outbox: "ops.billing.stages.outbox",
  usage_outbox: "ops.billing.stages.outbox",
  queue: "ops.billing.stages.queue",
  usage_charge_queue: "ops.billing.stages.queue",
  ledger: "ops.billing.stages.ledger",
  ledger_entries: "ops.billing.stages.ledger",
};

/**
 * 将数据库物理表名或阶段代码映射为标准业务阶段 i18n key，防止向 UI 暴露底层表名。
 */
export function resolvePipelineStageI18nKey(
  raw: string | null | undefined,
): PipelineStageI18nKey | null {
  if (!raw) return null;
  const normalized = raw.trim().toLowerCase();
  return STAGE_KEY_MAP[normalized] ?? null;
}

export function formatPipelineStageLabel(
  raw: string | null | undefined,
  translate: (key: string) => string,
): string {
  const i18nKey = resolvePipelineStageI18nKey(raw);
  if (i18nKey) {
    return translate(i18nKey);
  }
  return raw?.trim() || "—";
}

export function buildFailuresQueueQuery(
  range: OpsRange,
  page: number,
  pageSize: number,
): BillingFailuresQuery {
  const safeSize = Math.max(1, pageSize);
  const safePage = Math.max(1, page);
  return {
    range,
    limit: safeSize,
    failed_offset: (safePage - 1) * safeSize,
    queue: "failures",
  };
}

export function buildDeadLettersQueueQuery(
  range: OpsRange,
  page: number,
  pageSize: number,
): BillingFailuresQuery {
  const safeSize = Math.max(1, pageSize);
  const safePage = Math.max(1, page);
  return {
    range,
    limit: safeSize,
    dlq_offset: (safePage - 1) * safeSize,
    queue: "dead_letters",
  };
}
