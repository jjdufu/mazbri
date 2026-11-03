<template>
  <TenantTabShell>
    <ElAlert
      v-if="isInitialized && licenseStatus !== null && !tenantCryptoEnabled"
      type="warning"
      show-icon
      :closable="false"
      :title="$t('tenant.transportEncryption.alert.title')"
    >
      <template #default>
        <div class="alert-content">
          <span>
            {{
              $t("tenant.transportEncryption.alert.currentStatus", {
                status: getStatusLabel(licenseStatus?.status ?? ""),
              })
            }}
          </span>
          <ElButton
            v-if="cryptoActions.manage_root_package"
            v-roles="ROLES_ADMIN"
            link
            type="primary"
            @click="goToRootPackagePage"
          >
            {{ $t("tenant.transportEncryption.actions.goRootPackage") }}
          </ElButton>
        </div></template>
    </ElAlert>

    <div class="strategy-grid">

      <ElCard shadow="hover" class="strategy-card">
        <template #header>
          <SurfaceCardHeader stack-on-mobile>
            <template #left>
              <div class="header-title">
                <ArtSvgIcon icon="ri:lock-password-line" class="header-icon" />
                <span>{{
                  $t("tenant.transportEncryption.sections.strategy")
                }}</span>
              </div>
            </template>
            <template #actions>
              <ElButton
                v-if="cryptoActions.update_profile"
                v-roles="ROLES_BIZ"
                type="primary"
                :loading="saving"
                :disabled="!tenantCryptoEnabled"
                @click="saveProfile"
              >
                {{ $t("common.actions.save") }}
              </ElButton>
            </template>
          </SurfaceCardHeader>
        </template>

        <ElForm label-position="top" class="strategy-form">

          <div class="settings-panel">
            <CommonFormGrid :columns="2" gap="16px 32px">
              <div class="form-row-item span-2 encryption-mode-section">
                <div class="form-row-label">
                  <span>{{
                    $t("tenant.transportEncryption.fields.encryptionMode")
                  }}</span>
                </div>
                <div class="form-row-content encryption-mode-content">
                  <div class="encryption-mode-cards">
                    <div
                      v-for="option in cardOptions"
                      :key="option.value"
                      class="mode-card"
                      :class="{
                        active: form.encryptionMode === option.value,
                        disabled: !tenantCryptoEnabled || !canWriteBusiness,
                      }"
                      @click="
                        tenantCryptoEnabled &&
                        canWriteBusiness &&
                        (form.encryptionMode = option.value)
                      "
                    >
                      <div class="mode-card-icon">
                        <ArtSvgIcon :icon="option.icon" />
                      </div>
                      <div class="mode-card-info">
                        <div class="mode-card-title">{{ option.label }}</div>
                        <div class="mode-card-desc">{{ option.desc }}</div>
                      </div>
                    </div>
                  </div>
                </div>
              </div>
              <div class="form-row-item span-2 encryption-suite-section">
                <div class="form-row-label">
                  <span>{{
                    $t("tenant.transportEncryption.fields.suite")
                  }}</span>
                </div>
                <div class="form-row-content encryption-suite-content">
                  <CryptoSuiteSelect
                    v-model="suiteForm"
                    :disabled="
                      !tenantCryptoEnabled ||
                      !canWriteBusiness ||
                      form.encryptionMode === 'none'
                    "
                    :can-toggle-insecure="isSuperAdmin"
                  />
                </div>
              </div>
              <div class="form-row-item span-2">
                <div class="form-row-label">
                  <span>{{
                    $t("tenant.transportEncryption.fields.currentKeyVersion")
                  }}</span>
                </div>
                <div class="form-row-content">
                  <ElSelect
                    v-model="form.currentKeyVersion"
                    clearable
                    :disabled="
                      form.encryptionMode === 'none' ||
                      !tenantCryptoEnabled ||
                      !canWriteBusiness
                    "
                    :placeholder="
                      $t(
                        'tenant.transportEncryption.placeholders.selectActiveKey',
                      )
                    "
                  >
                    <ElOption
                      v-for="key in activeKeys"
                      :key="key.key_version"
                      :label="key.key_version"
                      :value="key.key_version"
                    />
                  </ElSelect>
                </div>
              </div>
            </CommonFormGrid>
          </div>

          <div class="settings-panel">
            <CommonFormGrid :columns="2" gap="16px 32px">
              <div class="form-row-item">
                <div class="form-row-label">
                  <span>{{
                    $t("tenant.transportEncryption.fields.signatureCheck")
                  }}</span>
                </div>
                <div class="form-row-content">
                  <CommonSwitchRow>
                    <ElSwitch
                      v-model="form.requireSignature"
                      :disabled="!tenantCryptoEnabled || !canWriteBusiness || form.encryptionMode === 'none'"
                    />
                    <span class="switch-text ml-2">
                      {{
                        form.requireSignature
                          ? $t("common.status.enabled")
                          : $t("common.status.disabled")
                      }}
                    </span>
                  </CommonSwitchRow>
                </div>
              </div>
            </CommonFormGrid>
          </div>

          <div class="settings-panel">
            <CommonFormGrid :columns="2" gap="16px 32px">
              <div class="form-row-item span-2">
                <div class="form-row-label">
                  <span>{{
                    $t("tenant.transportEncryption.fields.nonceTtlSeconds")
                  }}</span>
                </div>
                <div class="form-row-content">
                  <ElInputNumber
                    v-model="form.nonceTtlSeconds"
                    :min="60"
                    :max="3600"
                    :step="60"
                    :disabled="!tenantCryptoEnabled || !canWriteBusiness"
                  />
                </div>
              </div>
              <div class="form-row-item span-2">
                <div class="form-row-label">
                  <span>{{
                    $t("tenant.transportEncryption.fields.clockSkewSeconds")
                  }}</span>
                </div>
                <div class="form-row-content">
                  <ElInputNumber
                    v-model="form.clockSkewSeconds"
                    :min="60"
                    :max="3600"
                    :step="60"
                    :disabled="!tenantCryptoEnabled || !canWriteBusiness"
                  />
                </div>
              </div>
            </CommonFormGrid>
          </div>
        </ElForm>
      </ElCard>

      <ElCard shadow="hover" class="strategy-card">
        <template #header>
          <SurfaceCardHeader stack-on-mobile>
            <template #left>
              <div class="header-title">
                <ArtSvgIcon icon="ri:key-2-line" class="header-icon" />
                <span>{{
                  $t("tenant.transportEncryption.sections.keyVersions")
                }}</span>
              </div>
            </template>
            <template #actions>
              <ElButton
                v-if="cryptoActions.create_key"
                v-roles="ROLES_BIZ"
                :loading="creating"
                :disabled="!tenantCryptoEnabled"
                @click="createKey"
              >
                {{ $t("tenant.transportEncryption.actions.createKey") }}
              </ElButton>
              <ElButton
                v-if="cryptoActions.rotate_key"
                v-roles="ROLES_BIZ"
                type="primary"
                :loading="rotating"
                :disabled="!tenantCryptoEnabled"
                @click="rotateKey"
              >
                {{ $t("tenant.transportEncryption.actions.rotateKey") }}
              </ElButton>
            </template>
          </SurfaceCardHeader>
        </template>

        <ArtTable
          v-loading="loading"
          :data="keys"
          :columns="keyColumns"
          border
          stripe
          empty-height="auto"
          class="keys-table"
        >
          <template #status="{ row }">
            <StatusIndicator :type="statusType(row.status)">{{
              getStatusLabel(row.status)
            }}</StatusIndicator>
          </template>
          <template #operations="{ row }">
            <CommonInlineRow gap="8px">
              <ElButton
                v-if="cryptoActions.activate_key && row.status !== 'active'"
                v-roles="ROLES_BIZ"
                link
                type="primary"
                :disabled="!tenantCryptoEnabled"
                @click="setStatus(row.key_version, 'activate')"
              >
                {{ $t("tenant.transportEncryption.actions.activate") }}
              </ElButton>
              <ElButton
                v-if="cryptoActions.disable_key && row.status !== 'disabled'"
                v-roles="ROLES_BIZ"
                link
                type="warning"
                :disabled="!tenantCryptoEnabled"
                @click="setStatus(row.key_version, 'disable')"
              >
                {{ $t("tenant.transportEncryption.actions.disable") }}
              </ElButton>
              <ElButton
                v-if="cryptoActions.delete_key"
                v-roles="ROLES_BIZ"
                link
                type="danger"
                :disabled="!tenantCryptoEnabled"
                @click="handleDelete(row.key_version)"
              >
                {{ $t("tenant.transportEncryption.actions.delete") }}
              </ElButton>
            </CommonInlineRow>
          </template>
        </ArtTable>
      </ElCard>
    </div>

    <CenteredDialog
      v-model="secretDialogVisible"
      :title="$t('tenant.transportEncryption.secretDialog.title')"
      width="620px"
      destroy-on-close
    >
      <ElAlert
        type="warning"
        show-icon
        :closable="false"
        :title="$t('tenant.transportEncryption.secretDialog.warning')"
      />
      <ElDescriptions class="mt-4" :column="1" border>
        <ElDescriptionsItem
          :label="
            $t('tenant.transportEncryption.secretDialog.fields.keyVersion')
          "
          >{{ revealedKey?.keyVersion }}</ElDescriptionsItem
        >
        <ElDescriptionsItem
          :label="
            $t('tenant.transportEncryption.secretDialog.fields.algorithm')
          "
          >{{ revealedKey?.algorithm }}</ElDescriptionsItem
        >
        <ElDescriptionsItem
          :label="
            $t(
              'tenant.transportEncryption.secretDialog.fields.signatureAlgorithm',
            )
          "
          >{{ revealedKey?.signatureAlgorithm }}</ElDescriptionsItem
        >
      </ElDescriptions>
      <ElInput
        class="mt-4"
        type="textarea"
        :rows="4"
        readonly
        :model-value="revealedKey?.tenantSecretKey"
      />
      <template #footer>
        <ElButton type="primary" @click="secretDialogVisible = false">{{
          $t("common.actions.close")
        }}</ElButton>
      </template>
    </CenteredDialog>
  </TenantTabShell>
