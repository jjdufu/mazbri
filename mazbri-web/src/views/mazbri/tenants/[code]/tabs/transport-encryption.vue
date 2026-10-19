<template>
  <div class="transport-encryption-tab">
    <ElAlert
      v-if="isInitialized && !tenantCryptoEnabled"
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
                status: getStatusLabel(rootStatus.status),
              })
            }}
          </span>
          <ElButton link type="primary" @click="goToRootPackagePage">
            {{ $t("tenant.transportEncryption.actions.goRootPackage") }}
          </ElButton>
        </div>
      </template>
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
                v-roles="['R_ADMIN', 'R_OPERATOR']"
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
                        disabled: !tenantCryptoEnabled,
                      }"
                      @click="
                        tenantCryptoEnabled &&
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
                      form.encryptionMode === 'none' || !tenantCryptoEnabled
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
                      :disabled="!tenantCryptoEnabled || form.encryptionMode === 'none'"
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
              <div class="form-row-item">
                <div class="form-row-label">
                  <span>{{
                    $t("tenant.transportEncryption.fields.codecEnabled")
                  }}</span>
                </div>
                <div class="form-row-content">
                  <CommonSwitchRow>
                    <ElSwitch
                      v-model="form.codecEnabled"
                      :disabled="
                        !tenantCryptoEnabled ||
                        (form.encryptionMode !== 'full' &&
                          form.encryptionMode !== 'request')
                      "
                    />
                    <span class="switch-text ml-2">
                      {{
                        form.codecEnabled
                          ? $t("common.status.enabled")
                          : $t("common.status.disabled")
                      }}
                    </span>
                  </CommonSwitchRow>
                </div>
              </div>
            </CommonFormGrid>
          </div>

          <div v-if="form.codecEnabled" class="settings-panel">
            <CommonFormGrid :columns="2" gap="16px 32px">
              <div class="form-row-item span-2">
                <div class="form-row-label">
                  <span>{{
                    $t("tenant.transportEncryption.fields.requestCodec")
                  }}</span>
                </div>
                <div class="form-row-content">
                  <ElSelect
                    v-model="form.requestCodec"
                    :disabled="
                      !tenantCryptoEnabled ||
                      !form.codecEnabled ||
                      (form.encryptionMode !== 'full' &&
                        form.encryptionMode !== 'request')
                    "
                  >
                    <ElOption
                      v-for="codec in codecOptions"
                      :key="codec.value"
                      :label="codec.label"
                      :value="codec.value"
                    />
                  </ElSelect>
                </div>
              </div>
              <div class="form-row-item span-2">
                <div class="form-row-label">
                  <span>{{
                    $t("tenant.transportEncryption.fields.responseCodec")
                  }}</span>
                </div>
                <div class="form-row-content">
                  <ElSelect
                    v-model="form.responseCodec"
                    :disabled="
                      !tenantCryptoEnabled ||
                      !form.codecEnabled ||
                      (form.encryptionMode !== 'full' &&
                        form.encryptionMode !== 'request')
                    "
                  >
                    <ElOption
                      v-for="codec in codecOptions"
                      :key="codec.value"
                      :label="codec.label"
                      :value="codec.value"
                    />
                  </ElSelect>
                </div>
              </div>
              <div class="form-row-item span-2">
                <div class="form-row-label">
                  <span>{{
                    $t("tenant.transportEncryption.fields.codecVersion")
                  }}</span>
                </div>
                <div class="form-row-content">
                  <ElInputNumber
                    v-model="form.codecVersion"
                    :min="1"
                    :max="99"
                    :step="1"
                    :disabled="
                      !tenantCryptoEnabled ||
                      !form.codecEnabled ||
                      (form.encryptionMode !== 'full' &&
                        form.encryptionMode !== 'request')
                    "
                  />
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
                    :disabled="!tenantCryptoEnabled"
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
                    :disabled="!tenantCryptoEnabled"
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
                :loading="creating"
                :disabled="!tenantCryptoEnabled"
                @click="createKey"
              >
                {{ $t("tenant.transportEncryption.actions.createKey") }}
              </ElButton>
              <ElButton
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
                v-if="row.status !== 'active'"
                link
                type="primary"
                :disabled="!tenantCryptoEnabled"
                @click="setStatus(row.key_version, 'activate')"
              >
                {{ $t("tenant.transportEncryption.actions.activate") }}
              </ElButton>
              <ElButton
                v-if="row.status !== 'disabled'"
                link
                type="warning"
                @click="setStatus(row.key_version, 'disable')"
              >
                {{ $t("tenant.transportEncryption.actions.disable") }}
              </ElButton>
              <ElButton
                link
                type="danger"
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
  </div>
</template>

