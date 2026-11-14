<template>
  <ControlPanelPageShell
    :title="$t('certificate.page.title')"
    icon="ri:award-line"
  >
    <ResourceTableShell
      v-model:columns="columns"
      :title="$t('certificate.list.title')"
      :loading="loading"
      :data="tableData"
      :pagination="artPagination"
      :empty-text="emptyDescription"
      stripe
      @refresh="loadData"
      @selection-change="handleSelectionChange"
      @pagination:size-change="handleSizeChange"
      @pagination:current-change="handleCurrentChange"
    >
      <template #actions>
        <ResourceTableActionBar
          v-model="q"
          v-model:filters="filters"
          :filter-catalog="certificatesCatalog"
          :show-export="false"
          :show-primary="true"
          :primary-label="$t('certificate.list.actions.issue')"
          :primary-disabled="authorizationDisabled"
          :primary-disabled-reason="authorizationDisabledReason"
          :show-delete="true"
          :delete-total-count="tableData.length"
          :delete-selected-count="selectedRows.length"
          :delete-label="$t('certificate.list.actions.revoke')"
          :delete-disabled="authorizationDisabled"
          :delete-disabled-reason="authorizationDisabledReason"
          delete-icon="ri:forbid-line"
          @search="handleSearch"
          @primary="handleIssue"
          @delete="handleDeleteAction"
        />
      </template>

      <template #subject="{ row }">
        <span class="cert-subject-cell">{{ row.subject }}</span>
      </template>

      <template #dns_names="{ row }">
        <span
          v-if="row.dns_names?.length === 1 && row.dns_names[0] === row.subject"
          class="cert-san-same-badge"
        >
          {{ $t("certificate.list.sameAsSubject") }}
        </span>
        <div
          v-else-if="row.dns_names?.length > 1"
          class="cert-san-multi-cell"
        >
          <span class="cert-san-primary">{{ row.dns_names[0] }}</span>
          <ElTooltip :content="row.dns_names.join(', ')" placement="top">
            <ElTag size="small" type="info" effect="plain">
              +{{ row.dns_names.length - 1 }} SAN
            </ElTag>
          </ElTooltip>
        </div>
        <ElTag
          v-else-if="row.dns_names?.length === 1"
          size="small"
          effect="plain"
        >
          {{ row.dns_names[0] }}
        </ElTag>
        <span v-else>{{ EMPTY_PLACEHOLDER }}</span>
      </template>

      <template #fingerprint="{ row }">
        <div v-if="row.fingerprint" class="cert-fingerprint-cell">
          <ElTooltip :content="row.fingerprint" placement="top">
            <code class="cert-fingerprint-cell__short">
              {{ formatShortFingerprint(row.fingerprint) }}
            </code>
          </ElTooltip>
          <ElTooltip
            :content="$t('certificate.list.copyFingerprint')"
            placement="top"
          >
            <ElButton
              link
              type="primary"
              :icon="CopyDocument"
              :aria-label="$t('certificate.list.copyFingerprint')"
              @click="handleCopyFingerprint(row.fingerprint)"
            />
          </ElTooltip>
        </div>
        <span v-else>{{ EMPTY_PLACEHOLDER }}</span>
      </template>

      <template #expires_at="{ row }">
        <div class="cert-expiry-cell">
          <span :class="getExpiryToneClass(row)">
            {{ formatDate(row.expires_at) }}
          </span>
          <span class="cert-expiry-cell__sub" :class="getExpiryToneClass(row)">
            {{ formatExpirySubtitle(row) }}
          </span>
        </div>
      </template>

      <template #status="{ row }">
        <StatusIndicator :type="getStatusType(getCertificateStatus(row))">
          {{ getStatusLabel(getCertificateStatus(row)) }}
        </StatusIndicator>
      </template>

      <template #operations="{ row }">
        <TableActionRow>
          <ElButton link type="primary" @click="handleView(row)">
            {{ $t("common.actions.details") }}
          </ElButton>
          <ElTooltip
            :disabled="!(row.revoked || authorizationDisabled)"
            :content="
              row.revoked
                ? $t('certificate.list.disabledReasons.revoked')
                : authorizationDisabledReason
            "
          >
            <span>
              <ElButton
                link
                type="danger"
                :disabled="row.revoked || authorizationDisabled"
                @click="handleRevoke(row)"
              >
                {{ $t("certificate.list.actions.revoke") }}
              </ElButton>
            </span>
          </ElTooltip>
        </TableActionRow>
      </template>
    </ResourceTableShell>

    <IssueDialog
      v-if="issueVisible"
      v-model="issueVisible"
      :issued-subjects="issuedSubjects"
      :authorization-disabled="authorizationDisabled"
      :authorization-disabled-reason="authorizationDisabledReason"
      @success="loadData"
    />

    <CertificateDetailDrawer
      v-model="detailVisible"
      :cert-id="selectedCert?.id ?? null"
      :initial-cert="selectedCert"
      :authorization-disabled="authorizationDisabled"
      :authorization-disabled-reason="authorizationDisabledReason"
      @changed="loadData"
    />
  </ControlPanelPageShell>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { ElMessage, ElMessageBox } from "element-plus";
