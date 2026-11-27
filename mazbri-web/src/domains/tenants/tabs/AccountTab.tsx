import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  AlertTriangle,
  CirclePlus,
  RefreshCw,
  RotateCcw,
  Wallet,
} from "lucide-react";
import {
  fetchTenantAccount,
  fetchTenantSpend,
  getTenantTransactions,
  rechargeTenantAccount,
  refundTenantAccount,
  type TenantSpend,
} from "@/api/tenant";
import {
  Badge,
  Button,
  DataTablePage,
  EntityDetailCard,
  FormField,
  FormModal,
  Input,
  Select,
  StatusBadge,
  Textarea,
  cn,
  type DataTableColumn,
  type SelectOptionItem,
} from "@/components/ui";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import type {
  RechargeRequest,
  RefundRequest,
  TenantAccount,
  Transaction,
  TransactionListQuery,
} from "@/types/api/tenant";
import {
  EMPTY_PLACEHOLDER,
  centsToYuanString,
  formatDateTime,
  formatDisplayDateYmd,
  formatMoney,
  yuanToCentsExact,
} from "@/utils/format";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { isBizRole } from "@/utils/ops/roles";
import { logger } from "@/utils/sys/logger";
import {
  showErrorToast,
  showSuccessToast,
} from "@/utils/ui/message-guard";
import { MfaRequiredHint } from "../components/MfaRequiredHint";

export interface AccountTabProps {
  tenantCode: string;
}

type TxFilterType =
  | ""
  | "credit"
  | "refund"
  | "charge"
  | "adjustment"
  | "payment";

function last30DayRangeUtc8(): { from: string; to: string } {
  const now = Date.now();
  const fromMs = now - 29 * 86_400_000;
  return {
    from: formatDisplayDateYmd(fromMs),
    to: formatDisplayDateYmd(now),
  };
}

