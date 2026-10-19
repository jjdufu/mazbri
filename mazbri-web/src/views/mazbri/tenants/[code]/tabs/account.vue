<template>
  <div class="account-tab">
    <ElCard
      v-loading="loading"
      shadow="never"
      class="account-overview-card surface-panel-card"
    >
      <template #header>
        <SurfaceCardHeader>
          <template #left>
            <div class="account-overview-header">
              <span>{{ $t("tenant.account.overviewTitle") }}</span>
              <span class="account-overview-subtitle">{{
                $t("tenant.account.overviewSubtitle")
              }}</span>
            </div>
          </template>
        </SurfaceCardHeader>
      </template>

      <div class="account-overview-grid">
        <div class="balance-content">
          <div class="balance-label">
            {{ $t("tenant.account.balanceLabel") }}
          </div>
          <div class="balance-amount">
            <span class="currency">{{ account?.currency || "CNY" }}</span>
            <span class="amount">{{
              formatBalance(account?.balance || "0")
            }}</span>
          </div>
          <div class="balance-meta">
            {{
              account?.status === "active"
                ? $t("tenant.account.status.active")
                : $t("tenant.account.status.frozen")
            }}
            ·
            {{ account?.currency || "CNY" }}
          </div>
        </div>

        <CommonActionPanel
          surface
          align="start"
          :title="$t('tenant.account.quickActionsTitle')"
        >
          <template #actions>
            <ElButton
              v-if="accountActions.recharge"
              v-roles="['R_ADMIN', 'R_OPERATOR', 'R_FINANCE']"
              type="primary"
              @click="handleRecharge"
            >
              <template #icon
                ><ArtSvgIcon icon="ri:add-circle-line"
              /></template>
              {{ $t("tenant.account.recharge") }}
            </ElButton>
            <ElButton
              v-if="accountActions.refund"
              v-roles="['R_ADMIN', 'R_OPERATOR', 'R_FINANCE']"
              type="warning"
              @click="handleRefund"
            >
              <template #icon><ArtSvgIcon icon="ri:refund-2-line" /></template>
              {{ $t("tenant.account.refund") }}
            </ElButton>
          </template>
        </CommonActionPanel>
      </div>
    </ElCard>

    <ResourceTableShell
      v-model:columns="transactionColumns"
      :loading="transactionsLoading"
      :data="transactions"
      :pagination="artPagination"
      :show-toolbar="false"
      table-class="surface-panel-table transactions-table"
      stripe
      border
      @pagination:size-change="handlePageSizeChange"
      @pagination:current-change="handleCurrentPageChange"
    >
      <template #header>
        <SurfaceCardHeader class="transactions-card-header">
          <template #left>
            <div class="transactions-card-title">
              <span>{{ $t("tenant.account.transactionsTitle") }}</span>
              <span class="transactions-card-subtitle">{{
                $t("tenant.account.transactionsSubtitle")
              }}</span>
            </div>
          </template>
          <template #actions>
            <ElSelect
              v-model="transactionFilter"
              :placeholder="$t('tenant.account.filterPlaceholder')"
              clearable
              size="small"
              class="transactions-filter"
              @change="handleFilterChange"
            >
              <ElOption :label="$t('tenant.account.filters.all')" value="" />
              <ElOption
                :label="$t('tenant.account.filters.credit')"
                value="credit"
              >
                <span
                  ><ArtSvgIcon icon="ri:money-dollar-circle-line" />
                  {{ $t("tenant.account.filters.credit") }}</span
                >
              </ElOption>
              <ElOption
                :label="$t('tenant.account.filters.refund')"
                value="refund"
              >
                <span
                  ><ArtSvgIcon icon="ri:refund-2-line" />
                  {{ $t("tenant.account.filters.refund") }}</span
                >
              </ElOption>
              <ElOption
                :label="$t('tenant.account.filters.charge')"
                value="charge"
              />
              <ElOption
                :label="$t('tenant.account.filters.adjustment')"
                value="adjustment"
              />
            </ElSelect>
          </template>
        </SurfaceCardHeader>
      </template>

      <template #transaction_type="{ row }">
        <ElTag type="info" effect="plain" size="small" class="transaction-tag">
          {{ getTransactionTypeLabel(row.transaction_type) }}
        </ElTag>
      </template>
      <template #amount="{ row }">
        <span :class="['amount', getAmountClass(row.transaction_type)]">
          {{ getAmountPrefix(row.transaction_type)
          }}{{ formatAmount(row.amount) }}
        </span>
      </template>
      <template #balance_change="{ row }">
        <span class="balance-change">
          {{ formatBalance(row.balance_before) }}
          <span class="balance-change-separator">/</span>
          {{ formatBalance(row.balance_after) }}
        </span>
      </template>
      <template #description="{ row }">
        {{ row.description || "-" }}
      </template>
      <template #payment_method="{ row }">
        {{ getPaymentMethodLabel(row.payment_method) }}
      </template>
      <template #operator_name="{ row }">
        {{ row.operator_name || "-" }}
      </template>
      <template #created_at="{ row }">
        {{ formatDateTime(row.created_at) }}
      </template>
    </ResourceTableShell>

    <EditDialog
      v-model="rechargeVisible"
      :title="$t('tenant.account.rechargeDialogTitle')"
      width="450px"
      dialog-class="account-action-dialog"
      :confirm-text="$t('tenant.account.rechargeConfirm')"
      :saving="recharging"
      @confirm="handleSubmitRecharge"
    >
      <ElForm
        ref="rechargeFormRef"
        :model="rechargeForm"
        :rules="rechargeRules"
        label-width="92px"
        class="compact-form"
      >
        <div class="dialog-balance-strip">
          <span class="dialog-balance-label">{{
            $t("tenant.account.currentBalance")
          }}</span>
          <span class="dialog-balance-value current-balance">
            {{ account?.currency || "CNY" }}
            {{ formatBalance(account?.balance || "0") }}
          </span>
        </div>
        <ElFormItem :label="$t('tenant.account.rechargeAmount')" prop="amount">
          <ElInput
            v-model.number="rechargeForm.amount"
            type="number"
            :min="1"
            :placeholder="$t('tenant.account.rechargeAmountPlaceholder')"
            class="amount-input"
            style="width: 100%"
          >
            <template #suffix>
              <CommonInlineHint size="sm">{{
                $t("common.currencies.cny")
              }}</CommonInlineHint>
            </template>
          </ElInput>
        </ElFormItem>
        <ElFormItem
          :label="$t('tenant.account.reasonCode')"
          prop="reason_code"
        >
          <ElSelect
            v-model="rechargeReasonCode"
            style="width: 100%"
            :placeholder="$t('tenant.account.reasonCodePlaceholder')"
          >
            <ElOption
              v-for="opt in reasonCodeOptions"
              :key="opt.value"
              :label="opt.label"
              :value="opt.value"
            />
          </ElSelect>
        </ElFormItem>
        <ElFormItem
          :label="$t('tenant.account.description')"
          prop="description"
        >
          <ElInput
            v-model="rechargeForm.description"
            :placeholder="$t('tenant.account.descriptionPlaceholder')"
            class="text-input"
          />
        </ElFormItem>
      </ElForm>
    </EditDialog>

    <EditDialog
      v-model="refundVisible"
      :title="$t('tenant.account.refundDialogTitle')"
      width="450px"
      dialog-class="account-action-dialog"
      :confirm-text="$t('tenant.account.refundConfirm')"
      :confirm-disabled="refundForm.amount > currentBalance"
      :saving="refunding"
      @confirm="handleSubmitRefund"
    >
      <ElForm
        ref="refundFormRef"
        :model="refundForm"
        :rules="refundRules"
        label-width="92px"
        class="compact-form"
      >
        <div class="dialog-balance-strip">
          <span class="dialog-balance-label">{{
            $t("tenant.account.currentBalance")
          }}</span>
          <span class="dialog-balance-value current-balance">
            {{ account?.currency || "CNY" }}
            {{ formatBalance(account?.balance || "0") }}
          </span>
        </div>
        <ElFormItem :label="$t('tenant.account.refundAmount')" prop="amount">
          <ElInput
            v-model.number="refundForm.amount"
            type="number"
            :min="1"
            :max="maxRefundAmount"
            :placeholder="$t('tenant.account.refundAmountPlaceholder')"
            class="amount-input"
            style="width: 100%"
          >
            <template #suffix>
              <CommonInlineHint size="sm">{{
                $t("common.currencies.cny")
              }}</CommonInlineHint>
            </template>
          </ElInput>
        </ElFormItem>
        <ElFormItem
          :label="$t('tenant.account.reasonCode')"
          prop="reason_code"
        >
          <ElSelect
            v-model="refundReasonCode"
            style="width: 100%"
            :placeholder="$t('tenant.account.reasonCodePlaceholder')"
          >
            <ElOption
              v-for="opt in reasonCodeOptions"
              :key="opt.value"
              :label="opt.label"
              :value="opt.value"
            />
          </ElSelect>
        </ElFormItem>
        <ElFormItem :label="$t('tenant.account.refundReason')" prop="reason">
          <ElInput
            v-model="refundForm.reason"
            type="textarea"
            :rows="3"
            maxlength="500"
            show-word-limit
            :placeholder="$t('tenant.account.refundReasonPlaceholder')"
            class="text-input"
          />
        </ElFormItem>
        <div
          :class="[
            'dialog-preview-strip',
            refundForm.amount > currentBalance ? 'warning' : '',
          ]"
        >
          <span class="dialog-balance-label">{{
            $t("tenant.account.balanceAfterRefund")
          }}</span>
          <span class="dialog-balance-value balance-preview">
            {{ account?.currency || "CNY" }}
            {{ formatBalance(balanceAfterRefund) }}
          </span>
        </div>
        <CommonInlineHint
          v-if="refundForm.amount > currentBalance"
          block
          tone="warning"
          class="-mt-1.5 mb-3.5"
        >
          {{ $t("tenant.account.refundExceedsBalance") }}
        </CommonInlineHint>
      </ElForm>
    </EditDialog>
  </div>
