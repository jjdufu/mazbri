<template>
  <div class="certificate-detail-page">
    <ElPageHeader @back="router.back()">
      <template #content>
        <span class="text-lg font-medium">{{
          $t("certificate.detail.title")
        }}</span>
      </template>
      <template #extra>
        <ElTag :type="getStatusType(certificate?.status || 'valid')">
          {{ getStatusLabel(certificate?.status || "valid") }}
        </ElTag>
      </template>
    </ElPageHeader>

    <ElAlert
      v-if="authorizationDisabled"
      class="mt-4"
      :title="authorizationDisabledReason"
      type="warning"
      :closable="false"
      show-icon
    />

    <ElRow :gutter="20" class="mt-5">
      <ElCol :sm="24" :md="16" :lg="16">
        <ElCard :loading="loading" shadow="never" class="surface-settings-card">
          <template #header>
            <SurfaceCardHeader :title="$t('certificate.detail.sections.info')">
              <template #actions>
                <AuthorizationTooltipButton
                  type="primary"
                  icon="ri:download-line"
                  :disabled="authorizationDisabled || !certificate"
                  :disabled-reason="
                    authorizationDisabled
                      ? authorizationDisabledReason
                      : certificate
                        ? undefined
                        : $t('certificate.detail.disabledReasons.notLoaded')
                  "
                  @click="handleDownload"
                >
                  {{ $t("certificate.detail.actions.download") }}
                </AuthorizationTooltipButton>
                <AuthorizationTooltipButton
                  type="warning"
                  :disabled="
                    authorizationDisabled || certificate?.status === 'revoked'
                  "
                  :disabled-reason="
                    authorizationDisabled
                      ? authorizationDisabledReason
                      : certificate?.status === 'revoked'
                        ? $t('certificate.detail.disabledReasons.revoked')
                        : undefined
                  "
                  @click="handleRenew"
                >
                  {{ $t("certificate.detail.actions.renew") }}
                </AuthorizationTooltipButton>
                <AuthorizationTooltipButton
                  type="success"
                  :disabled="
                    authorizationDisabled || certificate?.status === 'revoked'
                  "
                  :disabled-reason="
                    authorizationDisabled
                      ? authorizationDisabledReason
                      : certificate?.status === 'revoked'
                        ? $t('certificate.detail.disabledReasons.revoked')
                        : undefined
                  "
                  @click="handleAutoRenewal"
                >
                  {{ $t("certificate.detail.actions.autoRenewal") }}
                </AuthorizationTooltipButton>
                <AuthorizationTooltipButton
                  type="danger"
                  :disabled="
                    authorizationDisabled || certificate?.status === 'revoked'
                  "
                  :disabled-reason="
                    authorizationDisabled
                      ? authorizationDisabledReason
                      : certificate?.status === 'revoked'
                        ? $t('certificate.detail.disabledReasons.revoked')
                        : undefined
                  "
                  @click="handleRevoke"
                >
                  {{ $t("certificate.detail.actions.revoke") }}
                </AuthorizationTooltipButton>
              </template>
            </SurfaceCardHeader>
          </template>

          <ElDescriptions :column="2" border>
            <ElDescriptionsItem :label="$t('certificate.detail.fields.id')">{{
              certificate?.id
            }}</ElDescriptionsItem>
            <ElDescriptionsItem
              :label="$t('certificate.detail.fields.subject')"
              >{{ certificate?.subject }}</ElDescriptionsItem
            >
            <ElDescriptionsItem
              :label="$t('certificate.detail.fields.dnsNames')"
              :span="2"
            >
              <ElTag
                v-for="(name, idx) in certificate?.dns_names"
                :key="idx"
                size="small"
                class="mr-1"
              >
                {{ name }}
              </ElTag>
            </ElDescriptionsItem>
            <ElDescriptionsItem
              :label="$t('certificate.detail.fields.issuedAt')"
              >{{
                formatDate(certificate?.issued_at || "")
              }}</ElDescriptionsItem
            >
            <ElDescriptionsItem
              :label="$t('certificate.detail.fields.expiresAt')"
            >
              <span
                :class="{
                  'text-red-500': isExpiringSoon(certificate?.expires_at || ''),
                }"
              >
                {{ formatDate(certificate?.expires_at || "") }}
              </span>
            </ElDescriptionsItem>
            <ElDescriptionsItem
              :label="$t('certificate.detail.fields.status')"
              :span="2"
            >
              <ElTag :type="getStatusType(certificate?.status || 'valid')">
                {{ getStatusLabel(certificate?.status || "valid") }}
              </ElTag>
            </ElDescriptionsItem>
          </ElDescriptions>
        </ElCard>

        <ElCard shadow="never" class="mt-5 surface-settings-card">
          <template #header>
            <span>{{ $t("certificate.detail.sections.files") }}</span>
          </template>
          <ElTabs>
            <ElTabPane :label="$t('certificate.detail.tabs.certPem')">
              <ElInput v-model="certPem" type="textarea" :rows="10" readonly />
            </ElTabPane>
            <ElTabPane :label="$t('certificate.detail.tabs.keyPem')">
              <ElInput v-model="keyPem" type="textarea" :rows="10" readonly />
            </ElTabPane>
            <ElTabPane
              v-if="caPem"
              :label="$t('certificate.detail.tabs.caPem')"
            >
              <ElInput v-model="caPem" type="textarea" :rows="10" readonly />
            </ElTabPane>
          </ElTabs>
        </ElCard>
      </ElCol>

      <ElCol :sm="24" :md="8" :lg="8">
        <ElCard shadow="never" class="surface-settings-card">
          <template #header>
            <span>{{ $t("certificate.detail.sections.statusLegend") }}</span>
          </template>
          <CommonStack direction="vertical" fill gap="12px">
            <CommonInlineRow gap="0">
              <ElTag type="success">{{
                $t("certificate.detail.statusLegend.valid.label")
              }}</ElTag>
              <CommonInlineHint class="ml-2" size="sm">
                {{ $t("certificate.detail.statusLegend.valid.description") }}
              </CommonInlineHint>
            </CommonInlineRow>
            <CommonInlineRow gap="0">
              <ElTag type="warning">{{
                $t("certificate.detail.statusLegend.expiringSoon.label")
              }}</ElTag>
              <CommonInlineHint class="ml-2" size="sm">
                {{
                  $t("certificate.detail.statusLegend.expiringSoon.description")
                }}
              </CommonInlineHint>
            </CommonInlineRow>
            <CommonInlineRow gap="0">
              <ElTag type="danger">{{
                $t("certificate.detail.statusLegend.expired.label")
              }}</ElTag>
              <CommonInlineHint class="ml-2" size="sm">
                {{ $t("certificate.detail.statusLegend.expired.description") }}
              </CommonInlineHint>
            </CommonInlineRow>
            <CommonInlineRow gap="0">
              <ElTag type="info">{{
                $t("certificate.detail.statusLegend.revoked.label")
              }}</ElTag>
              <CommonInlineHint class="ml-2" size="sm">
                {{ $t("certificate.detail.statusLegend.revoked.description") }}
              </CommonInlineHint>
            </CommonInlineRow>
          </CommonStack>
        </ElCard>
      </ElCol>
    </ElRow>

    <AutoRenewalDialog
      v-model="autoRenewalVisible"
      :certificate-id="certificate?.id || ''"
      :certificate-subject="certificate?.subject || ''"
      :certificate-expires-at="certificate?.expires_at || ''"
      :authorization-disabled="authorizationDisabled"
      :authorization-disabled-reason="authorizationDisabledReason"
      @success="loadData"
    />
  </div>
