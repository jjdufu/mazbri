import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Award,
  Download,
  FileKey2,
  FileText,
  Loader2,
  Upload,
  X,
} from "lucide-react";
import {
  exportLicenseRequest,
  fetchLicenseStatus,
  importLicense,
} from "@/api/authorizations";
import {
  Button,
  EmptyState,
  SettingRow,
  SettingsSectionCard,
  SettingsSectionLayout,
  StatusBadge,
  Textarea,
  cn,
} from "@/components/ui";
import { useT } from "@/locales";
import type { LicenseStatus } from "@/types/api/authorizations";
import { getStatusLabel } from "@/utils/display-labels";
import { EMPTY_PLACEHOLDER, formatBytes, formatDateTime } from "@/utils/format";
import { getUserFacingErrorMessage } from "@/utils/http/error";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { logger } from "@/utils/sys/logger";
import {
  showErrorToast,
  showSuccessToast,
  showWarningToast,
} from "@/utils/ui/message-guard";

const ACCEPTED_EXT_RE = /\.(jws|txt)$/i;

function mapLicenseStatusBadge(status: string): string {
  if (status === "active") return "ACTIVE";
  if (status === "grace" || status === "pending") return "WARNING";
  if (status === "expired") return "EXPIRED";
  if (status === "invalid") return "ERROR";
  return status.toUpperCase();
}

