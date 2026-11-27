import type { TenantProductConfig } from "@/types/api/tenant";
import { $t } from "@/locales";
import { EMPTY_PLACEHOLDER } from "@/utils/format";

type ConflictField = {
  labelKey: string;
  attempted: unknown;
  latest: unknown;
};

function stringifyValue(value: unknown): string {
  if (typeof value === "boolean") {
    return value ? $t("common.status.enabled") : $t("common.status.disabled");
  }
  if (value == null) return EMPTY_PLACEHOLDER;
  if (Array.isArray(value)) return value.length ? value.join(", ") : EMPTY_PLACEHOLDER;
  if (typeof value === "object") return JSON.stringify(value, null, 2);
  return String(value);
}

function isSameValue(left: unknown, right: unknown): boolean {
  return stringifyValue(left) === stringifyValue(right);
}

function buildConflictSummary(
  scopeKey: string,
  updatedAt: string | undefined,
  fields: ConflictField[],
): string {
  const changedFields = fields.filter(
    (field) => !isSameValue(field.attempted, field.latest),
  );
  const scopeLabel = $t(scopeKey);
  if (changedFields.length === 0) {
    return [
      $t("common.conflict.updatedByOthers", { scopeLabel }),
      $t("common.conflict.latestUpdatedAt", { updatedAt: updatedAt ?? EMPTY_PLACEHOLDER }),
    ].join("\n");
  }
  return [
    $t("common.conflict.recheckLatest", { scopeLabel }),
    $t("common.conflict.latestUpdatedAt", { updatedAt: updatedAt ?? EMPTY_PLACEHOLDER }),
    "",
    ...changedFields.flatMap((field) => [
      $t(field.labelKey),
      $t("common.conflict.yourSubmission", {
        value: stringifyValue(field.attempted),
      }),
      $t("common.conflict.latestValue", {
        value: stringifyValue(field.latest),
      }),
      "",
    ]),
  ].join("\n");
}

export function summarizeTenantProductConflict(
  attempted: TenantProductConfig,
  latest: TenantProductConfig,
  updatedAt?: string,
): string {
  return buildConflictSummary("common.conflict.productScope", updatedAt, [
    {
      labelKey: "common.conflict.productUnitPrice",
      attempted: attempted.unit_price,
      latest: latest.unit_price,
    },
    {
      labelKey: "common.conflict.productCbProFallback",
      attempted: attempted.cb_pro_fallback,
      latest: latest.cb_pro_fallback,
    },
    {
      labelKey: "common.conflict.productCbProPolicy",
      attempted: attempted.cbpro,
      latest: latest.cbpro,
    },
  ]);
}
