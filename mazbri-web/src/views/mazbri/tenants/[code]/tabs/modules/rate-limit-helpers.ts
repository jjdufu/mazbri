
import { $t } from "@/locales";

export const commandTypeLabel = (type: string) => {
  if (type === "deny_until") return $t("rateLimit.typeDeny");
  if (type === "quota_correction") return $t("rateLimit.typeQuota");
  if (type === "refresh_policy") return $t("rateLimit.typeRefresh");
  return type;
};

export const formatTime = (value?: string | null) => {
  if (!value) return "—";
  try {
    return new Date(value).toLocaleString();
  } catch {
    return value;
  }
};