<script setup lang="ts">
import {
  activateTenantCryptoKey,
  createTenantCryptoKey,
  deleteTenantCryptoKey,
  disableTenantCryptoKey,
  fetchTenantCryptoKeys,
  fetchTenantCryptoProfile,
  rotateTenantCryptoKey,
  updateTenantCryptoProfile,
} from "@/api/tenant";
import { fetchLicenseStatus } from "@/api/authorizations";
import { $t } from "@/locales";
import type {
  TenantCryptoKey,
  TenantCryptoKeyCreateResponse,
  TenantEncryptionMode,
  PayloadCodec,
} from "@/types/api/tenant";
import type { LicenseStatus } from "@/types/api/authorizations";
import { logger } from "@/utils/sys";
import { getStatusLabel } from "@/utils/display-labels";
import type { ColumnOption } from "@/types/component";
import SurfaceCardHeader from "@/components/common/surface-card-header/index.vue";
import CommonSwitchRow from "@/components/common/switch-row/index.vue";
import CommonInlineRow from "@/components/common/inline-row/index.vue";
import StatusIndicator from "@/components/common/StatusIndicator.vue";
import CenteredDialog from "@/components/common/CenteredDialog.vue";
import CommonFormGrid from "@/components/common/form-grid/index.vue";

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
const rootStatus = reactive<LicenseStatus>({
  status: "unconfigured",
  active: false,
  issuanceAllowed: false,
  decryptionAllowed: false,
  sequence: null,
  keyVersion: null,
  customerId: null,
  installationId: null,
  notBefore: null,
  expiresAt: null,
  graceUntil: null,
  secondsToExpiry: null,
  secondsToGraceEnd: null,
});
const form = reactive({
  encryptionMode: "none" as TenantEncryptionMode,
  requireSignature: false,
  currentKeyVersion: undefined as string | undefined,
  requestCodec: "json" as PayloadCodec,
  responseCodec: "json" as PayloadCodec,
  codecEnabled: false,
  codecVersion: 1,
  nonceTtlSeconds: 300,
  clockSkewSeconds: 300,
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

const codecOptions = computed(() => [
  { label: "JSON", value: "json" as const },
  { label: "Fury", value: "fury" as const },
]);

const activeKeys = computed(() =>
  keys.value.filter((key) => key.status === "active"),
);
const tenantCryptoEnabled = computed(() => rootStatus.issuanceAllowed === true);

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
    if (mode !== "full" && mode !== "request") {
      form.codecEnabled = false;
    }
    if (mode === "none") {
      form.requireSignature = false;
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

const loadData = async () => {
  loading.value = true;
  try {
    const [profileResult, keyListResult, rootResult] = await Promise.allSettled(
      [
        fetchTenantCryptoProfile(props.tenantCode),
        fetchTenantCryptoKeys(props.tenantCode),
        fetchLicenseStatus(),
      ],
    );
    if (profileResult.status === "fulfilled") {
      const profile = profileResult.value;
      form.encryptionMode = profile.encryption_mode;
      form.requireSignature = profile.encryption_mode === 'none' ? false : profile.require_signature;
      form.currentKeyVersion = profile.current_key_version;
      form.requestCodec = profile.request_codec;
      form.responseCodec = profile.response_codec;
      form.codecEnabled = profile.codec_enabled;
      form.codecVersion = profile.codec_version;
      form.nonceTtlSeconds = profile.nonce_ttl_seconds;
      form.clockSkewSeconds = profile.clock_skew_seconds;
    }
    if (keyListResult.status === "fulfilled") {
      keys.value = keyListResult.value.keys;
    }
    if (rootResult.status === "fulfilled") {
      Object.assign(rootStatus, rootResult.value);
    }
    isInitialized.value = true;
  } catch (error) {
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
  if (!tenantCryptoEnabled.value) return;
  creating.value = true;
  try {
    showSecret(await createTenantCryptoKey(props.tenantCode));
    await loadData();
  } finally {
    creating.value = false;
  }
};

const rotateKey = async () => {
  if (!tenantCryptoEnabled.value) return;
  rotating.value = true;
  try {
    showSecret(await rotateTenantCryptoKey(props.tenantCode));
    await loadData();
  } finally {
    rotating.value = false;
  }
};

const setStatus = async (version: string, action: "activate" | "disable") => {
  if (!tenantCryptoEnabled.value && action !== "disable") return;
  if (action === "activate")
    await activateTenantCryptoKey(props.tenantCode, version);
  if (action === "disable")
    await disableTenantCryptoKey(props.tenantCode, version);
  await loadData();
};

const handleDelete = async (version: string) => {
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
  if (!tenantCryptoEnabled.value) return;
  saving.value = true;
  try {
    await updateTenantCryptoProfile(props.tenantCode, { ...form });
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
  background: var(--el-fill-color-lighter);
  border: 1px solid var(--el-border-color-light);
  border-radius: var(--custom-radius);
  padding: 16px 20px;
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

.form-row-item.encryption-mode-section {
  height: auto;
  align-items: flex-start;
  flex-direction: column;
  gap: 12px;

  .form-row-label {
    margin-bottom: 4px;
  }
}

.encryption-mode-content {
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