</template>
<script setup lang="ts">
import TenantTabShell from "@/components/common/tenant-tab-shell/index.vue";
import {
  activateTenantCryptoKey,
  createTenantCryptoKey,
  deleteTenantCryptoKey,
  disableTenantCryptoKey,
  fetchTenantCrypto,
  rotateTenantCryptoKey,
  updateTenantCryptoProfile,
} from "@/api/tenant";
import { $t } from "@/locales";
import type {
  TenantCryptoActions,
  TenantCryptoKey,
  TenantCryptoKeyCreateResponse,
  TenantCryptoLicenseStatus,
  TenantEncryptionMode,
} from "@/types/api/tenant";
import { logger } from "@/utils/sys";
import { getStatusLabel } from "@/utils/display-labels";
import type { ColumnOption } from "@/types/component";
import SurfaceCardHeader from "@/components/common/surface-card-header/index.vue";
import CommonSwitchRow from "@/components/common/switch-row/index.vue";
import CommonInlineRow from "@/components/common/inline-row/index.vue";
import StatusIndicator from "@/components/common/StatusIndicator.vue";
import CenteredDialog from "@/components/common/CenteredDialog.vue";
import CommonFormGrid from "@/components/common/form-grid/index.vue";
import CryptoSuiteSelect, {
  type CryptoSuiteSelectValue,
} from "@/components/crypto/CryptoSuiteSelect.vue";
import { ROLES_ADMIN, ROLES_BIZ } from "@/utils/ops/roles";

