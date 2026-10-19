
import { $t } from "@/locales";

export const canMarkPaid = (status?: string) =>
  ["pending", "processing", "overdue"].includes(String(status ?? "").toLowerCase());
export const canMarkOverdue = (status?: string) =>
  ["pending", "processing"].includes(String(status ?? "").toLowerCase());
export const getStatusLabel = (status?: string) => {
  const key = String(status ?? "pending").toLowerCase();
  const map: Record<string, string> = {
    pending: $t("billing.status.pending"),
    processing: $t("billing.status.processing"),
    paid: $t("billing.status.paid"),
    overdue: $t("billing.status.overdue"),
    cancelled: $t("billing.status.cancelled"),
  };
  return map[key] ?? status ?? "-";
};
export const statusTagType = (
  status?: string,
): "success" | "warning" | "info" | "danger" | "primary" => {
  switch (String(status ?? "").toLowerCase()) {
    case "paid":
      return "success";
    case "overdue":
      return "danger";
    case "processing":
      return "warning";
    case "cancelled":
      return "info";
    default:
      return "primary";
  }
};
