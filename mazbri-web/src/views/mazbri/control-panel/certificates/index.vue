<template>
  <ControlPanelPageShell
    :title="$t('certificate.page.title')"
    icon="ri:award-line"
  >
    <ResourceTableShell
      :columns="[]"
      :title="$t('certificate.list.title')"
      :loading="loading"
      :data="tableData"
      :pagination="artPagination"
      :empty-text="emptyDescription"
      stripe
      border
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

      <template #default>
        <ElTableColumn type="selection" width="55" />
        <ElTableColumn
          prop="subject"
          :label="$t('certificate.list.columns.subject')"
          min-width="250"
        />
        <ElTableColumn
          prop="dns_names"
          :label="$t('certificate.list.columns.dnsNames')"
          min-width="300"
        >
          <template #default="{ row }">
            <ElTag
              v-for="(name, idx) in row.dns_names?.slice(0, 3)"
              :key="idx"
              size="small"
              class="mr-1"
            >
              {{ name }}
            </ElTag>
            <ElTag v-if="row.dns_names?.length > 3" size="small" type="info">
              +{{ row.dns_names.length - 3 }}
            </ElTag>
          </template>
        </ElTableColumn>
        <ElTableColumn
          prop="expires_at"
          :label="$t('certificate.list.columns.expiresAt')"
          width="200"
        >
          <template #default="{ row }">
            <span :class="{ 'text-red-500': isExpiringSoon(row.expires_at) }">
              {{ formatDate(row.expires_at) }}
            </span>
          </template>
        </ElTableColumn>
        <ElTableColumn
          prop="status"
          :label="$t('certificate.list.columns.status')"
          width="150"
        >
          <template #default="{ row }">
            <StatusIndicator :type="getStatusType(getCertificateStatus(row))">
              {{ getStatusLabel(getCertificateStatus(row)) }}
            </StatusIndicator>
          </template>
        </ElTableColumn>
        <ElTableColumn
          :label="$t('common.fields.operations')"
          width="150"
          fixed="right"
        >
          <template #default="{ row }">
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
        </ElTableColumn>
      </template>
    </ResourceTableShell>

    <IssueDialog
      v-model="issueVisible"
      :issued-subjects="issuedSubjects"
      :authorization-disabled="authorizationDisabled"
      :authorization-disabled-reason="authorizationDisabledReason"
      @success="loadData"
    />
  </ControlPanelPageShell>
</template>

<script setup lang="ts">
import { ElMessageBox, ElMessage } from "element-plus";
import { $t } from "@/locales";
import type { Certificate, CertificateStatus } from "@/types/api/certificate";
import { fetchCertificateList, revokeCertificate } from "@/api/certificate";
import IssueDialog from "./modules/issue-dialog.vue";
import StatusIndicator from "@/components/common/StatusIndicator.vue";
import ControlPanelPageShell from "@/components/common/control-panel-page-shell/index.vue";
import ResourceTableShell from "@/components/common/resource-table-shell/index.vue";
import ResourceTableActionBar from "@/components/common/resource-table-action-bar/index.vue";
import { runWithLoading } from "@/composables/useAsyncState";
import { useLicenseGate } from "@/composables/useLicenseGate";
import { useListQuery } from "@/composables/useListQuery";
import { useTableSelection } from "@/composables/useTableSelection";
import { logger } from "@/utils/sys";
import { getStatusType, getStatusLabel } from "@/utils/display-labels";
import { formatDateTime as formatDate } from "@/utils/format";
import type { ColumnOption, PaginationConfig } from "@/types/component";
import { certificatesCatalog } from "@/list-catalog/certificates";
defineOptions({ name: "CertificateList" });
const router = useRouter();
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
    loadData();
  },
});
const columns = ref<ColumnOption<Certificate>[]>([
  { type: "selection", width: 55 },
  {
    prop: "subject",
    label: $t("certificate.list.columns.subject"),
    minWidth: 120,
  },
  {
    prop: "dns_names",
    label: $t("certificate.list.columns.dnsNames"),
    minWidth: 120,
    useSlot: true,
  },
  {
    prop: "expires_at",
    label: $t("certificate.list.columns.expiresAt"),
    width: 180,
    useSlot: true,
  },
  {
    prop: "status",
    label: $t("certificate.list.columns.status"),
    width: 120,
    useSlot: true,
  },
  {
    prop: "operations",
    label: $t("common.fields.operations"),
    width: 120,
    fixed: "right",
    useSlot: true,
  },
]);
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
const isExpiringSoon = (expiresAt: string) => {
  const expiry = new Date(expiresAt);
  const now = new Date();
  const daysUntilExpiry =
    (expiry.getTime() - now.getTime()) / (1000 * 60 * 60 * 24);
  return daysUntilExpiry < 30;
};
const syncTableScroll = () => {
  nextTick(() => {
    const tableEl = document.querySelector(".surface-panel-table");
    if (!tableEl) return;
    const bodyWrapper = tableEl.querySelector(".el-scrollbar__wrap");
    const headerWrapper = tableEl.querySelector(".el-table__header-wrapper");
    if (bodyWrapper && headerWrapper) {
      (bodyWrapper as HTMLElement).onscroll = () => {
        (headerWrapper as HTMLElement).scrollLeft = (
          bodyWrapper as HTMLElement
        ).scrollLeft;
      };
    }
  });
};
const loadData = async () => {
  await runWithLoading(loading, async () => {
    await loadLicenseStatus();
    const offset = (pagination.page - 1) * pagination.limit;
    try {
      const res = await fetchCertificateList({
        ...(toRequestParams() as { q?: string }),
        page: pagination.page,
        limit: pagination.limit,
        offset,
      });
      tableData.value = res.data;
      pagination.total = res.pagination.total;
      syncTableScroll();
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
  loadData();
};
const handleCurrentChange = (page: number) => {
  pagination.page = page;
  loadData();
};
const handleIssue = async () => {
  if (!ensureLicenseActive()) return;
  try {
    const res = await fetchCertificateList({ limit: 1000 });
    issuedSubjects.value = res.data.map((cert) => cert.subject);
  } catch (error) {
    logger.error("Failed to load issued subjects:", error);
    issuedSubjects.value = [];
  }
  issueVisible.value = true;
};
const handleView = (row: Certificate) => {
  router.push(`/control-panel/certificate/${row.id}`);
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
    loadData();
  } catch (error) {
    if (error !== "cancel") {
      logger.error("Failed to revoke certificate:", error);
    }
  }
};
onMounted(() => {
  loadData();
  syncTableScroll();
});
</script>