</template>

<script setup lang="ts">
import { ElMessageBox, ElMessage } from "element-plus";
import { $t } from "@/locales";
import type {
  FullCertificateResponse,
  CertificateStatus,
} from "@/types/api/certificate";
import {
  fetchCertificateById,
  renewCertificate,
  revokeCertificate,
} from "@/api/certificate";
import { getStatusType, getStatusLabel } from "@/utils/display-labels";
import { formatDateTime as formatDate } from "@/utils/format";
import { logger } from "@/utils/sys";
import CommonStack from "@/components/common/stack/index.vue";
import CommonInlineRow from "@/components/common/inline-row/index.vue";
import CommonInlineHint from "@/components/common/inline-hint/index.vue";
import SurfaceCardHeader from "@/components/common/surface-card-header/index.vue";
import AuthorizationTooltipButton from "@/components/common/authorization-tooltip-button/index.vue";
import { useLicenseGate } from "@/composables/useLicenseGate";
import AutoRenewalDialog from "../modules/auto-renewal-dialog.vue";
defineOptions({ name: "CertificateDetail" });
const router = useRouter();
const route = useRoute();
const loading = ref(false);
const certificate = ref<FullCertificateResponse | null>(null);
const certPem = ref("");
const keyPem = ref("");
const caPem = ref("");
const autoRenewalVisible = ref(false);
const {
  authorizationDisabled,
  authorizationDisabledReason,
  loadLicenseStatus,
  ensureLicenseActive,
} = useLicenseGate();
const certId = computed(() => route.params.id as string);
const isExpiringSoon = (expiresAt: string) => {
  if (!expiresAt) return false;
  const expiry = new Date(expiresAt);
  const now = new Date();
  const daysUntilExpiry =
    (expiry.getTime() - now.getTime()) / (1000 * 60 * 60 * 24);
  return daysUntilExpiry < 30;
};
const loadData = async () => {
  loading.value = true;
  try {
    await loadLicenseStatus();
    if (authorizationDisabled.value) {
      certificate.value = null;
      certPem.value = "";
      keyPem.value = "";
      caPem.value = "";
      return;
    }
    const res = await fetchCertificateById(certId.value);
    certificate.value = res;
    certPem.value = res.cert_pem;
    keyPem.value = res.key_pem;
    caPem.value = res.ca_cert_pem || "";
  } catch (error) {
    logger.error("Failed to load certificate:", error);
  } finally {
    loading.value = false;
  }
};
const handleDownload = () => {
  if (!ensureLicenseActive() || !certificate.value) return;
  const content = `=== cert.pem ===\n${certPem.value}\n\n=== key.pem ===\n${keyPem.value}\n${caPem.value ? `\n=== ca.pem ===\n${caPem.value}` : ""}`;
  const blob = new Blob([content], { type: "text/plain" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = `${certificate.value.subject || "certificate"}-certificates.txt`;
  a.click();
  URL.revokeObjectURL(url);
};
const handleRenew = async () => {
  if (!ensureLicenseActive()) {
    return;
  }
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
    const days = parseInt(validityDays, 10);
    if (days < 1 || days > 36135) {
      ElMessage.error($t("certificate.messages.renewDaysRange"));
      return;
    }
    await renewCertificate(certId.value, { validity_days: days });
    ElMessage.success($t("common.feedback.renewSuccess"));
    loadData();
  } catch (error) {
    if (error !== "cancel") {
      logger.error("Failed to renew certificate:", error);
    }
  }
};
const handleAutoRenewal = () => {
  if (
    !ensureLicenseActive() ||
    certificate.value?.status === "revoked"
  ) {
    return;
  }
  autoRenewalVisible.value = true;
};
const handleRevoke = async () => {
  if (!ensureLicenseActive()) {
    return;
  }
  try {
    await ElMessageBox.confirm(
      $t("certificate.messages.revokeCurrentConfirm"),
      $t("common.warning"),
      {
        confirmButtonText: $t("certificate.messages.confirmRevoke"),
        cancelButtonText: $t("common.cancel"),
        type: "warning",
      },
    );
    await revokeCertificate(certId.value);
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
});
onActivated(() => {
  loadData();
});
</script>
