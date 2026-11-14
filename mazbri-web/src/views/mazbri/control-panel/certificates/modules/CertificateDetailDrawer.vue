<template>
  <AdaptiveDrawer v-model="visible" size="680px" destroy-on-close>
    <template #header>
      <CommonDrawerHeader
        :title="headerTitle"
        :subtitle="headerSubtitle"
        align="center"
      >
        <template #actions>
          <ElTooltip
            :disabled="!renewDisabledReason"
            :content="renewDisabledReason"
          >
            <span>
              <ElButton
                type="primary"
                plain
                :loading="renewing"
                :disabled="Boolean(renewDisabledReason)"
                @click="handleRenew"
              >
                {{ $t("certificate.detail.actions.renew") }}
              </ElButton>
            </span>
          </ElTooltip>

          <ElDropdown
            trigger="click"
            :disabled="authorizationDisabled || !cert"
            @command="handleDownloadCommand"
          >
            <ElButton
              plain
              :loading="downloadingBundle"
              :disabled="authorizationDisabled || !cert"
            >
              {{ $t("certificate.detail.actions.download") }}
              <ElIcon class="el-icon--right"><ArrowDown /></ElIcon>
            </ElButton>
            <template #dropdown>
              <ElDropdownMenu>
                <ElDropdownItem command="cert">
                  {{ $t("certificate.detail.actions.downloadPublicCert") }}
                </ElDropdownItem>
                <ElDropdownItem command="bundle">
                  {{ $t("certificate.detail.actions.downloadFullBundle") }}
                </ElDropdownItem>
              </ElDropdownMenu>
            </template>
          </ElDropdown>

          <ElTooltip
            v-if="!cert || cert.status !== 'revoked'"
            :disabled="!authorizationDisabled"
            :content="authorizationDisabledReason"
          >
            <span>
              <ElButton
                type="danger"
                plain
                :loading="revoking"
                :disabled="authorizationDisabled || !cert"
                @click="handleRevoke"
              >
                {{ $t("certificate.detail.actions.revoke") }}
              </ElButton>
            </span>
          </ElTooltip>
        </template>
      </CommonDrawerHeader>
    </template>

    <div v-loading="loading" class="cert-detail-drawer">
      <ElAlert
        v-if="authorizationDisabled"
        class="drawer-alert"
        :title="authorizationDisabledReason"
        type="warning"
        :closable="false"
        show-icon
      />

      <template v-if="cert">
        <!-- 1. 证书基础元数据 -->
        <section class="drawer-section">
          <div class="drawer-section__title">
            {{ $t("certificate.detail.sections.info") }}
          </div>
          <ElDescriptions :column="2" border size="small">
            <ElDescriptionsItem :label="$t('certificate.detail.fields.id')">
              <CopyableText
                :value="cert.id"
                :label="$t('certificate.detail.fields.id')"
              />
            </ElDescriptionsItem>
            <ElDescriptionsItem :label="$t('certificate.detail.fields.status')">
              <ElTag
                :type="getStatusType(cert.status)"
                effect="light"
                size="small"
              >
                {{ getStatusLabel(cert.status) }}
              </ElTag>
            </ElDescriptionsItem>
            <ElDescriptionsItem :label="$t('certificate.detail.fields.subject')">
              <CopyableText
                :value="cert.subject"
                :label="$t('certificate.detail.fields.subject')"
              />
            </ElDescriptionsItem>
            <ElDescriptionsItem
              :label="$t('certificate.detail.fields.renewalCount')"
            >
              {{ cert.renewal_count }}
            </ElDescriptionsItem>
            <ElDescriptionsItem
              :label="$t('certificate.detail.fields.dnsNames')"
              :span="2"
            >
              <div v-if="cert.dns_names?.length" class="san-tag-list">
                <ElTag
                  v-for="name in cert.dns_names"
                  :key="name"
                  size="small"
                  effect="plain"
                >
                  {{ name }}
                </ElTag>
              </div>
              <span v-else>{{ EMPTY_PLACEHOLDER }}</span>
            </ElDescriptionsItem>
            <ElDescriptionsItem
              :label="$t('certificate.detail.fields.fingerprint')"
              :span="2"
            >
              <div v-if="cert.fingerprint" class="fingerprint-full-row">
                <code class="fingerprint-full-row__code">{{
                  cert.fingerprint
                }}</code>
                <ElButton
                  link
                  type="primary"
                  :icon="CopyDocument"
                  :aria-label="$t('certificate.detail.actions.copyFingerprint')"
                  @click="handleCopyText(cert.fingerprint)"
                />
              </div>
              <span v-else>{{ EMPTY_PLACEHOLDER }}</span>
            </ElDescriptionsItem>
            <ElDescriptionsItem
              :label="$t('certificate.detail.fields.issuedAt')"
            >
              {{ formatDate(cert.issued_at) }}
            </ElDescriptionsItem>
            <ElDescriptionsItem
              :label="$t('certificate.detail.fields.expiresAt')"
            >
              <span :class="expiryToneClass">
                {{ formatDate(cert.expires_at) }}
              </span>
            </ElDescriptionsItem>
            <ElDescriptionsItem
              v-if="cert.revoked && cert.revoked_at"
              :label="$t('certificate.detail.fields.revokedAt')"
              :span="2"
            >
              {{ formatDate(cert.revoked_at) }}
            </ElDescriptionsItem>
          </ElDescriptions>
        </section>

        <!-- 2. 内联自动续期策略 -->
        <section class="drawer-section">
          <div class="auto-renew-card">
            <div class="auto-renew-card__header">
              <div class="auto-renew-card__meta">
                <div class="auto-renew-card__title">
                  {{ $t("certificate.detail.autoRenewal.title") }}
                </div>
                <div class="auto-renew-card__desc">
                  {{ $t("certificate.detail.autoRenewal.subtitle") }}
                </div>
              </div>
              <div class="auto-renew-card__switch">
                <span class="auto-renew-card__switch-label">
                  {{
                    autoRenewForm.enabled
                      ? $t("certificate.detail.autoRenewal.enabledTag")
                      : $t("certificate.detail.autoRenewal.disabledTag")
                  }}
                </span>
                <ElSwitch
                  v-model="autoRenewForm.enabled"
                  :disabled="authorizationDisabled || cert.status === 'revoked'"
                />
              </div>
            </div>

            <ElForm
              v-if="autoRenewForm.enabled"
              ref="autoRenewFormRef"
              :model="autoRenewForm"
              :rules="autoRenewRules"
              label-position="top"
              class="auto-renew-card__body"
            >
              <div class="auto-renew-grid">
                <ElFormItem
                  :label="$t('certificate.detail.autoRenewal.thresholdDays')"
                  prop="threshold_days"
                >
                  <ElInputNumber
                    v-model="autoRenewForm.threshold_days"
                    :min="1"
                    :max="365"
                    :step="1"
                    :disabled="authorizationDisabled || cert.status === 'revoked'"
                    style="width: 100%"
                  />
                  <CommonInlineHint block class="mt-1">
                    {{ $t("certificate.autoRenewalDialog.hints.thresholdDays") }}
                  </CommonInlineHint>
                </ElFormItem>

                <ElFormItem
                  :label="$t('certificate.detail.autoRenewal.validityDays')"
                  prop="validity_days"
                >
                  <ElSelect
                    v-model="autoRenewForm.validity_days"
                    :disabled="authorizationDisabled || cert.status === 'revoked'"
                    style="width: 100%"
                  >
                    <ElOption
                      v-for="days in VALIDITY_DAY_OPTIONS"
                      :key="days"
                      :label="
                        $t(
                          `certificate.autoRenewalDialog.options.validityDays.${days}`,
                        )
                      "
                      :value="days"
                    />
                  </ElSelect>
                  <CommonInlineHint block class="mt-1">
                    {{ $t("certificate.autoRenewalDialog.hints.validityDays") }}
                  </CommonInlineHint>
                </ElFormItem>
              </div>
            </ElForm>

            <div class="auto-renew-card__footer">
              <div class="auto-renew-card__schedule">
                <span class="auto-renew-card__schedule-label">
                  {{ $t("certificate.detail.autoRenewal.nextRenewalAt") }}:
                </span>
                <span class="auto-renew-card__schedule-value">
                  {{
                    autoRenewForm.enabled && nextRenewalAt
                      ? formatDate(nextRenewalAt)
                      : EMPTY_PLACEHOLDER
                  }}
                </span>
              </div>
              <ElButton
                type="primary"
                plain
                size="default"
                :loading="savingAutoRenew"
                :disabled="authorizationDisabled || cert.status === 'revoked'"
                @click="handleSaveAutoRenewal"
              >
                {{ $t("certificate.detail.autoRenewal.saveAction") }}
              </ElButton>
            </div>
          </div>
        </section>

        <!-- 3. 证书 PEM 内容切换区（公钥 / CA 根证书 / 按需解锁私钥） -->
        <section class="drawer-section">
          <div class="drawer-section__title">
            {{ $t("certificate.detail.sections.files") }}
          </div>
          <ElTabs v-model="activePemTab" class="cert-pem-tabs">
            <ElTabPane
              name="cert"
              :label="$t('certificate.detail.tabs.certPem')"
            >
              <div class="pem-panel">
                <div class="pem-panel__toolbar">
                  <span class="pem-panel__hint">
                    {{ $t("certificate.detail.pem.publicCertHint") }}
                  </span>
                  <ElButton
                    size="small"
                    plain
                    :icon="CopyDocument"
                    :disabled="!cert.cert_pem"
                    @click="handleCopyText(cert.cert_pem)"
                  >
                    {{ $t("common.actions.copy") }}
                  </ElButton>
                </div>
                <pre class="pem-code-block">{{ cert.cert_pem }}</pre>
              </div>
            </ElTabPane>

            <ElTabPane name="ca" :label="$t('certificate.detail.tabs.caPem')">
              <div v-loading="loadingCa" class="pem-panel">
                <div class="pem-panel__toolbar">
                  <span class="pem-panel__hint">
                    {{ $t("certificate.detail.pem.caCertHint") }}
                  </span>
                  <ElButton
                    size="small"
                    plain
                    :icon="CopyDocument"
                    :disabled="!caPem"
                    @click="handleCopyText(caPem)"
                  >
                    {{ $t("common.actions.copy") }}
                  </ElButton>
                </div>
                <pre class="pem-code-block">{{ caPem || EMPTY_PLACEHOLDER }}</pre>
              </div>
            </ElTabPane>

            <ElTabPane name="key" :label="$t('certificate.detail.tabs.keyPem')">
              <div v-if="!keyPem" class="key-locked-card">
                <div class="key-locked-card__icon">
                  <ArtSvgIcon icon="ri:shield-keyhole-line" />
                </div>
                <div class="key-locked-card__title">
                  {{ $t("certificate.detail.privateKeyGuard.title") }}
                </div>
                <div class="key-locked-card__desc">
                  {{ $t("certificate.detail.privateKeyGuard.description") }}
                </div>
                <ElButton
                  type="primary"
                  :loading="unlockingKey"
                  :disabled="authorizationDisabled"
                  @click="handleUnlockPrivateKey"
                >
                  {{ $t("certificate.detail.privateKeyGuard.unlockAction") }}
                </ElButton>
              </div>
              <div v-else class="pem-panel">
                <div class="pem-panel__toolbar">
                  <span class="pem-panel__hint pem-panel__hint--warning">
                    {{ $t("certificate.detail.privateKeyGuard.unlockedHint") }}
                  </span>
                  <div class="pem-panel__actions">
                    <ElButton
                      size="small"
                      plain
                      @click="clearUnlockedPrivateKey"
                    >
                      {{ $t("certificate.detail.privateKeyGuard.lockAction") }}
                    </ElButton>
                    <ElButton
                      size="small"
                      type="primary"
                      plain
                      :icon="CopyDocument"
                      @click="handleCopyText(keyPem)"
                    >
                      {{ $t("common.actions.copy") }}
                    </ElButton>
                  </div>
                </div>
                <pre class="pem-code-block">{{ keyPem }}</pre>
              </div>
            </ElTabPane>
          </ElTabs>
        </section>
      </template>
    </div>
  </AdaptiveDrawer>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, reactive, ref, watch } from "vue";