const props = defineProps<{
  tenantCode: string;
}>();

const router = useRouter();
const loading = ref(false);
const isInitialized = ref(false);
const saving = ref(false);
const creating = ref(false);
const rotating = ref(false);
const keys = ref<TenantCryptoKey[]>([]);
const revealedKey = ref<TenantCryptoKeyCreateResponse | null>(null);
const secretDialogVisible = ref(false);
/** 未加载成功前为 null，禁止猜默认 unconfigured。 */
const licenseStatus = ref<TenantCryptoLicenseStatus | null>(null);
const cryptoActions = ref<TenantCryptoActions>({
  update_profile: false,
  create_key: false,
  rotate_key: false,
  activate_key: false,
  deprecate_key: false,
  disable_key: false,
  delete_key: false,
  allow_insecure: false,
  manage_root_package: false,
});
/** 写能力与不安全开关以 bundle.actions 为准，禁止与本地角色态分叉。 */
const isSuperAdmin = computed(() => cryptoActions.value.allow_insecure);
const canWriteBusiness = computed(() => cryptoActions.value.update_profile);

const form = reactive({
  encryptionMode: "none" as TenantEncryptionMode,
  requireSignature: false,
  currentKeyVersion: undefined as string | undefined,
  nonceTtlSeconds: 300,
  clockSkewSeconds: 300,
});