import { CopyDocument } from "@element-plus/icons-vue";
import { $t } from "@/locales";
import type { Certificate, CertificateStatus } from "@/types/api/certificate";
import {
  fetchCertificateList,
  fetchIssuedCertificateSubjects,
  revokeCertificate,
} from "@/api/certificate";
import IssueDialog from "./modules/issue-dialog.vue";
import CertificateDetailDrawer from "./modules/CertificateDetailDrawer.vue";
import StatusIndicator from "@/components/common/StatusIndicator.vue";
import ControlPanelPageShell from "@/components/common/control-panel-page-shell/index.vue";
import ResourceTableShell from "@/components/common/resource-table-shell/index.vue";
import ResourceTableActionBar from "@/components/common/resource-table-action-bar/index.vue";
import { runWithLoading } from "@/composables/useAsyncState";
import { useLicenseGate } from "@/composables/useLicenseGate";
import { useListQuery } from "@/composables/useListQuery";
import { useTableSelection } from "@/composables/useTableSelection";
import { logger } from "@/utils/sys";
import { getStatusLabel, getStatusType } from "@/utils/display-labels";
import {
  EMPTY_PLACEHOLDER,
  formatDateTime as formatDate,
} from "@/utils/format";
import { copyWithFeedback } from "@/utils/ui/copy-feedback";
import type { ColumnOption, PaginationConfig } from "@/types/component";
import { certificatesCatalog } from "@/list-catalog/certificates";

defineOptions({ name: "CertificateList" });

const { t, locale } = useI18n();
const loading = ref(false);
const {
  licenseActive,
  authorizationDisabled,
  authorizationDisabledReason,
  loadLicenseStatus,
  ensureLicenseActive,
} = useLicenseGate();

const tableData = ref<Certificate[]>([]);
const issueVisible = ref(false);
const issuedSubjects = ref<string[]>([]);
const detailVisible = ref(false);
const selectedCert = ref<Certificate | null>(null);

const pagination = reactive({
  page: 1,
  limit: 10,
  total: 0,
});

const { q, filters, handleSearch, toRequestParams } = useListQuery({
  catalog: certificatesCatalog,
  searchOnQChange: false,
  encodeMode: "json",
  onChange: () => {
    pagination.page = 1;
    void loadData();
  },
});

const buildCertificateColumns = (): ColumnOption<Certificate>[] => {
  const isEn = String(locale.value).startsWith("en");
  return [
    { type: "selection", width: 48 },
    {
      prop: "subject",
      label: t("certificate.list.columns.subject"),
      minWidth: 164,
      useSlot: true,
    },
    {
      prop: "dns_names",
      label: t("certificate.list.columns.dnsNames"),
      minWidth: 184,
      useSlot: true,
    },
    {
      prop: "fingerprint",
      label: t("certificate.list.columns.fingerprint"),
      minWidth: 212,
      useSlot: true,
    },
    {
      prop: "expires_at",
      label: t("certificate.list.columns.expiresAt"),
      minWidth: 188,
      useSlot: true,
    },
    {
      prop: "status",
      label: t("certificate.list.columns.status"),
      width: isEn ? 136 : 116,
      useSlot: true,
    },
    {
      prop: "operations",
      label: t("common.fields.operations"),
      width: isEn ? 148 : 128,
      fixed: "right",
      useSlot: true,
    },
  ];
};

const columns = ref<ColumnOption<Certificate>[]>(buildCertificateColumns());
watch(locale, () => {
  const prev = new Map(
    columns.value.map((col) => [String(col.prop ?? col.type ?? ""), col]),
  );
  columns.value = buildCertificateColumns().map((col) => {
    const existing = prev.get(String(col.prop ?? col.type ?? ""));
    return existing
      ? {
          ...col,
          visible: existing.visible ?? col.visible,
          checked: existing.checked ?? col.checked,
        }
      : col;
  });
});

const hasActiveSearch = computed(
  () => Boolean(q.value.trim()) || filters.value.length > 0,
);
const emptyDescription = computed(() =>
  hasActiveSearch.value
    ? $t("common.feedback.noMatchingData")
    : $t("common.feedback.noData"),
);
const artPagination = computed<PaginationConfig>(() => ({
  current: pagination.page,
  size: pagination.limit,
  total: pagination.total,
}));

const { selectedRows, handleSelectionChange, handleDeleteCommand } =
  useTableSelection<Certificate>({
    deleteApi: (row) => revokeCertificate(row.id),
    getItemName: (row) => row.subject,
    onDeleteSuccess: () => loadData(),
    actionLabel: $t("certificate.list.actions.revoke"),
  });

const getDaysUntilExpiry = (expiresAt: string): number => {
  const expiry = new Date(expiresAt);
  const now = new Date();
  return Math.ceil(
    (expiry.getTime() - now.getTime()) / (1000 * 60 * 60 * 24),
  );
};