import {
  ElMessage,
  ElMessageBox,
  type FormInstance,
  type FormRules,
} from "element-plus";
import { ArrowDown, CopyDocument } from "@element-plus/icons-vue";
import { useI18n } from "vue-i18n";
import { $t } from "@/locales";
import type {
  AutoRenewalConfig,
  Certificate,
  PublicCertificateDetailResponse,
} from "@/types/api/certificate";
import {
  fetchCaCertificate,
  fetchCertificateDetail,
  fetchCertificateFullWithPrivateKey,
  getAutoRenewalConfig,
  renewCertificate,
  revokeCertificate,
  updateAutoRenewal,
} from "@/api/certificate";
import AdaptiveDrawer from "@/components/common/AdaptiveDrawer.vue";
import CommonDrawerHeader from "@/components/common/drawer-header/index.vue";
import CommonInlineHint from "@/components/common/inline-hint/index.vue";
import CopyableText from "@/components/common/copyable-text/index.vue";
import { runWithLoading } from "@/composables/useAsyncState";
import { getStatusLabel, getStatusType } from "@/utils/display-labels";
import {
  EMPTY_PLACEHOLDER,
  formatDateTime as formatDate,
} from "@/utils/format";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-dialog-service";
import { logger } from "@/utils/sys";
import { copyWithFeedback } from "@/utils/ui/copy-feedback";