</template>

<script setup lang="ts">
import {
  ElMessage,
  ElMessageBox,
  type FormInstance,
  type FormRules,
} from "element-plus";
import { $t } from "@/locales";
import type {
  TenantAccount,
  RechargeRequest,
  RefundRequest,
  Transaction,
  TransactionListQuery,
} from "@/types/api/tenant";
import {
  fetchTenantAccount,
  rechargeTenantAccount,
  refundTenantAccount,
  getTenantTransactions,
} from "@/api/tenant";
import EditDialog from "@/components/common/EditDialog.vue";
import CommonActionPanel from "@/components/common/action-panel/index.vue";
import CommonInlineHint from "@/components/common/inline-hint/index.vue";
import ResourceTableShell from "@/components/common/resource-table-shell/index.vue";
import SurfaceCardHeader from "@/components/common/surface-card-header/index.vue";
import { isHttpError } from "@/utils/http/error";
import { logger } from "@/utils/sys";
import type { ColumnOption, PaginationConfig } from "@/types/component";
import { formatAmount, formatDateTime } from "@/utils/format";
defineOptions({ name: "AccountTab" });
const props = defineProps<{
  tenantCode: string;
}>();
const loading = ref(false);
const recharging = ref(false);
const refunding = ref(false);
const transactionsLoading = ref(false);
const account = ref<TenantAccount | null>(null);
const accountActions = ref<{
  recharge: boolean;
  refund: boolean;
}>({
  recharge: true,
  refund: true,
});
const transactions = ref<Transaction[]>([]);
const transactionsTotal = ref(0);
const transactionFilter = ref("");
const pagination = reactive({
  page: 1,
  limit: 20,
});
const transactionColumns = ref<ColumnOption<Transaction>[]>([
  {
    prop: "transaction_type",
    label: $t("tenant.account.columns.type"),
    width: 86,
    useSlot: true,
  },
  {
    prop: "amount",
    label: $t("tenant.account.columns.amount"),
    width: 120,
    useSlot: true,
  },
  {
    prop: "balance_change",
    label: $t("tenant.account.columns.balanceChange"),
    minWidth: 120,
    useSlot: true,
  },
  {
    prop: "description",
    label: $t("tenant.account.columns.description"),
    minWidth: 112,
    useSlot: true,
  },
  {
    prop: "payment_method",
    label: $t("tenant.account.columns.paymentMethod"),
    width: 92,
    useSlot: true,
  },
  {
    prop: "operator_name",
    label: $t("tenant.account.columns.operator"),
    width: 108,
    useSlot: true,
  },
  {
    prop: "created_at",
    label: $t("common.fields.createdAt"),
    width: 160,
    useSlot: true,
  },
]);
const artPagination = computed<PaginationConfig>(() => ({
  current: pagination.page,
  size: pagination.limit,
  total: transactionsTotal.value,
}));
const reasonCodeOptions = [
  {
    value: "compensation",
    label: $t("tenant.account.reasonCodes.compensation"),
  },
  {
    value: "correction",
    label: $t("tenant.account.reasonCodes.correction"),
  },
  {
    value: "commercial",
    label: $t("tenant.account.reasonCodes.commercial"),
  },
  {
    value: "other",
    label: $t("tenant.account.reasonCodes.other"),
  },
];
const rechargeReasonCode = ref("other");
const refundReasonCode = ref("other");
const composeReasonText = (code: string, freeText: string) => {
  const label =
    reasonCodeOptions.find((o) => o.value === code)?.label || code;
  const note = freeText.trim();
  return note ? `[${label}] ${note}` : `[${label}]`;
};
const rechargeVisible = ref(false);
const rechargeFormRef = ref<FormInstance>();
const rechargeForm = reactive<RechargeRequest>({
  amount: 0,
  description: "",
});
const rechargeRules: FormRules = {
  amount: [
    {
      required: true,
      message: $t("tenant.account.validation.rechargeAmountRequired"),
      trigger: "blur",
    },
    {
      type: "number",
      min: 1,
      message: $t("tenant.account.validation.rechargeAmountPositive"),
      trigger: "blur",
    },
  ],
  description: [
    {
      required: true,
      message: $t("tenant.account.validation.rechargeDescriptionRequired"),
      trigger: "blur",
    },
  ],
};
const refundVisible = ref(false);
const refundFormRef = ref<FormInstance>();
const refundForm = reactive<RefundRequest>({
  amount: 0,
  reason: "",
});
const refundRules: FormRules = {
  amount: [
    {
      required: true,
      message: $t("tenant.account.validation.refundAmountRequired"),
      trigger: "blur",
    },
    {
      type: "number",
      min: 1,
      message: $t("tenant.account.validation.refundAmountPositive"),
      trigger: "blur",
    },
  ],
  reason: [
    {
      required: true,
      message: $t("tenant.account.validation.refundReasonRequired"),
      trigger: "blur",
    },
  ],
};
const currentBalance = computed(() => {
  return parseFloat(account.value?.balance || "0");
});
const maxRefundAmount = computed(() => {
  return Math.floor(currentBalance.value * 100);
});
const balanceAfterRefund = computed(() => {
  const refundAmount = refundForm.amount / 100;
  return Math.max(0, currentBalance.value - refundAmount);
});

