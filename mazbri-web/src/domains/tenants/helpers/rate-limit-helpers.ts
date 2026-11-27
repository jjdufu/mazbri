import { $t } from "@/locales";
import { EMPTY_PLACEHOLDER, formatDateTime } from "@/utils/format";

export function commandTypeLabel(type: string): string {
  if (type === "deny_until") return $t("rateLimit.typeDeny");
  if (type === "quota_correction") return $t("rateLimit.typeQuota");
  if (type === "refresh_policy") return $t("rateLimit.typeRefresh");
  return type;
}

export function formatTime(value?: string | null): string {
  if (!value) return EMPTY_PLACEHOLDER;
  try {
    return formatDateTime(value);
  } catch {
    return value;
  }
}