defineOptions({ name: "CertificateDetailDrawer" });

const VALIDITY_DAY_OPTIONS = [90, 180, 365, 730, 1095] as const;

const visible = defineModel<boolean>({ required: true });
const props = withDefaults(
  defineProps<{
    certId: string | null;
    initialCert?: Certificate | null;
    authorizationDisabled?: boolean;
    authorizationDisabledReason?: string;
  }>(),
  {
    initialCert: null,
    authorizationDisabled: false,
    authorizationDisabledReason: "",
  },
);

const emit = defineEmits<{
  changed: [];
}>();

const { t } = useI18n();
const loading = ref(false);
const loadingCa = ref(false);
const unlockingKey = ref(false);
const downloadingBundle = ref(false);
const renewing = ref(false);
const revoking = ref(false);
const savingAutoRenew = ref(false);

const cert = ref<PublicCertificateDetailResponse | null>(null);
const activePemTab = ref<"cert" | "ca" | "key">("cert");
const caPem = ref("");
const keyPem = ref("");

const autoRenewFormRef = ref<FormInstance>();
const nextRenewalAt = ref<string | null>(null);
const autoRenewForm = reactive<AutoRenewalConfig>({
  enabled: false,
  threshold_days: 30,
  validity_days: 365,
});

const autoRenewRules = computed<FormRules>(() => ({
  threshold_days: [
    {
      required: true,
      message: t(
        "certificate.autoRenewalDialog.validation.thresholdDaysRequired",
      ),
      trigger: "blur",
    },
    {
      type: "number",
      min: 1,
      max: 365,
      message: t(
        "certificate.autoRenewalDialog.validation.thresholdDaysRange",
      ),
      trigger: "blur",
    },
  ],
  validity_days: [
    {
      required: true,
      message: t(
        "certificate.autoRenewalDialog.validation.validityDaysRequired",
      ),
      trigger: "change",
    },
  ],
}));

