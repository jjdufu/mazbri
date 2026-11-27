import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Ban,
  Plus,
  RefreshCw,
  RotateCcw,
  Search,
} from "lucide-react";
import {
  fetchCertificateList,
  fetchIssuedCertificateSubjects,
  revokeCertificate,
} from "@/api/certificate";
import {
  AuthorizationTooltipButton,
  Badge,
  Button,
  ConfirmDialog,
  CopyableText,
  DataTablePage,
  Input,
  Select,
  StatusBadge,
  Tooltip,
  cn,
  type DataTableColumn,
} from "@/components/ui";
import { useT } from "@/locales";
import type { Certificate, CertificateStatus } from "@/types/api/certificate";
import { getStatusLabel } from "@/utils/display-labels";
import { EMPTY_PLACEHOLDER, formatDateTime } from "@/utils/format";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { logger } from "@/utils/sys/logger";
import { showSuccessToast } from "@/utils/ui/message-guard";
import { useLicenseGate } from "../shared/useLicenseGate";
import { CertificateDetailDrawer } from "./CertificateDetailDrawer";
import { IssueCertificateModal } from "./IssueCertificateModal";

function getDaysUntilExpiry(expiresAt: string): number {
  const expiry = new Date(expiresAt).getTime();
  const now = Date.now();
  return Math.ceil((expiry - now) / (1000 * 60 * 60 * 24));
}

function getCertificateStatus(cert: Certificate): CertificateStatus {
  if (cert.revoked) return "revoked";
  const diffDays =
    (new Date(cert.expires_at).getTime() - Date.now()) / (1000 * 60 * 60 * 24);
  if (diffDays < 0) return "expired";
  if (diffDays < 30) return "expiring_soon";
  return "valid";
}

function mapStatusBadgeCode(status: CertificateStatus): string {
  if (status === "valid") return "ACTIVE";
  if (status === "expiring_soon") return "EXPIRING";
  if (status === "expired") return "EXPIRED";
  return "REVOKED";
}

function formatShortFingerprint(fp: string): string {
  const normalized = fp.trim();
  if (normalized.length <= 20) return normalized;
  return `${normalized.slice(0, 8)}...${normalized.slice(-8)}`;
}

