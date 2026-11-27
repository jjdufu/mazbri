import { $t } from "@/locales";
import { EMPTY_PLACEHOLDER } from "@/utils/format";

export const canMarkPaid = (status?: string): boolean =>
  ["pending", "processing", "overdue"].includes(
    String(status ?? "").toLowerCase(),
  );

export const canMarkOverdue = (status?: string): boolean =>
  ["pending", "processing"].includes(String(status ?? "").toLowerCase());

export const getStatusLabel = (status?: string): string => {
  const key = String(status ?? "pending").toLowerCase();
  const map: Record<string, string> = {
    pending: $t("billing.status.pending"),
    processing: $t("billing.status.processing"),
    paid: $t("billing.status.paid"),
    overdue: $t("billing.status.overdue"),
    cancelled: $t("billing.status.cancelled"),
  };
  return map[key] ?? status ?? EMPTY_PLACEHOLDER;
};

export const statusBadgeKey = (status?: string): string => {
  switch (String(status ?? "").toLowerCase()) {
    case "paid":
      return "SUCCEEDED";
    case "overdue":
      return "FAILED";
    case "processing":
      return "PROCESSING";
    case "cancelled":
      return "DISABLED";
    default:
      return "PENDING";
  }
};