const formatBalance = (balance: string | number | null | undefined) => {
  return formatAmount(balance);
};

const getTransactionTypeLabel = (type: string) => {
  const labels: Record<string, string> = {
    credit: $t("tenant.account.transactionTypes.credit"),
    refund: $t("tenant.account.transactionTypes.refund"),
    charge: $t("tenant.account.transactionTypes.charge"),
    adjustment: $t("tenant.account.transactionTypes.adjustment"),
    payment: $t("tenant.account.transactionTypes.payment"),
  };
  return labels[type] || type;
};
const getAmountPrefix = (type: string) => {
  return type === "charge" || type === "refund" ? "-" : "+";
};
const getAmountClass = (type: string) => {
  return type === "charge" || type === "refund" ? "negative" : "positive";
};
const getPaymentMethodLabel = (method?: string) => {
  if (!method) return "-";
  const labels: Record<string, string> = {
    manual: $t("tenant.account.paymentMethods.manual"),
    bank_transfer: $t("tenant.account.paymentMethods.bankTransfer"),
    alipay: $t("tenant.account.paymentMethods.alipay"),
    wechat_pay: $t("tenant.account.paymentMethods.wechatPay"),
    credit_card: $t("tenant.account.paymentMethods.creditCard"),
    refund: $t("tenant.account.paymentMethods.refund"),
    credit: $t("tenant.account.paymentMethods.credit"),
    adjustment: $t("tenant.account.paymentMethods.adjustment"),
  };
  return labels[method] || method;
};
const loadAccount = async () => {
  loading.value = true;
  try {
    const res = await fetchTenantAccount(props.tenantCode);
    account.value = res;
    if (res.actions) {
      accountActions.value = res.actions;
    }
  } catch (error) {
    logger.error("Failed to load tenant account:", error);
  } finally {
    loading.value = false;
  }
};
const loadTransactions = async () => {
  transactionsLoading.value = true;
  try {
    const params: TransactionListQuery = {
      page: pagination.page,
      limit: pagination.limit,
      transaction_type:
        (transactionFilter.value as
          | "credit"
          | "refund"
          | "charge"
          | "adjustment"
          | "payment"
          | undefined) || undefined,
    };
    const res = await getTenantTransactions(props.tenantCode, params);
    transactions.value = res.transactions;
    transactionsTotal.value = res.total;
  } catch (error) {
    logger.error("Failed to load transactions:", error);
  } finally {
    transactionsLoading.value = false;
  }
};
const handleFilterChange = () => {
  pagination.page = 1;
  loadTransactions();
};
const handlePageSizeChange = (size: number) => {
  pagination.limit = size;
  pagination.page = 1;
  loadTransactions();
};
const handleCurrentPageChange = (page: number) => {
  pagination.page = page;
  loadTransactions();
};
const handleRecharge = () => {
  rechargeForm.amount = 100;
  rechargeForm.description = "";
  rechargeReasonCode.value = "other";
  rechargeVisible.value = true;
};
const handleSubmitRecharge = async () => {
  const valid = await rechargeFormRef.value?.validate().catch(() => false);
  if (!valid) return;
  recharging.value = true;
  try {
    const payload: RechargeRequest = {
      ...toRaw(rechargeForm),
      description: composeReasonText(
        rechargeReasonCode.value,
        rechargeForm.description || "",
      ),
    };
    await rechargeTenantAccount(props.tenantCode, payload);
    ElMessage.success($t("common.feedback.rechargeSuccess"));
    rechargeVisible.value = false;
    pagination.page = 1;
    transactionFilter.value = "credit";
    await loadAccount();
    await loadTransactions();
  } catch (error) {
    if (
      isHttpError(error) &&
      typeof error.data === "object" &&
      error.data !== null &&
      (
        error.data as {
          code?: string;
        }
      ).code === "MFA_NOT_ENABLED"
    ) {
      await ElMessageBox.alert(
        $t("tenant.account.mfaDisabledMessage"),
        $t("tenant.account.mfaDisabledTitle"),
        {
          type: "warning",
          confirmButtonText: $t("common.confirm"),
          closeOnClickModal: false,
          closeOnPressEscape: false,
        },
      );
      return;
    }
    logger.error("Failed to recharge:", error);
  } finally {
    recharging.value = false;
  }
};
const handleRefund = () => {
  refundForm.amount = Math.min(100, maxRefundAmount.value);
  refundForm.reason = "";
  refundReasonCode.value = "other";
  refundVisible.value = true;
};
const handleSubmitRefund = async () => {
  const valid = await refundFormRef.value?.validate().catch(() => false);
  if (!valid) return;
  if (refundForm.amount > maxRefundAmount.value) {
    ElMessage.error($t("tenant.messages.refundExceedsBalance"));
    return;
  }
  refunding.value = true;
  try {
    const payload: RefundRequest = {
      ...toRaw(refundForm),
      reason: composeReasonText(
        refundReasonCode.value,
        refundForm.reason || "",
      ),
    };
    await refundTenantAccount(props.tenantCode, payload);
    ElMessage.success($t("common.feedback.refundSuccess"));
    refundVisible.value = false;
    pagination.page = 1;
    transactionFilter.value = "refund";
    await loadAccount();
    await loadTransactions();
  } catch (error) {
    logger.error("Failed to refund:", error);
  } finally {
    refunding.value = false;
  }
};
onMounted(() => {
  loadAccount();
  loadTransactions();
});
</script>