export function CertificatesPage() {
  const { t } = useT();
  const {
    authorizationDisabled,
    authorizationDisabledReason,
    loadLicenseStatus,
    ensureLicenseActive,
  } = useLicenseGate();

  const [rows, setRows] = useState<Certificate[]>([]);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(10);
  const [loading, setLoading] = useState(false);

  const [keywordInput, setKeywordInput] = useState("");
  const [appliedKeyword, setAppliedKeyword] = useState("");
  const [statusFilter, setStatusFilter] = useState<"" | CertificateStatus>("");

  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const [issueOpen, setIssueOpen] = useState(false);
  const [issuedSubjects, setIssuedSubjects] = useState<string[]>([]);

  const [detailOpen, setDetailOpen] = useState(false);
  const [selectedCert, setSelectedCert] = useState<Certificate | null>(null);

  const [revokeTarget, setRevokeTarget] = useState<Certificate | null>(null);
  const [batchRevokeIds, setBatchRevokeIds] = useState<string[] | null>(null);
  const [revoking, setRevoking] = useState(false);

  const loadData = useCallback(async () => {
    setLoading(true);
    try {
      const active = await loadLicenseStatus();
      const offset = (page - 1) * pageSize;
      try {
        const res = await fetchCertificateList({
          page,
          limit: pageSize,
          offset,
          q: appliedKeyword || undefined,
          status: statusFilter || undefined,
          include_revoked: statusFilter === "revoked" ? true : undefined,
        });
        setRows(res.data ?? []);
        setTotal(res.pagination?.total ?? 0);
        setSelectedIds([]);
      } catch (error) {
        setRows([]);
        setTotal(0);
        if (active) {
          logger.error("Failed to load certificate list:", error);
        }
      }
    } finally {
      setLoading(false);
    }
  }, [page, pageSize, appliedKeyword, statusFilter, loadLicenseStatus]);

  useEffect(() => {
    void loadData();
  }, [loadData]);

  const handleSearchSubmit = () => {
    setPage(1);
    setAppliedKeyword(keywordInput.trim());
  };

  const handleResetFilters = () => {
    setKeywordInput("");
    setAppliedKeyword("");
    setStatusFilter("");
    setPage(1);
  };

  const handleOpenIssue = async () => {
    if (!ensureLicenseActive()) return;
    try {
      const subjects = await fetchIssuedCertificateSubjects();
      setIssuedSubjects(subjects ?? []);
    } catch (error) {
      logger.error("Failed to load issued subjects:", error);
      setIssuedSubjects([]);
    }
    setIssueOpen(true);
  };

  const handleConfirmSingleRevoke = async () => {
    if (!revokeTarget || revoking || !ensureLicenseActive()) return;
    setRevoking(true);
    try {
      await revokeCertificate(revokeTarget.id);
      showSuccessToast(t("common.feedback.revokeSuccess"));
      setRevokeTarget(null);
      void loadData();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to revoke certificate:", error);
      }
    } finally {
      setRevoking(false);
    }
  };

  const handleConfirmBatchRevoke = async () => {
    if (!batchRevokeIds || batchRevokeIds.length === 0 || revoking) return;
    if (!ensureLicenseActive()) return;
    const targets = rows.filter(
      (r) => batchRevokeIds.includes(r.id) && !r.revoked,
    );
    if (targets.length === 0) {
      setBatchRevokeIds(null);
      return;
    }
    setRevoking(true);
    let revokedCount = 0;
    try {
      for (const target of targets) {
        try {
          await revokeCertificate(target.id);
          revokedCount += 1;
        } catch (error) {
          if (isMfaStepUpCancelled(error)) {
            break;
          }
          logger.error(`Failed to revoke certificate ${target.subject}:`, error);
        }
      }
      if (revokedCount > 0) {
        showSuccessToast(t("common.feedback.revokeSuccess"));
        setSelectedIds([]);
        void loadData();
      }
      setBatchRevokeIds(null);
    } finally {
      setRevoking(false);
    }
  };

  const columns = useMemo<DataTableColumn<Certificate>[]>(
    () => [
      {
        id: "subject",
        header: t("certificate.list.columns.subject"),
        minWidth: 164,
        cell: (row) => (
          <span className="font-semibold text-foreground">{row.subject}</span>
        ),
      },
      {
        id: "dns_names",
        header: t("certificate.list.columns.dnsNames"),
        minWidth: 184,
        cell: (row) => {
          const names = row.dns_names ?? [];
          if (names.length === 1 && names[0] === row.subject) {
            return (
              <Badge variant="neutral">
                {t("certificate.list.sameAsSubject")}
              </Badge>
            );
          }
          if (names.length > 1) {
            return (
              <div className="inline-flex items-center gap-1.5 min-w-0">
                <span className="truncate text-foreground">{names[0]}</span>
                <Tooltip content={names.join(", ")}>
                  <Badge variant="outline">+{names.length - 1} SAN</Badge>
                </Tooltip>
              </div>
            );
          }
          if (names.length === 1) {
            return <Badge variant="outline">{names[0]}</Badge>;
          }
          return (
            <span className="text-muted-foreground">{EMPTY_PLACEHOLDER}</span>
          );
        },
      },
      {
        id: "fingerprint",
        header: t("certificate.list.columns.fingerprint"),
        minWidth: 196,
        cell: (row) =>
          row.fingerprint ? (
            <CopyableText
              value={row.fingerprint}
              displayText={formatShortFingerprint(row.fingerprint)}
              copyTooltip={t("certificate.list.copyFingerprint")}
            />
          ) : (
            <span className="text-muted-foreground">{EMPTY_PLACEHOLDER}</span>
          ),
      },
      {
        id: "expires_at",
        header: t("certificate.list.columns.expiresAt"),
        minWidth: 184,
        cell: (row) => {
          const status = getCertificateStatus(row);
          const days = getDaysUntilExpiry(row.expires_at);
          const toneClass =
            status === "expired"
              ? "text-danger-fg"
              : status === "expiring_soon"
                ? "text-warning-fg"
                : "text-foreground";
          const subtitle = row.revoked
            ? t("common.statusLabels.revoked")
            : days < 0
              ? t("certificate.list.expiredDaysAgo", { count: Math.abs(days) })
              : t("certificate.list.remainingDays", { count: days });
          return (
            <div className="flex flex-col leading-snug">
              <span className={cn("font-mono-tech tabular-nums", toneClass)}>
                {formatDateTime(row.expires_at)}
              </span>
              <span
                className={cn(
                  "text-[11px]",
                  status === "expired" || status === "expiring_soon"
                    ? toneClass
                    : "text-muted-foreground",
                )}
              >
                {subtitle}
              </span>
            </div>
          );
        },
      },
      {
        id: "status",
        header: t("certificate.list.columns.status"),
        width: 112,
        cell: (row) => {
          const status = getCertificateStatus(row);
          return (
            <StatusBadge
              status={mapStatusBadgeCode(status)}
              label={getStatusLabel(status)}
            />
          );
        },
      },
      {
        id: "operations",
        header: t("common.fields.operations"),
        width: 140,
        align: "right",
        cell: (row) => {
          const revokeDisabled = row.revoked || authorizationDisabled;
          const revokeReason = row.revoked
            ? t("certificate.list.disabledReasons.revoked")
            : authorizationDisabledReason;
          return (
            <div
              className="inline-flex items-center justify-end gap-1"
              onClick={(e) => e.stopPropagation()}
            >
              <Button
                type="button"
                variant="link"
                size="xs"
                onClick={() => {
                  setSelectedCert(row);
                  setDetailOpen(true);
                }}
              >
                {t("common.actions.details")}
              </Button>
              <AuthorizationTooltipButton
                type="button"
                variant="link"
                size="xs"
                disabled={revokeDisabled}
                disabledReason={revokeReason}
                className="text-danger-fg hover:text-danger-fg hover:bg-danger-fg/10"
                onClick={() => setRevokeTarget(row)}
              >
                {t("certificate.list.actions.revoke")}
              </AuthorizationTooltipButton>
            </div>
          );
        },
      },
    ],
    [t, authorizationDisabled, authorizationDisabledReason],
  );

  const hasFilters = Boolean(appliedKeyword || statusFilter);

  return (
    <>
      <DataTablePage<Certificate>
        testId="certificates-page"
        columns={columns}
        data={rows}
        getRowId={(row) => row.id}
        loading={loading}
        selectable
        selectedKeys={selectedIds}
        onSelectionChange={setSelectedIds}
        emptyText={
          hasFilters
            ? t("common.feedback.noMatchingData")
            : t("common.feedback.noData")
        }
        page={page}
        pageSize={pageSize}
        total={total}
        pageSizeOptions={[10, 20, 50]}
        onPageChange={setPage}
        onPageSizeChange={(nextSize) => {
          setPageSize(nextSize);
          setPage(1);
        }}
        onRowClick={(row) => {
          setSelectedCert(row);
          setDetailOpen(true);
        }}
        filters={
          <>
            <div className="relative w-56">
              <Search className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground" />
              <Input
                value={keywordInput}
                onChange={(e) => setKeywordInput(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") handleSearchSubmit();
                }}
                placeholder={t("common.search")}
                aria-label={t("common.search")}
                className="pl-8"
                data-testid="certificates-search-input"
              />
            </div>

            <Select
              value={statusFilter}
              onChange={(e) => {
                setStatusFilter(e.target.value as "" | CertificateStatus);
                setPage(1);
              }}
              aria-label={t("certificate.list.columns.status")}
              className="w-36"
              options={[
                {
                  value: "",
                  label: `${t("certificate.list.columns.status")}: ${t("common.all")}`,
                  text: t("common.all"),
                },
                {
                  value: "valid",
                  label: t("common.statusLabels.valid"),
                  text: t("common.statusLabels.valid"),
                },
                {
                  value: "expiring_soon",
                  label: t("common.statusLabels.expiringSoon"),
                  text: t("common.statusLabels.expiringSoon"),
                },
                {
                  value: "expired",
                  label: t("common.statusLabels.expired"),
                  text: t("common.statusLabels.expired"),
                },
                {
                  value: "revoked",
                  label: t("common.statusLabels.revoked"),
                  text: t("common.statusLabels.revoked"),
                },
              ]}
            />

            <Button
              type="button"
              variant="outline"
              size="md"
              onClick={handleSearchSubmit}
            >
              {t("common.search")}
            </Button>

            {hasFilters && (
              <Button
                type="button"
                variant="ghost"
                size="md"
                onClick={handleResetFilters}
              >
                <RotateCcw className="h-3.5 w-3.5" />
                {t("common.reset")}
              </Button>
            )}
          </>
        }
        batchActions={(ids) => (
          <AuthorizationTooltipButton
            type="button"
            variant="danger"
            size="md"
            disabled={authorizationDisabled}
            disabledReason={authorizationDisabledReason}
            onClick={() => setBatchRevokeIds(ids)}
            data-testid="certificates-batch-revoke-btn"
          >
            <Ban className="h-3.5 w-3.5" />
            {t("certificate.list.actions.revoke")}
          </AuthorizationTooltipButton>
        )}
        actions={
          <>
            <Button
              type="button"
              variant="outline"
              size="md"
              onClick={() => void loadData()}
            >
              <RefreshCw className="h-3.5 w-3.5" />
              {t("common.actions.refresh")}
            </Button>
            <AuthorizationTooltipButton
              type="button"
              variant="primary"
              size="md"
              disabled={authorizationDisabled}
              disabledReason={authorizationDisabledReason}
              onClick={() => void handleOpenIssue()}
              data-testid="certificates-issue-btn"
            >
              <Plus className="h-3.5 w-3.5" />
              {t("certificate.list.actions.issue")}
            </AuthorizationTooltipButton>
          </>
        }
      />

      <IssueCertificateModal
        open={issueOpen}
        issuedSubjects={issuedSubjects}
        authorizationDisabled={authorizationDisabled}
        authorizationDisabledReason={authorizationDisabledReason}
        onClose={() => setIssueOpen(false)}
        onSuccess={() => void loadData()}
      />

      <CertificateDetailDrawer
        open={detailOpen}
        certId={selectedCert?.id ?? null}
        initialCert={selectedCert}
        authorizationDisabled={authorizationDisabled}
        authorizationDisabledReason={authorizationDisabledReason}
        onClose={() => setDetailOpen(false)}
        onChanged={() => void loadData()}
      />

      <ConfirmDialog
        open={Boolean(revokeTarget)}
        onClose={() => setRevokeTarget(null)}
        onConfirm={() => void handleConfirmSingleRevoke()}
        title={t("common.warning")}
        description={t("certificate.messages.revokeConfirm", {
          name: revokeTarget?.subject ?? "",
        })}
        confirmLabel={t("certificate.messages.confirmRevoke")}
        cancelLabel={t("common.cancel")}
        danger
        loading={revoking}
      />

      <ConfirmDialog
        open={Boolean(batchRevokeIds && batchRevokeIds.length > 0)}
        onClose={() => setBatchRevokeIds(null)}
        onConfirm={() => void handleConfirmBatchRevoke()}
        title={t("common.warning")}
        description={t("certificate.messages.revokeConfirm", {
          name: `${batchRevokeIds?.length ?? 0}`,
        })}
        confirmLabel={t("certificate.messages.confirmRevoke")}
        cancelLabel={t("common.cancel")}
        danger
        loading={revoking}
      />
    </>
  );
}