const headerTitle = computed(
  () =>
    cert.value?.subject ||
    props.initialCert?.subject ||
    t("certificate.detail.title"),
);

const headerSubtitle = computed(
  () => cert.value?.id || props.certId || undefined,
);

const renewDisabledReason = computed(() => {
  if (props.authorizationDisabled) return props.authorizationDisabledReason;
  if (!cert.value) return t("certificate.detail.disabledReasons.notLoaded");
  if (cert.value.status === "revoked") {
    return t("certificate.detail.disabledReasons.revoked");
  }
  return "";
});

const expiryToneClass = computed(() => {
  if (!cert.value) return "";
  if (cert.value.status === "expired") return "expiry-text--danger";
  if (cert.value.status === "expiring_soon") return "expiry-text--warning";
  return "";
});

function clearUnlockedPrivateKey(): void {
  keyPem.value = "";
}

function resetDrawerState(): void {
  cert.value = null;
  caPem.value = "";
  keyPem.value = "";
  activePemTab.value = "cert";
  nextRenewalAt.value = null;
  autoRenewForm.enabled = false;
  autoRenewForm.threshold_days = 30;
  autoRenewForm.validity_days = 365;
}

async function loadPublicDrawerData(id: string): Promise<void> {
  clearUnlockedPrivateKey();
  if (props.authorizationDisabled) {
    cert.value = null;
    return;
  }
  await runWithLoading(loading, async () => {
    const [detail, autoRenew] = await Promise.all([
      fetchCertificateDetail(id),
      getAutoRenewalConfig(id).catch(() => null),
    ]);
    cert.value = detail;
    if (autoRenew) {
      autoRenewForm.enabled = autoRenew.enabled;
      autoRenewForm.threshold_days = autoRenew.threshold_days;
      autoRenewForm.validity_days = autoRenew.validity_days;
      nextRenewalAt.value = autoRenew.next_renewal_at;
    } else {
      autoRenewForm.enabled = false;
      autoRenewForm.threshold_days = 30;
      autoRenewForm.validity_days = 365;
      nextRenewalAt.value = null;
    }
  }).catch((error) => {
    logger.error("Failed to load public certificate detail:", error);
  });
}