const suiteForm = reactive<CryptoSuiteSelectValue>({
  suite: "suite/tenant-default-2026-11",
  dataAlg: null,
  macAlg: null,
  allowInsecure: false,
});

const cardOptions = computed(() => [
  {
    value: "none" as const,
    label: $t("tenant.transportEncryption.modes.none"),
    desc: $t("tenant.transportEncryption.modesDesc.none"),
    icon: "ri:lock-unlock-line",
  },
  {
    value: "request" as const,
    label: $t("tenant.transportEncryption.modes.request"),
    desc: $t("tenant.transportEncryption.modesDesc.request"),
    icon: "ri:login-box-line",
  },
  {
    value: "full" as const,
    label: $t("tenant.transportEncryption.modes.full"),
    desc: $t("tenant.transportEncryption.modesDesc.full"),
    icon: "ri:shield-keyhole-line",
  },
]);

const activeKeys = computed(() =>
  keys.value.filter((key) => key.status === "active"),
);
const tenantCryptoEnabled = computed(
  () => licenseStatus.value?.issuanceAllowed === true,
);

const keyColumns = ref<ColumnOption[]>([
  {
    prop: "key_version",
    label: $t("tenant.transportEncryption.table.version"),
    width: 90,
  },
  {
    prop: "status",
    label: $t("tenant.transportEncryption.table.status"),
    width: 120,
    useSlot: true,
  },
  {
    prop: "data_algorithm",
    label: $t("tenant.transportEncryption.table.encryptionAlgorithm"),
    minWidth: 96,
  },
  {
    prop: "signature_algorithm",
    label: $t("tenant.transportEncryption.table.signatureAlgorithm"),
    minWidth: 100,
  },
  {
    prop: "created_at",
    label: $t("tenant.transportEncryption.table.createdAt"),
    minWidth: 120,
  },
  {
    prop: "operations",
    label: $t("tenant.transportEncryption.table.operations"),
    width: 230,
    fixed: "right",
    useSlot: true,
  },
]);

watch(
  () => form.encryptionMode,
  (mode) => {
    if (mode === "none") {
      form.requireSignature = false;
      suiteForm.suite = null;
      suiteForm.dataAlg = null;
      suiteForm.macAlg = null;
      suiteForm.allowInsecure = false;
    } else if (!suiteForm.suite && !suiteForm.dataAlg) {
      suiteForm.suite = "suite/tenant-default-2026-11";
    }
  },
);

const statusType = (status: string) => {
  if (status === "active") return "success";
  if (status === "pending" || status === "imported") return "info";
  if (status === "expired" || status === "revoked" || status === "deleted")
    return "danger";
  return "danger";
};

const applyProfile = (profile: {
  encryption_mode: TenantEncryptionMode;
  require_signature: boolean;
  current_key_version?: string | null;
  nonce_ttl_seconds: number;
  clock_skew_seconds: number;
  suite?: string | null;
  data_alg?: string | null;
  mac_alg?: string | null;
  allow_insecure: boolean;
}) => {
  form.encryptionMode = profile.encryption_mode;
  form.requireSignature =
    profile.encryption_mode === "none" ? false : profile.require_signature;
  form.currentKeyVersion = profile.current_key_version ?? undefined;
  form.nonceTtlSeconds = profile.nonce_ttl_seconds;
  form.clockSkewSeconds = profile.clock_skew_seconds;
  suiteForm.suite = profile.suite ?? null;
  suiteForm.dataAlg = profile.data_alg ?? null;
  suiteForm.macAlg = profile.mac_alg ?? null;
  suiteForm.allowInsecure = profile.allow_insecure === true;
  if (
    form.encryptionMode !== "none" &&
    !suiteForm.suite &&
    !suiteForm.dataAlg
  ) {
    suiteForm.suite = "suite/tenant-default-2026-11";
  }
};