export function AccountTab({ tenantCode }: AccountTabProps) {
  const { t } = useT();
  const role = useAuthStore((s) => s.info.role);
  const canWrite = isBizRole(role);

  const [accountLoading, setAccountLoading] = useState(false);
  const [account, setAccount] = useState<TenantAccount | null>(null);
  const [spend, setSpend] = useState<TenantSpend | null>(null);
  const [accountActions, setAccountActions] = useState<{
    recharge: boolean;
    refund: boolean;
  }>({ recharge: false, refund: false });

  const [txLoading, setTxLoading] = useState(false);
  const [transactions, setTransactions] = useState<Transaction[]>([]);
  const [txTotal, setTxTotal] = useState(0);
  const [txPage, setTxPage] = useState(1);
  const [txPageSize, setTxPageSize] = useState(20);
  const [txFilter, setTxFilter] = useState<TxFilterType>("");

  // Recharge modal
  const [rechargeOpen, setRechargeOpen] = useState(false);
  const [recharging, setRecharging] = useState(false);
  const [rechargeAmountStr, setRechargeAmountStr] = useState("100");
  const [rechargeReasonCode, setRechargeReasonCode] = useState("other");
  const [rechargeDesc, setRechargeDesc] = useState("");
  const [rechargeErrors, setRechargeErrors] = useState<{
    amount?: string;
    description?: string;
  }>({});

  // Refund modal
  const [refundOpen, setRefundOpen] = useState(false);
  const [refunding, setRefunding] = useState(false);
  const [refundAmountStr, setRefundAmountStr] = useState("0");
  const [refundReasonCode, setRefundReasonCode] = useState("other");
  const [refundReason, setRefundReason] = useState("");
  const [refundErrors, setRefundErrors] = useState<{
    amount?: string;
    reason?: string;
  }>({});

  const accountAbortRef = useRef<AbortController | null>(null);
  const txAbortRef = useRef<AbortController | null>(null);

  const loadAccount = useCallback(async () => {
    if (!tenantCode) return;
    accountAbortRef.current?.abort();
    const controller = new AbortController();
    accountAbortRef.current = controller;
    setAccountLoading(true);
    try {
      const range = last30DayRangeUtc8();
      const [acc, sp] = await Promise.all([
        fetchTenantAccount(tenantCode, controller.signal),
        fetchTenantSpend(tenantCode, range, controller.signal),
      ]);
      if (controller.signal.aborted) return;
      setAccount(acc);
      setSpend(sp);
      if (acc.actions) {
        setAccountActions(acc.actions);
      }
    } catch (error) {
      if (!controller.signal.aborted && !isMfaStepUpCancelled(error)) {
        logger.error("Failed to load tenant account:", error);
      }
    } finally {
      if (!controller.signal.aborted) {
        setAccountLoading(false);
      }
    }
  }, [tenantCode]);

  const loadTransactions = useCallback(async () => {
    if (!tenantCode) return;
    txAbortRef.current?.abort();
    const controller = new AbortController();
    txAbortRef.current = controller;
    setTxLoading(true);
    try {
      const params: TransactionListQuery = {
        page: txPage,
        page_size: txPageSize,
        transaction_type: txFilter || undefined,
      };
      const res = await getTenantTransactions(
        tenantCode,
        params,
        controller.signal,
      );
      if (controller.signal.aborted) return;
      setTransactions(res.transactions ?? []);
      setTxTotal(res.pagination?.total ?? 0);
    } catch (error) {
      if (!controller.signal.aborted && !isMfaStepUpCancelled(error)) {
        logger.error("Failed to load transactions:", error);
      }
    } finally {
      if (!controller.signal.aborted) {
        setTxLoading(false);
      }
    }
  }, [tenantCode, txPage, txPageSize, txFilter]);

  useEffect(() => {
    void loadAccount();
    return () => accountAbortRef.current?.abort();
  }, [loadAccount]);

  useEffect(() => {
    void loadTransactions();
    return () => txAbortRef.current?.abort();
  }, [loadTransactions]);

  const reasonCodeOptions = useMemo<SelectOptionItem[]>(
    () => [
      {
        value: "compensation",
        label: t("tenant.account.reasonCodes.compensation"),
        text: t("tenant.account.reasonCodes.compensation"),
      },
      {
        value: "correction",
        label: t("tenant.account.reasonCodes.correction"),
        text: t("tenant.account.reasonCodes.correction"),
      },
      {
        value: "commercial",
        label: t("tenant.account.reasonCodes.commercial"),
        text: t("tenant.account.reasonCodes.commercial"),
      },
      {
        value: "other",
        label: t("tenant.account.reasonCodes.other"),
        text: t("tenant.account.reasonCodes.other"),
      },
    ],
    [t],
  );

  const composeReasonText = (code: string, freeText: string): string => {
    const label =
      reasonCodeOptions.find((item) => item.value === code)?.text ?? code;
    const note = freeText.trim();
    return note ? `[${label}] ${note}` : `[${label}]`;
  };

  // U2: Strict integer-cent balance computation without floating-point drift
  const maxRefundCents = useMemo(() => {
    const raw = account?.balance;
    if (!raw) return 0;
    const trimmed = raw.trim();
    if (trimmed.startsWith("-")) return 0;
    // Truncate beyond 2 decimal places safely via string parsing if micro-yuan precision exists
    const match = /^(\d+)(?:\.(\d+))?$/.exec(trimmed);
    if (!match) return 0;
    const intPart = match[1];
    const frac2 = (match[2] ?? "").slice(0, 2).padEnd(2, "0");
    return yuanToCentsExact(`${intPart}.${frac2}`) ?? 0;
  }, [account?.balance]);

  const refundCents = useMemo(
    () => yuanToCentsExact(refundAmountStr),
    [refundAmountStr],
  );

  const refundExceedsBalance =
    refundCents !== null && refundCents > maxRefundCents;

  const balanceAfterRefund = useMemo(() => {
    if (refundCents === null) return account?.balance || "0";
    return centsToYuanString(Math.max(0, maxRefundCents - refundCents));
  }, [refundCents, maxRefundCents, account?.balance]);

  const handleOpenRecharge = () => {
    setRechargeAmountStr("100");
    setRechargeReasonCode("other");
    setRechargeDesc("");
    setRechargeErrors({});
    setRechargeOpen(true);
  };

  const handleSubmitRecharge = async () => {
    if (recharging) return;
    const nextErrors: { amount?: string; description?: string } = {};
    const cents = yuanToCentsExact(rechargeAmountStr);
    if (!rechargeAmountStr.trim()) {
      nextErrors.amount = t("tenant.account.validation.rechargeAmountRequired");
    } else if (cents === null || cents <= 0) {
      nextErrors.amount = t("tenant.account.validation.rechargeAmountPositive");
    }
    if (!rechargeDesc.trim()) {
      nextErrors.description = t(
        "tenant.account.validation.rechargeDescriptionRequired",
      );
    }
    setRechargeErrors(nextErrors);
    if (Object.keys(nextErrors).length > 0 || cents === null) return;

    setRecharging(true);
    try {
      const payload: RechargeRequest = {
        amount: cents,
        description: composeReasonText(rechargeReasonCode, rechargeDesc),
      };
      await rechargeTenantAccount(tenantCode, payload);
      showSuccessToast(t("common.feedback.rechargeSuccess"));
      setRechargeOpen(false);
      setTxPage(1);
      setTxFilter("credit");
      void loadAccount();
      void loadTransactions();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to recharge:", error);
      }
    } finally {
      setRecharging(false);
    }
  };

  const handleOpenRefund = () => {
    const defaultCents = Math.min(10000, maxRefundCents);
    setRefundAmountStr(defaultCents > 0 ? centsToYuanString(defaultCents) : "");
    setRefundReasonCode("other");
    setRefundReason("");
    setRefundErrors({});
    setRefundOpen(true);
  };

  const handleSubmitRefund = async () => {
    if (refunding) return;
    const nextErrors: { amount?: string; reason?: string } = {};
    const cents = yuanToCentsExact(refundAmountStr);
    if (!refundAmountStr.trim()) {
      nextErrors.amount = t("tenant.account.validation.refundAmountRequired");
    } else if (cents === null || cents <= 0) {
      nextErrors.amount = t("tenant.account.validation.refundAmountPositive");
    } else if (cents > maxRefundCents) {
      nextErrors.amount = t("tenant.messages.refundExceedsBalance");
      showErrorToast(t("tenant.messages.refundExceedsBalance"));
    }
    if (!refundReason.trim()) {
      nextErrors.reason = t("tenant.account.validation.refundReasonRequired");
    }
    setRefundErrors(nextErrors);
    if (Object.keys(nextErrors).length > 0 || cents === null) return;

    setRefunding(true);
    try {
      const payload: RefundRequest = {
        amount: cents,
        reason: composeReasonText(refundReasonCode, refundReason),
      };
      await refundTenantAccount(tenantCode, payload);
      showSuccessToast(t("common.feedback.refundSuccess"));
      setRefundOpen(false);
      setTxPage(1);
      setTxFilter("refund");
      void loadAccount();
      void loadTransactions();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to refund:", error);
      }
    } finally {
      setRefunding(false);
    }
  };

  const getTransactionTypeLabel = useCallback(
    (type: string) => {
      const labels: Record<string, string> = {
        credit: t("tenant.account.transactionTypes.credit"),
        refund: t("tenant.account.transactionTypes.refund"),
        charge: t("tenant.account.transactionTypes.charge"),
        adjustment: t("tenant.account.transactionTypes.adjustment"),
        payment: t("tenant.account.transactionTypes.payment"),
      };
      return labels[type] ?? type;
    },
    [t],
  );

  const getPaymentMethodLabel = useCallback(
    (method?: string) => {
      if (!method) return EMPTY_PLACEHOLDER;
      const labels: Record<string, string> = {
        manual: t("tenant.account.paymentMethods.manual"),
        bank_transfer: t("tenant.account.paymentMethods.bankTransfer"),
        alipay: t("tenant.account.paymentMethods.alipay"),
        wechat_pay: t("tenant.account.paymentMethods.wechatPay"),
        credit_card: t("tenant.account.paymentMethods.creditCard"),
        refund: t("tenant.account.paymentMethods.refund"),
        credit: t("tenant.account.paymentMethods.credit"),
        adjustment: t("tenant.account.paymentMethods.adjustment"),
      };
      return labels[method] ?? method;
    },
    [t],
  );

  const filterOptions = useMemo<SelectOptionItem[]>(
    () => [
      {
        value: "",
        label: t("tenant.account.filters.all"),
        text: t("tenant.account.filters.all"),
      },
      {
        value: "credit",
        label: t("tenant.account.filters.credit"),
        text: t("tenant.account.filters.credit"),
      },
      {
        value: "refund",
        label: t("tenant.account.filters.refund"),
        text: t("tenant.account.filters.refund"),
      },
      {
        value: "charge",
        label: t("tenant.account.filters.charge"),
        text: t("tenant.account.filters.charge"),
      },
      {
        value: "adjustment",
        label: t("tenant.account.filters.adjustment"),
        text: t("tenant.account.filters.adjustment"),
      },
    ],
    [t],
  );

  const txColumns = useMemo<DataTableColumn<Transaction>[]>(
    () => [
      {
        id: "transaction_type",
        header: t("tenant.account.columns.type"),
        width: 96,
        cell: (row) => (
          <Badge variant="outline">
            {getTransactionTypeLabel(row.transaction_type)}
          </Badge>
        ),
      },
      {
        id: "amount",
        header: t("tenant.account.columns.amount"),
        width: 132,
        align: "right",
        mono: true,
        cell: (row) => {
          const isNegative =
            row.transaction_type === "charge" ||
            row.transaction_type === "refund";
          return (
            <span
              className={cn(
                "font-semibold tabular-nums",
                isNegative ? "text-danger-fg" : "text-success-fg",
              )}
            >
              {isNegative ? "-" : "+"}
              {formatMoney(row.amount)}
            </span>
          );
        },
      },
      {
        id: "balance_change",
        header: t("tenant.account.columns.balanceChange"),
        minWidth: 184,
        align: "right",
        mono: true,
        cell: (row) => (
          <span className="tabular-nums text-foreground">
            {formatMoney(row.balance_before)}
            <span className="mx-1.5 text-muted-foreground">→</span>
            {formatMoney(row.balance_after)}
          </span>
        ),
      },
      {
        id: "description",
        header: t("tenant.account.columns.description"),
        minWidth: 140,
        cell: (row) => row.description || EMPTY_PLACEHOLDER,
      },
      {
        id: "payment_method",
        header: t("tenant.account.columns.paymentMethod"),
        width: 108,
        cell: (row) => getPaymentMethodLabel(row.payment_method),
      },
      {
        id: "operator_name",
        header: t("tenant.account.columns.operator"),
        width: 116,
        cell: (row) => row.operator_name || EMPTY_PLACEHOLDER,
      },
      {
        id: "created_at",
        header: t("common.fields.createdAt"),
        width: 168,
        mono: true,
        cell: (row) => formatDateTime(row.created_at),
      },
    ],
    [getPaymentMethodLabel, getTransactionTypeLabel, t],
  );

  return (
    <div className="space-y-4 min-w-0">
      <EntityDetailCard
        title={t("tenant.account.overviewTitle")}
        icon={<Wallet className="h-4 w-4" />}
        testId="tenant-account-overview-card"
        actions={
          canWrite && (accountActions.recharge || accountActions.refund) ? (
            <>
              {accountActions.recharge && (
                <Button
                  type="button"
                  variant="primary"
                  size="sm"
                  onClick={handleOpenRecharge}
                  data-testid="tenant-account-recharge-btn"
                >
                  <CirclePlus className="h-3.5 w-3.5" />
                  {t("tenant.account.recharge")}
                </Button>
              )}
              {accountActions.refund && (
                <Button
                  type="button"
                  variant="warning"
                  size="sm"
                  onClick={handleOpenRefund}
                  data-testid="tenant-account-refund-btn"
                >
                  <RotateCcw className="h-3.5 w-3.5" />
                  {t("tenant.account.refund")}
                </Button>
              )}
            </>
          ) : undefined
        }
      >
        <div
          className={cn(
            "grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4",
            accountLoading && "opacity-60 transition-opacity",
          )}
        >
          <div className="rounded-md border border-border-subtle bg-muted/25 p-3.5 space-y-1">
            <div className="text-xs text-muted-foreground">
              {t("tenant.account.balanceLabel")}
            </div>
            <div className="flex items-baseline gap-1.5 font-mono-tech tabular-nums">
              <span className="text-xs font-medium text-muted-foreground">
                {account?.currency || "CNY"}
              </span>
              <span className="text-xl font-semibold text-foreground">
                {formatMoney(account?.balance ?? "0")}
              </span>
            </div>
          </div>

          <div className="rounded-md border border-border-subtle bg-muted/25 p-3.5 space-y-1">
            <div className="text-xs text-muted-foreground">
              {t("tenant.account.creditAndAvailable")}
            </div>
            <div className="text-base font-semibold font-mono-tech tabular-nums text-foreground">
              {formatMoney(account?.credit_limit ?? "0")} /{" "}
              {formatMoney(account?.available_balance ?? "0")}
            </div>
          </div>

          <div className="rounded-md border border-border-subtle bg-muted/25 p-3.5 space-y-1.5">
            <div className="text-xs text-muted-foreground">
              {t("tenant.account.statusLabel")}
            </div>
            <div>
              <StatusBadge
                status={account?.status === "active" ? "ACTIVE" : "FROZEN"}
                label={
                  account?.status === "active"
                    ? t("tenant.account.status.active")
                    : t("tenant.account.status.frozen")
                }
              />
            </div>
          </div>

          <div className="rounded-md border border-border-subtle bg-muted/25 p-3.5 space-y-1">
            <div className="text-xs text-muted-foreground">
              {t("tenant.account.windowSpend")}
            </div>
            <div className="text-base font-semibold font-mono-tech tabular-nums text-foreground">
              {spend ? formatMoney(spend.tenant_spend) : EMPTY_PLACEHOLDER}
            </div>
            {spend?.window?.label && (
              <div className="text-[11px] text-muted-foreground">
                {spend.window.label}
              </div>
            )}
          </div>
        </div>
      </EntityDetailCard>

      <DataTablePage<Transaction>
        testId="tenant-transactions-table"
        columns={txColumns}
        data={transactions}
        getRowId={(row) => row.id}
        loading={txLoading}
        emptyText={t("common.feedback.noData")}
        page={txPage}
        pageSize={txPageSize}
        total={txTotal}
        pageSizeOptions={[10, 20, 50]}
        onPageChange={setTxPage}
        onPageSizeChange={(nextSize) => {
          setTxPageSize(nextSize);
          setTxPage(1);
        }}
        filters={
          <div className="flex flex-wrap items-center gap-2.5">
            <span className="text-xs font-semibold text-foreground">
              {t("tenant.account.transactionsTitle")}
            </span>
            <Select
              value={txFilter}
              options={filterOptions}
              placeholder={t("tenant.account.filterPlaceholder")}
              onChange={(e) => {
                setTxFilter(e.target.value as TxFilterType);
                setTxPage(1);
              }}
              className="w-36"
            />
          </div>
        }
        actions={
          <Button
            type="button"
            variant="outline"
            size="md"
            onClick={() => {
              void loadAccount();
              void loadTransactions();
            }}
            disabled={txLoading || accountLoading}
          >
            <RefreshCw
              className={
                txLoading || accountLoading
                  ? "h-3.5 w-3.5 animate-spin"
                  : "h-3.5 w-3.5"
              }
            />
            {t("common.actions.refresh")}
          </Button>
        }
      />

      {/* Recharge Modal (FormModal size="sm" = 480px, label-top, U2 exact cents) */}
      <FormModal
        open={rechargeOpen}
        onClose={() => setRechargeOpen(false)}
        onSubmit={handleSubmitRecharge}
        title={t("tenant.account.rechargeDialogTitle")}
        size="sm"
        submitLabel={t("tenant.account.rechargeConfirm")}
        submitting={recharging}
        testId="tenant-recharge-modal"
      >
        <div className="flex items-center justify-between rounded-md border border-border-subtle bg-muted/35 px-3 py-2 text-xs">
          <span className="text-muted-foreground">
            {t("tenant.account.currentBalance")}
          </span>
          <span className="font-mono-tech font-semibold tabular-nums text-foreground">
            {account?.currency || "CNY"} {formatMoney(account?.balance ?? "0")}
          </span>
        </div>

        <FormField
          label={t("tenant.account.rechargeAmount")}
          htmlFor="recharge-amount-input"
          required
          error={rechargeErrors.amount}
        >
          <div className="flex items-center gap-2">
            <Input
              id="recharge-amount-input"
              type="text"
              inputMode="decimal"
              value={rechargeAmountStr}
              onChange={(e) => {
                setRechargeAmountStr(e.target.value);
                if (rechargeErrors.amount) {
                  setRechargeErrors((prev) => ({ ...prev, amount: undefined }));
                }
              }}
              placeholder={t("tenant.account.rechargeAmountPlaceholder")}
              className="font-mono-tech tabular-nums"
              data-testid="recharge-amount-input"
            />
            <span className="shrink-0 whitespace-nowrap text-xs text-muted-foreground">
              {t("common.currencies.cny")}
            </span>
          </div>
        </FormField>

        <FormField label={t("tenant.account.reasonCode")} required>
          <Select
            value={rechargeReasonCode}
            options={reasonCodeOptions}
            placeholder={t("tenant.account.reasonCodePlaceholder")}
            onChange={(e) => setRechargeReasonCode(e.target.value)}
            className="w-full"
          />
        </FormField>

        <FormField
          label={t("tenant.account.description")}
          htmlFor="recharge-desc-input"
          required
          error={rechargeErrors.description}
        >
          <Input
            id="recharge-desc-input"
            value={rechargeDesc}
            onChange={(e) => {
              setRechargeDesc(e.target.value);
              if (rechargeErrors.description) {
                setRechargeErrors((prev) => ({
                  ...prev,
                  description: undefined,
                }));
              }
            }}
            placeholder={t("tenant.account.descriptionPlaceholder")}
            data-testid="recharge-desc-input"
          />
        </FormField>

        <MfaRequiredHint />
      </FormModal>

      {/* Refund Modal (FormModal size="sm" = 480px, label-top, U2 exact cents) */}
      <FormModal
        open={refundOpen}
        onClose={() => setRefundOpen(false)}
        onSubmit={handleSubmitRefund}
        title={t("tenant.account.refundDialogTitle")}
        size="sm"
        submitLabel={t("tenant.account.refundConfirm")}
        submitting={refunding}
        submitDisabled={
          refundCents === null ||
          refundCents <= 0 ||
          refundCents > maxRefundCents
        }
        testId="tenant-refund-modal"
      >
        <div className="flex items-center justify-between rounded-md border border-border-subtle bg-muted/35 px-3 py-2 text-xs">
          <span className="text-muted-foreground">
            {t("tenant.account.currentBalance")}
          </span>
          <span className="font-mono-tech font-semibold tabular-nums text-foreground">
            {account?.currency || "CNY"} {formatMoney(account?.balance ?? "0")}
          </span>
        </div>

        <FormField
          label={t("tenant.account.refundAmount")}
          htmlFor="refund-amount-input"
          required
          error={refundErrors.amount}
        >
          <div className="flex items-center gap-2">
            <Input
              id="refund-amount-input"
              type="text"
              inputMode="decimal"
              value={refundAmountStr}
              onChange={(e) => {
                setRefundAmountStr(e.target.value);
                if (refundErrors.amount) {
                  setRefundErrors((prev) => ({ ...prev, amount: undefined }));
                }
              }}
              placeholder={t("tenant.account.refundAmountPlaceholder")}
              className="font-mono-tech tabular-nums"
              data-testid="refund-amount-input"
            />
            <span className="shrink-0 whitespace-nowrap text-xs text-muted-foreground">
              {t("common.currencies.cny")}
            </span>
          </div>
        </FormField>

        <FormField label={t("tenant.account.reasonCode")} required>
          <Select
            value={refundReasonCode}
            options={reasonCodeOptions}
            placeholder={t("tenant.account.reasonCodePlaceholder")}
            onChange={(e) => setRefundReasonCode(e.target.value)}
            className="w-full"
          />
        </FormField>

        <FormField
          label={t("tenant.account.refundReason")}
          htmlFor="refund-reason-input"
          required
          error={refundErrors.reason}
        >
          <Textarea
            id="refund-reason-input"
            rows={3}
            maxLength={500}
            value={refundReason}
            onChange={(e) => {
              setRefundReason(e.target.value);
              if (refundErrors.reason) {
                setRefundErrors((prev) => ({ ...prev, reason: undefined }));
              }
            }}
            placeholder={t("tenant.account.refundReasonPlaceholder")}
            data-testid="refund-reason-input"
          />
        </FormField>

        <div
          className={cn(
            "flex items-center justify-between rounded-md border px-3 py-2 text-xs",
            refundExceedsBalance
              ? "border-warning-border bg-warning-bg text-warning-fg"
              : "border-border-subtle bg-muted/35 text-foreground",
          )}
        >
          <span
            className={
              refundExceedsBalance ? "text-warning-fg" : "text-muted-foreground"
            }
          >
            {t("tenant.account.balanceAfterRefund")}
          </span>
          <span className="font-mono-tech font-semibold tabular-nums">
            {account?.currency || "CNY"} {formatMoney(balanceAfterRefund)}
          </span>
        </div>

        {refundExceedsBalance && (
          <div className="flex items-start gap-2 rounded-md border border-warning-border bg-warning-bg px-3 py-2 text-xs text-warning-fg">
            <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" />
            <span>{t("tenant.account.refundExceedsBalance")}</span>
          </div>
        )}

        <MfaRequiredHint />
      </FormModal>
    </div>
  );
}
