<template>
  <EditDialog
    v-model="visible"
    :title="$t('certificate.autoRenewalDialog.title')"
    width="500px"
    :confirm-text="$t('common.actions.save')"
    :saving="submitting"
    :confirm-disabled="authorizationDisabled"
    :confirm-disabled-reason="authorizationDisabledReason"
    @confirm="handleSubmit"
  >
    <ElForm
      ref="formRef"
      v-loading="loading"
      :model="form"
      :rules="rules"
      :disabled="authorizationDisabled"
      label-width="120px"
    >
      <ElFormItem :label="$t('certificate.autoRenewalDialog.fields.subject')">
        <span class="text-gray-600">{{ certificateSubject }}</span>
      </ElFormItem>

      <ElFormItem :label="$t('certificate.autoRenewalDialog.fields.expiresAt')">
        <span :class="{ 'text-red-500': isExpiringSoon }">
          {{ formatDate(certificateExpiresAt) }}
        </span>
      </ElFormItem>

      <ElFormItem :label="$t('certificate.autoRenewalDialog.fields.enabled')">
        <ElSwitch v-model="form.enabled" :disabled="authorizationDisabled" />
      </ElFormItem>

      <template v-if="form.enabled">
        <ElFormItem
          :label="$t('certificate.autoRenewalDialog.fields.thresholdDays')"
          prop="threshold_days"
        >
          <ElInputNumber
            v-model="form.threshold_days"
            :min="1"
            :max="365"
            :step="1"
            :disabled="authorizationDisabled"
            style="width: 150px"
          />
          <CommonInlineHint class="ml-2">
            {{ $t("certificate.autoRenewalDialog.hints.thresholdDays") }}
          </CommonInlineHint>
        </ElFormItem>

        <ElFormItem
          :label="$t('certificate.autoRenewalDialog.fields.validityDays')"
          prop="validity_days"
        >
          <ElSelect
            v-model="form.validity_days"
            :disabled="authorizationDisabled"
            style="width: 150px"
          >
            <ElOption
              :label="
                $t('certificate.autoRenewalDialog.options.validityDays.90')
              "
              :value="90"
            />
            <ElOption
              :label="
                $t('certificate.autoRenewalDialog.options.validityDays.180')
              "
              :value="180"
            />
            <ElOption
              :label="
                $t('certificate.autoRenewalDialog.options.validityDays.365')
              "
              :value="365"
            />
            <ElOption
              :label="
                $t('certificate.autoRenewalDialog.options.validityDays.730')
              "
              :value="730"
            />
            <ElOption
              :label="
                $t('certificate.autoRenewalDialog.options.validityDays.1095')
              "
              :value="1095"
            />
          </ElSelect>
          <CommonInlineHint class="ml-2">
            {{ $t("certificate.autoRenewalDialog.hints.validityDays") }}
          </CommonInlineHint>
        </ElFormItem>

        <ElFormItem
          v-if="nextRenewalAt"
          :label="$t('certificate.autoRenewalDialog.fields.nextRenewalAt')"
        >
          <span class="text-gray-600">{{ formatDate(nextRenewalAt) }}</span>
        </ElFormItem>
      </template>
    </ElForm>
  </EditDialog>
</template>

<script setup lang="ts">
import type { FormInstance, FormRules } from "element-plus";
import { ElMessage } from "element-plus";
import { $t } from "@/locales";
import type {
  AutoRenewalConfigResponse,
  AutoRenewalConfig,
} from "@/types/api/certificate";
import { getAutoRenewalConfig, updateAutoRenewal } from "@/api/certificate";
import EditDialog from "@/components/common/EditDialog.vue";
import CommonInlineHint from "@/components/common/inline-hint/index.vue";
import { runWithLoading } from "@/composables/useAsyncState";
import { logger } from "@/utils/sys";
import { formatDateTime as formatDate } from "@/utils/format";
defineOptions({ name: "AutoRenewalDialog" });
const props = defineProps<{
  certificateId: string;
  certificateSubject: string;
  certificateExpiresAt: string;
  authorizationDisabled?: boolean;
  authorizationDisabledReason?: string;
}>();
const visible = defineModel<boolean>({ required: true });
const emit = defineEmits<{
  success: [];
}>();
const formRef = ref<FormInstance>();
const loading = ref(false);
const submitting = ref(false);
const nextRenewalAt = ref<string | null>(null);
const form = reactive<AutoRenewalConfig>({
  enabled: false,
  threshold_days: 30,
  validity_days: 365,
});
const rules: FormRules = {
  threshold_days: [
    {
      required: true,
      message: $t(
        "certificate.autoRenewalDialog.validation.thresholdDaysRequired",
      ),
      trigger: "blur",
    },
    {
      type: "number",
      min: 1,
      max: 365,
      message: $t(
        "certificate.autoRenewalDialog.validation.thresholdDaysRange",
      ),
      trigger: "blur",
    },
  ],
  validity_days: [
    {
      required: true,
      message: $t(
        "certificate.autoRenewalDialog.validation.validityDaysRequired",
      ),
      trigger: "change",
    },
  ],
};
const isExpiringSoon = computed(() => {
  if (!props.certificateExpiresAt) return false;
  const expiry = new Date(props.certificateExpiresAt);
  const now = new Date();
  const daysUntilExpiry =
    (expiry.getTime() - now.getTime()) / (1000 * 60 * 60 * 24);
  return daysUntilExpiry < 30;
});
const loadConfig = async () => {
  if (props.authorizationDisabled) {
    form.enabled = false;
    form.threshold_days = 30;
    form.validity_days = 365;
    nextRenewalAt.value = null;
    return;
  }
  await runWithLoading(loading, async () => {
    const config = await getAutoRenewalConfig(props.certificateId);
    form.enabled = config.enabled;
    form.threshold_days = config.threshold_days;
    form.validity_days = config.validity_days;
    nextRenewalAt.value = config.next_renewal_at;
  }).catch((error) => {
    logger.error("Failed to load auto-renewal config:", error);
  });
};
const handleSubmit = async () => {
  if (props.authorizationDisabled) {
    ElMessage.warning(props.authorizationDisabledReason);
    return;
  }
  const valid = await formRef.value?.validate().catch(() => false);
  if (!valid) return;
  await runWithLoading(submitting, async () => {
    await updateAutoRenewal(props.certificateId, {
      enabled: form.enabled,
      threshold_days: form.threshold_days,
      validity_days: form.validity_days,
    });
    ElMessage.success(
      form.enabled
        ? $t("common.feedback.autoRenewEnabled")
        : $t("common.feedback.autoRenewDisabled"),
    );
    visible.value = false;
    emit("success");
  }).catch((error) => {
    logger.error("Failed to update auto-renewal config:", error);
  });
};
watch(visible, (val) => {
  if (val) {
    loadConfig();
  }
});
</script>