const loadData = async () => {
  loading.value = true;
  try {
    const bundle = await fetchTenantCrypto(props.tenantCode);
    applyProfile(bundle.profile);
    keys.value = bundle.keys;
    licenseStatus.value = bundle.license_status;
    cryptoActions.value = bundle.actions;
    isInitialized.value = true;
  } catch (error) {
    // 加载失败不猜许可证态，保持 null，黄条不渲染。
    licenseStatus.value = null;
    cryptoActions.value = {
      update_profile: false,
      create_key: false,
      rotate_key: false,
      activate_key: false,
      deprecate_key: false,
      disable_key: false,
      delete_key: false,
      allow_insecure: false,
      manage_root_package: false,
    };
    isInitialized.value = false;
    logger.error("Failed to load tenant crypto settings:", error);
  } finally {
    loading.value = false;
  }
};

const goToRootPackagePage = () => {
  router.push({ path: "/control-panel/authorizations" });
};

const showSecret = (key: TenantCryptoKeyCreateResponse) => {
  revealedKey.value = key;
  secretDialogVisible.value = true;
};

const createKey = async () => {
  if (!tenantCryptoEnabled.value || !cryptoActions.value.create_key) return;
  creating.value = true;
  try {
    showSecret(await createTenantCryptoKey(props.tenantCode));
    await loadData();
  } finally {
    creating.value = false;
  }
};

const rotateKey = async () => {
  if (!tenantCryptoEnabled.value || !cryptoActions.value.rotate_key) return;
  rotating.value = true;
  try {
    showSecret(await rotateTenantCryptoKey(props.tenantCode));
    await loadData();
  } finally {
    rotating.value = false;
  }
};

const setStatus = async (version: string, action: "activate" | "disable") => {
  if (!tenantCryptoEnabled.value) return;
  if (action === "activate" && !cryptoActions.value.activate_key) return;
  if (action === "disable" && !cryptoActions.value.disable_key) return;
  if (action === "activate")
    await activateTenantCryptoKey(props.tenantCode, version);
  if (action === "disable")
    await disableTenantCryptoKey(props.tenantCode, version);
  await loadData();
};

const handleDelete = async (version: string) => {
  if (!tenantCryptoEnabled.value || !cryptoActions.value.delete_key) return;
  try {
    await ElMessageBox.confirm(
      $t("tenant.transportEncryption.messages.deleteKeyConfirm", { version }),
      $t("common.tips"),
      {
        confirmButtonText: $t("common.confirm"),
        cancelButtonText: $t("common.cancel"),
        type: "warning",
      },
    );
    await deleteTenantCryptoKey(props.tenantCode, version);
    ElMessage.success($t("common.feedback.deleteSuccess"));
    await loadData();
  } catch (error) {
    if (error !== "cancel") {
      logger.error("Failed to delete crypto key version", error);
    }
  }
};

const saveProfile = async () => {
  if (!tenantCryptoEnabled.value || !cryptoActions.value.update_profile) return;
  if (suiteForm.allowInsecure && !cryptoActions.value.allow_insecure) {
    return;
  }
  saving.value = true;
  try {
    await updateTenantCryptoProfile(props.tenantCode, {
      encryptionMode: form.encryptionMode,
      suite: form.encryptionMode === "none" ? null : suiteForm.suite,
      dataAlg: form.encryptionMode === "none" ? null : suiteForm.dataAlg,
      macAlg: form.encryptionMode === "none" ? null : suiteForm.macAlg,
      allowInsecure:
        form.encryptionMode === "none" ? false : suiteForm.allowInsecure,
      requireSignature: form.requireSignature,
      nonceTtlSeconds: form.nonceTtlSeconds,
      clockSkewSeconds: form.clockSkewSeconds,
      currentKeyVersion: form.currentKeyVersion,
    });
    await loadData();
    ElMessage.success($t("common.feedback.saveSuccess"));
  } finally {
    saving.value = false;
  }
};