const getCertificateStatus = (cert: Certificate): CertificateStatus => {
  if (cert.revoked) return "revoked";
  const now = new Date();
  const expiresAt = new Date(cert.expires_at);
  const daysUntilExpiry =
    (expiresAt.getTime() - now.getTime()) / (1000 * 60 * 60 * 24);
  if (daysUntilExpiry < 0) return "expired";
  if (daysUntilExpiry < 30) return "expiring_soon";
  return "valid";
};

const getExpiryToneClass = (row: Certificate): string => {
  const status = getCertificateStatus(row);
  if (status === "expired") return "cert-expiry-text--danger";
  if (status === "expiring_soon") return "cert-expiry-text--warning";
  return "";
};

const formatExpirySubtitle = (row: Certificate): string => {
  if (row.revoked) return t("common.statusLabels.revoked");
  const days = getDaysUntilExpiry(row.expires_at);
  if (days < 0) {
    return t("certificate.list.expiredDaysAgo", { count: Math.abs(days) });
  }
  return t("certificate.list.remainingDays", { count: days });
};

const formatShortFingerprint = (fp: string): string => {
  const normalized = fp.trim();
  if (normalized.length <= 20) return normalized;
  return `${normalized.slice(0, 8)}...${normalized.slice(-8)}`;
};

const handleCopyFingerprint = (fp: string) => {
  void copyWithFeedback(fp);
};

const loadData = async () => {
  await runWithLoading(loading, async () => {
    await loadLicenseStatus();
    const offset = (pagination.page - 1) * pagination.limit;
    try {
      const res = await fetchCertificateList({
        ...(toRequestParams() as { q?: string; filters?: string }),
        page: pagination.page,
        limit: pagination.limit,
        offset,
      });
      tableData.value = res.data;
      pagination.total = res.pagination.total;
    } catch (error) {
      tableData.value = [];
      pagination.total = 0;
      if (licenseActive.value) {
        throw error;
      }
    }
  }).catch((error) => {
    logger.error("Failed to load certificate list:", error);
  });
};

const handleDeleteAction = async (type: "all" | "selected") => {
  if (!ensureLicenseActive()) return;
  await handleDeleteCommand(type, tableData.value);
};

const handleSizeChange = (size: number) => {
  pagination.limit = size;
  pagination.page = 1;
  void loadData();
};

const handleCurrentChange = (page: number) => {
  pagination.page = page;
  void loadData();
};

const handleIssue = async () => {
  if (!ensureLicenseActive()) return;
  try {
    issuedSubjects.value = await fetchIssuedCertificateSubjects();
  } catch (error) {
    logger.error("Failed to load issued subjects:", error);
    issuedSubjects.value = [];
  }
  issueVisible.value = true;
};

const handleView = (row: Certificate) => {
  selectedCert.value = row;
  detailVisible.value = true;
};

const handleRevoke = async (row: Certificate) => {
  if (!ensureLicenseActive()) return;
  try {
    await ElMessageBox.confirm(
      $t("certificate.messages.revokeConfirm", { name: row.subject }),
      $t("common.warning"),
      {
        confirmButtonText: $t("certificate.messages.confirmRevoke"),
        cancelButtonText: $t("common.cancel"),
        type: "warning",
      },
    );
    await revokeCertificate(row.id);
    ElMessage.success($t("common.feedback.revokeSuccess"));
    if (selectedCert.value?.id === row.id && detailVisible.value) {
      selectedCert.value = { ...row, revoked: true };
    }
    void loadData();
  } catch (error) {
    if (error !== "cancel") {
      logger.error("Failed to revoke certificate:", error);
    }
  }
};

onMounted(() => {
  void loadData();
});
</script>

<style scoped lang="scss">
.cert-subject-cell {
  font-weight: 600;
  color: var(--el-text-color-primary);
}

.cert-san-same-badge {
  display: inline-flex;
  align-items: center;
  padding: 1px 8px;
  border-radius: var(--el-border-radius-small);
  background-color: var(--el-fill-color-light);
  color: var(--el-text-color-secondary);
  font-size: 12px;
  line-height: 1.5;
}

.cert-san-multi-cell {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}

.cert-san-primary {
  color: var(--el-text-color-regular);
  font-size: 13px;
}

.cert-fingerprint-cell {
  display: inline-flex;
  align-items: center;
  gap: 4px;

  &__short {
    font-family: var(--el-font-family-monospace, ui-monospace, SFMono-Regular, Menlo, monospace);
    font-size: 12px;
    color: var(--el-text-color-regular);
    background-color: var(--el-fill-color-lighter);
    padding: 1px 6px;
    border-radius: var(--el-border-radius-small);
  }
}

.cert-expiry-cell {
  display: flex;
  flex-direction: column;
  gap: 2px;
  line-height: 1.35;

  &__sub {
    font-size: 12px;
    color: var(--el-text-color-secondary);
  }
}

.cert-expiry-text--warning {
  color: var(--el-color-warning) !important;
}

.cert-expiry-text--danger {
  color: var(--el-color-danger) !important;
}
</style>