export function PlatformLicenseTab() {
  const { t } = useT();
  const fileInputRef = useRef<HTMLInputElement | null>(null);

  const [packageFile, setPackageFile] = useState<File | null>(null);
  const [licenseText, setLicenseText] = useState("");
  const [dragging, setDragging] = useState(false);

  const [summary, setSummary] = useState<LicenseStatus | null>(null);
  const [loadingStatus, setLoadingStatus] = useState(false);
  const [importing, setImporting] = useState(false);
  const [exporting, setExporting] = useState(false);

  const loadStatus = useCallback(
    async (withSpinner = true) => {
      if (withSpinner) setLoadingStatus(true);
      try {
        const res = await fetchLicenseStatus();
        setSummary(res);
      } catch (error) {
        logger.error("Failed to load license status:", error);
        showErrorToast(
          getUserFacingErrorMessage(error, t("httpMsg.requestFailed")),
        );
      } finally {
        if (withSpinner) setLoadingStatus(false);
      }
    },
    [t],
  );

  useEffect(() => {
    void loadStatus(true);
  }, [loadStatus]);

  const acceptFile = (file: File | null | undefined) => {
    if (!file) return;
    if (!ACCEPTED_EXT_RE.test(file.name)) {
      showWarningToast(
        t("certificate.authorizationManagement.filePicker.invalidType"),
      );
      return;
    }
    setPackageFile(file);
  };

  const handleImport = async () => {
    if (importing) return;
    let file = packageFile;
    const rawText = licenseText.trim();
    if (!file && rawText) {
      file = new File([rawText], "license.jws", { type: "application/jose" });
    }
    if (!file) {
      showWarningToast(
        t("certificate.authorizationManagement.license.selectFile"),
      );
      return;
    }

    setImporting(true);
    try {
      await importLicense(file);
      setPackageFile(null);
      setLicenseText("");
      await loadStatus(false);
      showSuccessToast(
        t("certificate.authorizationManagement.license.importSuccess"),
      );
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to import license:", error);
      }
    } finally {
      setImporting(false);
    }
  };

  const handleExportRequest = async () => {
    if (exporting) return;
    setExporting(true);
    try {
      const requestFile = await exportLicenseRequest();
      const blob = new Blob([JSON.stringify(requestFile, null, 2)], {
        type: "application/json",
      });
      const url = URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = url;
      link.download = "mazbri-license-request.json";
      link.click();
      URL.revokeObjectURL(url);
      showSuccessToast(
        t("certificate.authorizationManagement.license.exportSuccess"),
      );
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to export license request:", error);
      }
    } finally {
      setExporting(false);
    }
  };

  const formatSecondsToExpiry = useCallback(
    (value: number | null | undefined): string => {
      if (value == null) return EMPTY_PLACEHOLDER;
      const days = Math.ceil(Math.abs(Math.round(value)) / 86400);
      return value < 0
        ? t("certificate.authorizationManagement.license.expiredByDays", {
            count: days,
          })
        : t("certificate.authorizationManagement.license.expiresInDays", {
            count: days,
          });
    },
    [t],
  );

  const summaryRows = useMemo(() => {
    if (!summary || summary.sequence == null) return [];
    const statusLabel =
      summary.status === "active"
        ? t("certificate.authorizationManagement.status.active")
        : summary.status === "grace"
          ? t("certificate.authorizationManagement.status.grace")
          : summary.status === "expired"
            ? t("certificate.authorizationManagement.status.expired")
            : summary.status === "invalid"
              ? t("certificate.authorizationManagement.status.invalid")
              : summary.status === "pending"
                ? t("certificate.authorizationManagement.status.pending")
                : summary.status === "unconfigured"
                  ? t("certificate.authorizationManagement.status.unconfigured")
                  : getStatusLabel(summary.status);

    const rows: Array<{
      label: string;
      value: string;
      isStatus?: boolean;
      statusCode?: string;
      mono?: boolean;
    }> = [
      {
        label: t("certificate.authorizationManagement.license.status"),
        value: statusLabel,
        isStatus: true,
        statusCode: mapLicenseStatusBadge(summary.status),
      },
      {
        label: t("certificate.authorizationManagement.license.sequence"),
        value: String(summary.sequence),
        mono: true,
      },
      {
        label: t("certificate.authorizationManagement.license.keyVersion"),
        value: summary.keyVersion || EMPTY_PLACEHOLDER,
        mono: true,
      },
      {
        label: t("certificate.authorizationManagement.license.customerId"),
        value: summary.customerId || EMPTY_PLACEHOLDER,
        mono: true,
      },
      {
        label: t("certificate.authorizationManagement.license.expiresAt"),
        value: formatDateTime(summary.expiresAt),
        mono: true,
      },
      {
        label: t("certificate.authorizationManagement.license.graceUntil"),
        value: formatDateTime(summary.graceUntil),
        mono: true,
      },
      {
        label: t("certificate.authorizationManagement.license.remaining"),
        value: formatSecondsToExpiry(summary.secondsToExpiry),
      },
    ];

    if (summary.status === "grace") {
      const days = Math.ceil(
        Math.max(summary.secondsToGraceEnd ?? 0, 0) / 86400,
      );
      rows.push({
        label: t("certificate.authorizationManagement.license.notice"),
        value: t("certificate.authorizationManagement.license.graceNotice", {
          count: days,
        }),
      });
    } else if (summary.status === "expired") {
      rows.push({
        label: t("certificate.authorizationManagement.license.notice"),
        value: t("certificate.authorizationManagement.license.expiredNotice"),
      });
    }
    return rows;
  }, [summary, t, formatSecondsToExpiry]);

  const importDisabled =
    (!packageFile && !licenseText.trim()) || importing;

  return (
    <SettingsSectionLayout testId="platform-license-tab" columns={2}>
      <SettingsSectionCard
        title={t("certificate.authorizationManagement.sections.import")}
        icon={<FileKey2 className="h-4 w-4" />}
      >
        <input
          ref={fileInputRef}
          type="file"
          accept=".jws,.txt,text/plain,application/jose"
          className="hidden"
          onChange={(e) => {
            acceptFile(e.target.files?.[0]);
            e.target.value = "";
          }}
        />

        <div
          onDragOver={(e) => {
            e.preventDefault();
            setDragging(true);
          }}
          onDragLeave={() => setDragging(false)}
          onDrop={(e) => {
            e.preventDefault();
            setDragging(false);
            acceptFile(e.dataTransfer.files?.[0]);
          }}
          onClick={() => fileInputRef.current?.click()}
          className={cn(
            "flex flex-col items-center justify-center gap-2 rounded-lg border border-dashed px-4 py-6 text-center transition-colors cursor-pointer",
            dragging
              ? "border-primary bg-primary-soft/50"
              : "border-border bg-muted/20 hover:border-primary/60 hover:bg-hover-bg/60",
          )}
        >
          {packageFile ? (
            <div className="flex items-center gap-2.5 text-xs">
              <FileText className="h-5 w-5 text-primary shrink-0" />
              <div className="text-left min-w-0">
                <div className="font-medium text-foreground truncate max-w-[260px]">
                  {packageFile.name}
                </div>
                <div className="text-[11px] text-muted-foreground font-mono-tech">
                  {formatBytes(packageFile.size)}
                </div>
              </div>
              <button
                type="button"
                aria-label={t("common.filePicker.remove")}
                onClick={(e) => {
                  e.stopPropagation();
                  setPackageFile(null);
                }}
                className="inline-flex h-6 w-6 items-center justify-center rounded-md text-muted-foreground hover:text-foreground hover:bg-hover-bg cursor-pointer"
              >
                <X className="h-3.5 w-3.5" />
              </button>
            </div>
          ) : (
            <>
              <Upload className="h-5 w-5 text-muted-foreground" />
              <div className="text-xs font-medium text-foreground">
                {t("certificate.authorizationManagement.filePicker.title")}
              </div>
              <div className="text-[11px] text-muted-foreground">
                {t("certificate.authorizationManagement.filePicker.hint")}
              </div>
            </>
          )}
        </div>

        <SettingRow
          label={t("certificate.authorizationManagement.license.pasteLabel")}
          htmlFor="license-jws-textarea"
        >
          <Textarea
            id="license-jws-textarea"
            rows={4}
            value={licenseText}
            onChange={(e) => setLicenseText(e.target.value)}
            placeholder="eyJhbGciOiJFZERTQSJ9..."
            className="font-mono-tech"
          />
        </SettingRow>

        <div className="flex flex-wrap items-center gap-2 pt-1">
          <Button
            type="button"
            variant="primary"
            size="md"
            loading={importing}
            disabled={importDisabled}
            onClick={() => void handleImport()}
            data-testid="license-import-btn"
          >
            <Upload className="h-3.5 w-3.5" />
            {t("certificate.authorizationManagement.license.importLicense")}
          </Button>
          <Button
            type="button"
            variant="outline"
            size="md"
            loading={exporting}
            onClick={() => void handleExportRequest()}
            data-testid="license-export-request-btn"
          >
            <Download className="h-3.5 w-3.5" />
            {t("certificate.authorizationManagement.license.exportRequest")}
          </Button>
        </div>
      </SettingsSectionCard>

      <SettingsSectionCard
        title={t("certificate.authorizationManagement.sections.detail")}
        icon={<Award className="h-4 w-4" />}
      >
        {loadingStatus && !summary ? (
          <div className="flex items-center justify-center py-12 text-xs text-muted-foreground gap-2">
            <Loader2 className="h-4 w-4 animate-spin text-primary" />
            <span>{t("layout.loading")}</span>
          </div>
        ) : summaryRows.length > 0 ? (
          <dl className="border border-border rounded-lg overflow-hidden divide-y divide-border text-xs">
            {summaryRows.map((row) => (
              <div
                key={row.label}
                className="grid grid-cols-[128px_1fr] items-center gap-3 px-3.5 py-2.5"
              >
                <dt className="text-muted-foreground font-medium">
                  {row.label}
                </dt>
                <dd
                  className={cn(
                    "text-foreground break-all",
                    row.mono && "font-mono-tech tabular-nums",
                  )}
                >
                  {row.isStatus ? (
                    <StatusBadge
                      status={row.statusCode ?? ""}
                      label={row.value}
                    />
                  ) : (
                    row.value
                  )}
                </dd>
              </div>
            ))}
          </dl>
        ) : (
          <EmptyState
            title={t(
              "certificate.authorizationManagement.sections.emptyDetail",
            )}
          />
        )}
      </SettingsSectionCard>
    </SettingsSectionLayout>
  );
}