async function ensureCaLoaded(): Promise<string> {
  if (caPem.value) return caPem.value;
  if (props.authorizationDisabled) return "";
  await runWithLoading(loadingCa, async () => {
    const res = await fetchCaCertificate();
    caPem.value = res.ca_cert_pem || "";
  }).catch((error) => {
    logger.error("Failed to load CA certificate:", error);
  });
  return caPem.value;
}

async function handleUnlockPrivateKey(): Promise<void> {
  if (!cert.value || props.authorizationDisabled || unlockingKey.value) return;
  await runWithLoading(unlockingKey, async () => {
    const full = await fetchCertificateFullWithPrivateKey(cert.value!.id);
    keyPem.value = full.key_pem;
    if (full.ca_cert_pem && !caPem.value) {
      caPem.value = full.ca_cert_pem;
    }
  }).catch((error) => {
    if (!isMfaStepUpCancelled(error)) {
      logger.error("Failed to unlock certificate private key:", error);
    }
  });
}

function triggerBlobDownload(filename: string, content: string): void {
  const blob = new Blob([content], { type: "application/x-pem-file" });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  URL.revokeObjectURL(url);
}

async function handleDownloadCommand(
  command: "cert" | "bundle",
): Promise<void> {
  if (!cert.value || props.authorizationDisabled) return;
  const safeSubject = cert.value.subject || "certificate";
  if (command === "cert") {
    triggerBlobDownload(`${safeSubject}-cert.pem`, cert.value.cert_pem);
    return;
  }
  await runWithLoading(downloadingBundle, async () => {
    const full = await fetchCertificateFullWithPrivateKey(cert.value!.id);
    const bundleParts = [
      `=== cert.pem ===\n${full.cert_pem}`,
      `=== key.pem ===\n${full.key_pem}`,
    ];
    if (full.ca_cert_pem) {
      bundleParts.push(`=== ca.pem ===\n${full.ca_cert_pem}`);
    }
    triggerBlobDownload(
      `${safeSubject}-certificates.pem`,
      bundleParts.join("\n\n"),
    );
  }).catch((error) => {
    if (!isMfaStepUpCancelled(error)) {
      logger.error("Failed to download full certificate bundle:", error);
    }
  });
}