onMounted(loadData);
</script>

<style scoped lang="scss">
.transport-encryption-tab {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.strategy-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 16px;
  align-items: start;
}

.strategy-card {
  min-width: 0;
  height: 100%;
}

.strategy-form {
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin-top: 10px;
}

.header-title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-weight: 600;
  font-size: 16px;
}

.header-icon {
  font-size: 20px;
  color: var(--el-color-primary);
}

.settings-panel {
  // Flat section inside the card: no nested bordered/gray box.
  background: transparent;
  border: none;
  border-radius: 0;
  padding: 4px 0 0;
}

.form-row-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 16px 32px;
}

.form-row-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  height: 32px;

  &.span-2 {
    grid-column: span 2;
  }

  .form-row-label {
    font-size: 13px;
    font-weight: 600;
    color: var(--el-text-color-regular);
    white-space: nowrap;
    display: flex;
    align-items: center;
  }

  .form-row-content {
    flex: 1;
    max-width: 180px;
    flex-shrink: 0;

    :deep(.el-input-number) {
      width: 100%;
      min-width: 120px;
    }

    :deep(.el-select) {
      width: 100%;
    }

    :deep(.el-segmented) {
      width: 100%;
    }
  }
}

.keys-table {
  width: 100%;

  :deep(.el-input__wrapper) {
    box-shadow: none !important;
    background: transparent;
  }
}

.alert-content {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  width: 100%;
}

.form-row-item.encryption-mode-section,
.form-row-item.encryption-suite-section {
  height: auto;
  align-items: flex-start;
  flex-direction: column;
  gap: 12px;

  .form-row-label {
    margin-bottom: 4px;
  }
}

.encryption-mode-content,
.encryption-suite-content {
  max-width: 100% !important;
  width: 100%;
}

.encryption-mode-cards {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 12px;
  width: 100%;

  @media (max-width: 1200px) {
    grid-template-columns: 1fr;
  }
}

.mode-card {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  padding: 12px 16px;
  background: var(--el-fill-color-blank);
  border: 1.5px solid var(--el-border-color-light);
  border-radius: var(--custom-radius);
  cursor: pointer;
  transition: all 0.25s cubic-bezier(0.4, 0, 0.2, 1);
  user-select: none;
  width: 100%;
  box-sizing: border-box;

  &:hover:not(.disabled) {
    border-color: var(--el-color-primary-light-5);
    background: var(--el-color-primary-light-9);
    transform: translateY(-1px);
    box-shadow: var(--el-box-shadow-light);
  }

  &.active {
    border-color: var(--el-color-primary);
    background: var(--el-color-primary-light-9);
    box-shadow: 0 4px 12px var(--el-color-primary-light-8);

    .mode-card-icon {
      color: var(--el-color-primary);
      background: var(--el-color-primary-light-8);
    }

    .mode-card-title {
      color: var(--el-color-primary);
    }
  }

  &.disabled {
    cursor: not-allowed;
    opacity: 0.6;
    background: var(--el-fill-color-lighter);
    border-color: var(--el-border-color-lighter);
  }
}

.mode-card-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 20px;
  color: var(--el-text-color-secondary);
  background: var(--el-fill-color-light);
  padding: 8px;
  border-radius: 6px;
  transition: all 0.25s ease;
  flex-shrink: 0;
}

.mode-card-info {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.mode-card-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--el-text-color-primary);
  transition: all 0.25s ease;
}

.mode-card-desc {
  font-size: 12px;
  color: var(--el-text-color-secondary);
  line-height: 1.4;
  white-space: normal;
}

@media (max-width: 768px) {
  .strategy-grid {
    grid-template-columns: 1fr;
  }

  .form-row-item {
    height: auto;

    .form-row-content {
      max-width: 160px;
    }
  }

  .form-row-item.encryption-mode-section {
    .form-row-content {
      max-width: 100%;
    }
  }
}
</style>
