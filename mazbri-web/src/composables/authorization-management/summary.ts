import { computed, type Ref } from "vue";
import type { LicenseStatus } from "@/types/api/authorizations";
import { $t } from "@/locales";
import { formatDateTime } from "@/utils/format";
import { getStatusLabel, getStatusType, type TagType } from "@/utils/display-labels";

type SummaryRow = {
  label: string;
  value: string;
  kind?: "text" | "tag" | "code";
  tagType?: TagType;
};

export function useAuthorizationManagementSummary(options: {
  currentSummary: Ref<LicenseStatus | null>;
}) {
  function formatSecondsToExpiry(value: number | null | undefined): string {
    if (value == null) {
      return "-";
    }
    const days = Math.ceil(Math.abs(Math.round(value)) / 86400);
    return value < 0
      ? $t("certificate.authorizationManagement.license.expiredByDays", {
          count: days,
        })
      : $t("certificate.authorizationManagement.license.expiresInDays", {
          count: days,
        });
  }

  function buildSummaryRows(summary: LicenseStatus | null): SummaryRow[] {
    if (!summary || summary.sequence == null) {
      return [];
    }
    const known = ["active", "grace", "expired", "invalid", "pending", "unconfigured"];
    const rows: SummaryRow[] = [
      {
        label: $t("certificate.authorizationManagement.license.status"),
        value: known.includes(summary.status)
          ? $t(`certificate.authorizationManagement.status.${summary.status}`)
          : getStatusLabel(summary.status),
        kind: "tag",
        tagType: getStatusType(summary.status),
      },
      {
        label: $t("certificate.authorizationManagement.license.sequence"),
        value: String(summary.sequence),
      },
      {
        label: $t("certificate.authorizationManagement.license.keyVersion"),
        value: summary.keyVersion || "-",
      },
      {
        label: $t("certificate.authorizationManagement.license.customerId"),
        value: summary.customerId || "-",
      },
      {
        label: $t("certificate.authorizationManagement.license.expiresAt"),
        value: formatDateTime(summary.expiresAt),
      },
      {
        label: $t("certificate.authorizationManagement.license.graceUntil"),
        value: formatDateTime(summary.graceUntil),
      },
      {
        label: $t("certificate.authorizationManagement.license.remaining"),
        value: formatSecondsToExpiry(summary.secondsToExpiry),
      },
    ];
    if (summary.status === "grace") {
      const days = Math.ceil(Math.max(summary.secondsToGraceEnd ?? 0, 0) / 86400);
      rows.push({
        label: $t("certificate.authorizationManagement.license.notice"),
        value: $t("certificate.authorizationManagement.license.graceNotice", {
          count: days,
        }),
      });
    }
    if (summary.status === "expired") {
      rows.push({
        label: $t("certificate.authorizationManagement.license.notice"),
        value: $t("certificate.authorizationManagement.license.expiredNotice"),
      });
    }
    return rows;
  }

  const hasSummary = computed(() => options.currentSummary.value?.sequence != null);
  const statusEmptyDescription = computed(() =>
    $t("certificate.authorizationManagement.sections.emptyDetail"),
  );
  const summaryRows = computed(() => buildSummaryRows(options.currentSummary.value));

  return {
    hasSummary,
    statusEmptyDescription,
    summaryRows,
  };
}