async function handleSaveAutoRenewal(): Promise<void> {
  if (!cert.value || props.authorizationDisabled || savingAutoRenew.value) {
    return;
  }
  if (autoRenewForm.enabled) {
    const valid = await autoRenewFormRef.value?.validate().catch(() => false);
    if (!valid) return;
  }
  await runWithLoading(savingAutoRenew, async () => {
    await updateAutoRenewal(cert.value!.id, {
      enabled: autoRenewForm.enabled,
      threshold_days: autoRenewForm.threshold_days,
      validity_days: autoRenewForm.validity_days,
    });
    const refreshed = await getAutoRenewalConfig(cert.value!.id).catch(
      () => null,
    );
    if (refreshed) {
      nextRenewalAt.value = refreshed.next_renewal_at;
    }
    ElMessage.success(
      autoRenewForm.enabled
        ? $t("common.feedback.autoRenewEnabled")
        : $t("common.feedback.autoRenewDisabled"),
    );
    emit("changed");
  }).catch((error) => {
    logger.error("Failed to update auto-renewal policy:", error);
  });
}

async function handleRenew(): Promise<void> {
  if (!cert.value || props.authorizationDisabled || renewing.value) return;
  try {
    const { value: validityDays } = await ElMessageBox.prompt(
      $t("certificate.messages.renewPromptMessage"),
      $t("certificate.messages.renewPromptTitle"),
      {
        confirmButtonText: $t("certificate.messages.confirmRenew"),
        cancelButtonText: $t("common.cancel"),
        inputPattern: /^[1-9]\d*$/,
        inputErrorMessage: $t("certificate.messages.renewPromptError"),
        inputValue: "365",
        inputPlaceholder: $t("certificate.messages.renewPromptPlaceholder"),
      },
    );
    const days = Number.parseInt(validityDays, 10);
    if (days < 1 || days > 36135) {
      ElMessage.error($t("certificate.messages.renewDaysRange"));
      return;
    }
    await runWithLoading(renewing, async () => {
      await renewCertificate(cert.value!.id, { validity_days: days });
      clearUnlockedPrivateKey();
      await loadPublicDrawerData(cert.value!.id);
      emit("changed");
    });
  } catch (error) {
    if (error !== "cancel" && !isMfaStepUpCancelled(error)) {
      logger.error("Failed to renew certificate:", error);
    }
  }
}

async function handleRevoke(): Promise<void> {
  if (!cert.value || props.authorizationDisabled || revoking.value) return;
  try {
    await ElMessageBox.confirm(
      $t("certificate.messages.revokeConfirm", { name: cert.value.subject }),
      $t("common.warning"),
      {
        confirmButtonText: $t("certificate.messages.confirmRevoke"),
        cancelButtonText: $t("common.cancel"),
        type: "warning",
      },
    );
    await runWithLoading(revoking, async () => {
      await revokeCertificate(cert.value!.id);
      clearUnlockedPrivateKey();
      await loadPublicDrawerData(cert.value!.id);
      emit("changed");
    });
  } catch (error) {
    if (error !== "cancel" && !isMfaStepUpCancelled(error)) {
      logger.error("Failed to revoke certificate:", error);
    }
  }
}

function handleCopyText(text: string | null | undefined): void {
  if (!text) return;
  void copyWithFeedback(text);
}

watch(activePemTab, (tab) => {
  if (tab === "ca") {
    void ensureCaLoaded();
  }
});