<style scoped lang="scss">
.account-tab {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.account-overview-card {
  :deep(.el-card__body) {
    padding: 20px 24px 24px;
  }
}

.account-overview-header {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.account-overview-subtitle {
  font-size: 13px;
  color: var(--text-color-secondary);
}

.account-overview-grid {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: 24px;
  align-items: center;
}

.balance-label {
  font-size: 14px;
  color: var(--text-color-secondary);
  margin-bottom: 8px;
}

.balance-amount {
  display: flex;
  align-items: baseline;
  flex-wrap: wrap;
  gap: 6px;

  .currency {
    font-size: 16px;
    color: var(--text-color-secondary);
  }

  .amount {
    font-size: 40px;
    font-weight: 600;
    color: var(--el-color-primary);
    line-height: 1;
  }
}

.balance-meta {
  margin-top: 12px;
  font-size: 13px;
  color: var(--text-color-secondary);
}

.transactions-card {
  margin-top: 0;

  :deep(.el-card__header) {
    padding: 12px 18px 10px;
  }

  :deep(.el-card__body) {
    padding-top: 0;
  }
}

.transactions-card-header {
  align-items: flex-start;
}

.transactions-card-title {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.transactions-card-subtitle {
  font-size: 12px;
  color: var(--text-color-secondary);
}

.transactions-filter {
  width: 104px;

  :deep(.el-input__wrapper) {
    padding-left: 8px;
    padding-right: 8px;
    border-radius: 999px;
  }
}

.transactions-table {
  :deep(.transaction-tag.el-tag) {
    color: var(--text-color-secondary);
    background-color: var(--el-fill-color-light);
    border-color: var(--border-color-lighter);
  }

  :deep(.el-table__header th.el-table__cell) {
    padding-top: 10px;
    padding-bottom: 10px;
  }

  :deep(.el-table__body td.el-table__cell) {
    padding-top: 10px;
    padding-bottom: 10px;
  }
}

.amount {
  font-weight: 600;

  &.positive {
    color: var(--el-color-success-light-3);
  }

  &.negative {
    color: var(--el-color-danger-light-3);
  }
}

.balance-change {
  color: var(--text-color-secondary);
  font-size: 12px;
}

.balance-change-separator {
  margin: 0 6px;
  color: var(--border-color);
}

.pagination-wrapper {
  margin-top: 20px;
  display: flex;
  justify-content: flex-end;
}

.current-balance {
  font-size: 16px;
  font-weight: 600;
  color: var(--el-color-primary);
}

.balance-preview {
  font-size: 16px;
  font-weight: 600;
  color: var(--el-color-success);

  &.warning {
    color: var(--el-color-danger);
  }
}

.ml-2 {
  margin-left: 8px;
}

.surface-panel-pagination {
  margin-top: 12px;
}

.account-action-dialog {
  :deep(.el-dialog__body) {
    padding: 14px 20px 10px;
  }

  :deep(.el-dialog__footer) {
    padding: 0 20px 16px;
  }
}

.compact-form {
  :deep(.amount-input .el-input__wrapper) {
    min-height: 38px;
    border-radius: var(--el-border-radius-small);
    background: var(--el-fill-color-light);
    box-shadow: none;
  }

  :deep(.amount-input .el-input__inner) {
    font-size: 14px;
    font-weight: 600;
  }

  :deep(.text-input .el-input__wrapper),
  :deep(.text-input .el-textarea__inner) {
    min-height: 38px;
    border-radius: var(--el-border-radius-small);
    background: var(--el-fill-color-light);
    box-shadow: none;
  }

  :deep(.text-input .el-textarea__inner) {
    min-height: 82px;
    padding-top: 10px;
    padding-bottom: 10px;
  }

  :deep(.dialog-balance-strip),
  :deep(.dialog-preview-strip) {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    margin-bottom: 14px;
    padding: 10px 12px;
    border-radius: var(--el-border-radius-small);
    background: var(--el-fill-color-light);
  }

  :deep(.dialog-preview-strip.warning) {
    background: rgba(var(--el-color-danger-rgb), 0.08);
  }

  :deep(.dialog-balance-label) {
    font-size: 13px;
    color: var(--text-color-secondary);
  }

  :deep(.dialog-balance-value) {
    font-size: 14px;
    font-weight: 600;
  }

  :deep(.el-form-item) {
    margin-bottom: 14px;
  }

  :deep(.el-form-item__label) {
    line-height: 32px;
  }

  :deep(.el-input__wrapper),
  :deep(.el-textarea__inner) {
    box-shadow: none;
  }
}

@media (max-width: 768px) {
  .account-overview-card {
    :deep(.el-card__body) {
      padding: 16px;
    }
  }

  .account-overview-grid {
    grid-template-columns: 1fr;
    gap: 16px;
  }

  .transactions-card-header {
    flex-direction: column;
    gap: 12px;
  }

  .transactions-filter {
    width: 100%;
  }

  .account-action-dialog {
    :deep(.el-dialog__body) {
      padding-left: 16px;
      padding-right: 16px;
    }

    :deep(.el-dialog__footer) {
      padding-left: 16px;
      padding-right: 16px;
    }
  }
}
</style>