watch(
  [visible, () => props.certId],
  ([isOpen, id]) => {
    if (!isOpen || !id) {
      resetDrawerState();
      return;
    }
    activePemTab.value = "cert";
    void loadPublicDrawerData(id);
  },
  { immediate: true },
);

onBeforeUnmount(() => {
  clearUnlockedPrivateKey();
});
</script>

<style scoped lang="scss">
.cert-detail-drawer {
  display: flex;
  flex-direction: column;
  gap: 20px;
  min-height: 220px;
}

.drawer-alert {
  margin-bottom: 4px;
}

.drawer-section {
  display: flex;
  flex-direction: column;
  gap: 10px;

  &__title {
    font-size: 14px;
    font-weight: 600;
    color: var(--el-text-color-primary);
  }
}

.san-tag-list {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.fingerprint-full-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;

  &__code {
    font-family: var(--el-font-family-monospace, ui-monospace, SFMono-Regular, Menlo, monospace);
    font-size: 12px;
    color: var(--el-text-color-regular);
    word-break: break-all;
  }
}

.expiry-text--warning {
  color: var(--el-color-warning);
  font-weight: 500;
}

.expiry-text--danger {
  color: var(--el-color-danger);
  font-weight: 500;
}

.auto-renew-card {
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding: 14px 16px;
  border: 1px solid var(--el-border-color-lighter);
  border-radius: var(--el-border-radius-base);
  background-color: var(--el-fill-color-blank);

  &__header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
  }

  &__meta {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  &__title {
    font-size: 14px;
    font-weight: 600;
    color: var(--el-text-color-primary);
  }

  &__desc {
    font-size: 12px;
    color: var(--el-text-color-secondary);
  }

  &__switch {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }

  &__switch-label {
    font-size: 12px;
    color: var(--el-text-color-secondary);
  }

  &__body {
    padding-top: 12px;
    border-top: 1px dashed var(--el-border-color-lighter);

    :deep(.el-form-item) {
      margin-bottom: 0;
    }
  }

  &__footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding-top: 10px;
    border-top: 1px solid var(--el-border-color-extra-light);
  }

  &__schedule {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    min-width: 0;
  }

  &__schedule-label {
    color: var(--el-text-color-secondary);
  }

  &__schedule-value {
    color: var(--el-text-color-regular);
    font-variant-numeric: tabular-nums;
  }
}

.auto-renew-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 14px;
}

.pem-panel {
  display: flex;
  flex-direction: column;
  gap: 8px;

  &__toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  &__hint {
    font-size: 12px;
    color: var(--el-text-color-secondary);

    &--warning {
      color: var(--el-color-warning);
    }
  }

  &__actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }
}

.pem-code-block {
  margin: 0;
  padding: 12px;
  max-height: 240px;
  overflow: auto;
  border: 1px solid var(--el-border-color-lighter);
  border-radius: var(--el-border-radius-base);
  background-color: var(--el-fill-color-lighter);
  color: var(--el-text-color-regular);
  font-family: var(--el-font-family-monospace, ui-monospace, SFMono-Regular, Menlo, monospace);
  font-size: 12px;
  line-height: 1.5;
  white-space: pre-wrap;
  word-break: break-all;
}

.key-locked-card {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  padding: 28px 20px;
  text-align: center;
  border: 1px dashed var(--el-border-color);
  border-radius: var(--el-border-radius-base);
  background-color: var(--el-fill-color-lighter);

  &__icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 40px;
    height: 40px;
    border-radius: 50%;
    background-color: var(--el-color-primary-light-9);
    color: var(--el-color-primary);
    font-size: 20px;
  }

  &__title {
    font-size: 14px;
    font-weight: 600;
    color: var(--el-text-color-primary);
  }

  &__desc {
    max-width: 420px;
    font-size: 12px;
    line-height: 1.6;
    color: var(--el-text-color-secondary);
  }
}
</style>
