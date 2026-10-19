<template>
  <EditDialog
    v-model="visible"
    :title="$t('certificate.issueDialog.title')"
    width="600px"
  >
    <ElForm ref="formRef" :model="form" :rules="rules" label-width="100px">
      <ElFormItem :label="$t('certificate.issueDialog.presetTemplate')">
        <ElSelect
          v-model="selectedPreset"
          :placeholder="$t('certificate.issueDialog.selectPresetPlaceholder')"
          clearable
          style="width: 100%"
          @change="applyPreset"
        >
          <ElOption
            v-for="preset in presets"
            :key="preset.id"
            :label="
              preset.service_name || $t('certificate.issueDialog.unnamed')
            "
            :value="preset.id"
            :disabled="isSubjectIssued(preset.service_name)"
          >
            <span
              :class="{
                'line-through text-gray-400': isSubjectIssued(
                  preset.service_name,
                ),
              }"
            >
              {{ preset.service_name || $t("certificate.issueDialog.unnamed") }}
            </span>
            <CommonInlineHint
              v-if="isSubjectIssued(preset.service_name)"
              class="ml-2"
              tone="danger"
            >
              ({{ $t("certificate.issueDialog.issued") }})
            </CommonInlineHint>
            <CommonInlineHint v-else-if="preset.description" class="ml-2">
              {{ preset.description }}
            </CommonInlineHint>
          </ElOption>
        </ElSelect>
        <CommonInlineHint v-if="presets.length === 0" block class="mt-1">
          {{ $t("certificate.issueDialog.noPresets") }}
        </CommonInlineHint>
      </ElFormItem>

      <ElFormItem
        :label="$t('certificate.issueDialog.serviceName')"
        prop="subject"
      >
        <ElInput
          v-model="form.subject"
          :placeholder="$t('certificate.issueDialog.serviceNamePlaceholder')"
          maxlength="255"
          show-word-limit
        />
        <CommonInlineHint block class="mt-1">
          {{ $t("certificate.issueDialog.serviceNameTip") }}
        </CommonInlineHint>
      </ElFormItem>

      <ElFormItem
        :label="$t('certificate.issueDialog.dnsNames')"
        prop="dns_names"
      >
        <ElInput
          v-model="dnsNamesText"
          type="textarea"
          :rows="4"
          :placeholder="$t('certificate.issueDialog.dnsNamesPlaceholder')"
          @blur="parseDnsNames"
          @input="parseDnsNames"
        />
        <div v-if="form.dns_names?.length > 0" class="mt-2">
          <ElTag
            v-for="(name, idx) in form.dns_names"
            :key="idx"
            size="small"
            class="mr-1 mb-1"
          >
            {{ name }}
          </ElTag>
        </div>
      </ElFormItem>

      <ElFormItem :label="$t('certificate.issueDialog.validity')">
        <ElSelect v-model="form.validity_days" style="width: 200px">
          <ElOption
            :label="$t('certificate.issueDialog.validityOptions.90')"
            :value="90"
          />
          <ElOption
            :label="$t('certificate.issueDialog.validityOptions.180')"
            :value="180"
          />
          <ElOption
            :label="$t('certificate.issueDialog.validityOptions.365')"
            :value="365"
          />
          <ElOption
            :label="$t('certificate.issueDialog.validityOptions.730')"
            :value="730"
          />
        </ElSelect>
      </ElFormItem>
    </ElForm>

    <template #footer>
      <div class="dialog-footer">
        <CommonInlineRow gap="8px">
          <ElTooltip
            :disabled="!authorizationDisabled"
            :content="authorizationDisabledReason"
          >
            <span>
              <ElButton
                type="success"
                :loading="batchSubmitting"
                :disabled="
                  authorizationDisabled || unissuedPresets.length === 0
                "
                @click="handleBatchIssue"
              >
                {{
                  $t("certificate.issueDialog.batchIssue", {
                    count: unissuedPresets.length,
                  })
                }}
              </ElButton>
            </span>
          </ElTooltip>
        </CommonInlineRow>
        <CommonInlineRow gap="8px">
          <ElButton @click="visible = false">{{
            $t("common.cancel")
          }}</ElButton>
          <ElTooltip
            :disabled="!authorizationDisabled"
            :content="authorizationDisabledReason"
          >
            <span>
              <ElButton
                type="primary"
                :loading="submitting"
                :disabled="authorizationDisabled"
                @click="handleSubmit"
              >
                {{ $t("certificate.issueDialog.issue") }}
              </ElButton>
            </span>
          </ElTooltip>
        </CommonInlineRow>
      </div>
    </template>
  </EditDialog>
</template>

<script setup lang="ts">
import type { FormInstance, FormRules } from "element-plus";
import { ElMessage } from "element-plus";
import { $t } from "@/locales";
import type {
  IssueCertificateRequest,
  PresetDnsName,
} from "@/types/api/certificate";
import { issueCertificate, fetchPresetList } from "@/api/certificate";
import EditDialog from "@/components/common/EditDialog.vue";
import CommonInlineHint from "@/components/common/inline-hint/index.vue";
import CommonInlineRow from "@/components/common/inline-row/index.vue";
import { runWithLoading } from "@/composables/useAsyncState";
import { getLicenseDisabledReason } from "@/composables/useLicenseGate";
import { logger } from "@/utils/sys";
defineOptions({ name: "IssueCertificateDialog" });
const props = withDefaults(
  defineProps<{
    issuedSubjects?: string[];
    authorizationDisabled?: boolean;
    authorizationDisabledReason?: string;
  }>(),
  {
    issuedSubjects: () => [],
    authorizationDisabled: false,
    authorizationDisabledReason: getLicenseDisabledReason(),
  },
);
const visible = defineModel<boolean>({ required: true });
const emit = defineEmits<{
  success: [];
}>();
const formRef = ref<FormInstance>();
const submitting = ref(false);
const batchSubmitting = ref(false);
const presets = ref<PresetDnsName[]>([]);
const selectedPreset = ref("");
const dnsNamesText = ref("");
const form = reactive<{
  subject: string;
  dns_names: string[];
  validity_days: number;
}>({
  subject: "",
  dns_names: [],
  validity_days: 365,
});
const isSubjectIssued = (subject: string | undefined): boolean => {
  if (!subject || !props.issuedSubjects) return false;
  return props.issuedSubjects.includes(subject);
};
const unissuedPresets = computed(() => {
  return presets.value.filter(
    (preset) => !isSubjectIssued(preset.service_name),
  );
});
const rules: FormRules = {
  subject: [
    {
      required: true,
      message: $t("certificate.issueDialog.validation.serviceNameRequired"),
      trigger: "blur",
    },
    {
      min: 1,
      max: 255,
      message: $t("certificate.issueDialog.validation.serviceNameLength"),
      trigger: "blur",
    },
  ],
  dns_names: [
    {
      required: true,
      message: $t("certificate.issueDialog.validation.dnsRequired"),
      trigger: "change",
      type: "array",
      min: 1,
    },
  ],
};
const loadPresets = async () => {
  try {
    const data = await fetchPresetList();
    if (Array.isArray(data)) {
      presets.value = data.filter(
        (preset) =>
          preset &&
          preset.id &&
          preset.service_name &&
          preset.service_name.trim() !== "" &&
          Array.isArray(preset.dns_names) &&
          preset.dns_names.length > 0,
      );
    } else {
      logger.error("Preset data is not an array:", typeof data, data);
      presets.value = [];
    }
  } catch (error) {
    logger.error("Failed to load presets:", error);
    presets.value = [];
  }
};
const applyPreset = (presetId: string) => {
  if (!presetId) {
    form.subject = "";
    form.dns_names = [];
    dnsNamesText.value = "";
    return;
  }
  const preset = presets.value.find((p) => p.id === presetId);
  if (preset) {
    form.subject = preset.service_name;
    form.dns_names = [...preset.dns_names];
    dnsNamesText.value = preset.dns_names.join(", ");
  }
};
const parseDnsNames = () => {
  form.dns_names = dnsNamesText.value
    .split(/[,，\n]/)
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
};
const handleSubmit = async () => {
  if (props.authorizationDisabled) {
    ElMessage.warning(props.authorizationDisabledReason);
    return;
  }
  parseDnsNames();
  const valid = await formRef.value?.validate().catch(() => false);
  if (!valid) return;
  if (!form.subject || form.subject.trim() === "") {
    ElMessage.error($t("certificate.messages.serviceNameRequired"));
    return;
  }
  if (!form.dns_names || form.dns_names.length === 0) {
    ElMessage.error($t("certificate.messages.dnsRequired"));
    return;
  }
  await runWithLoading(submitting, async () => {
    const request = {
      subject: form.subject.trim(),
      dns_names: form.dns_names,
      validity_days: form.validity_days,
    };
    await issueCertificate(request);
    ElMessage.success($t("certificate.messages.issueSuccess"));
    visible.value = false;
    emit("success");
    resetForm();
  }).catch((error) => {
    logger.error("Failed to issue certificate:", error);
  });
};
const handleBatchIssue = async () => {
  if (props.authorizationDisabled) {
    ElMessage.warning(props.authorizationDisabledReason);
    return;
  }
  if (unissuedPresets.value.length === 0) {
    ElMessage.warning($t("certificate.messages.noPresetToIssue"));
    return;
  }
  batchSubmitting.value = true;
  let successCount = 0;
  let failCount = 0;
  const errors: string[] = [];
  for (const preset of unissuedPresets.value) {
    try {
      const request: IssueCertificateRequest = {
        subject: preset.service_name,
        dns_names: preset.dns_names,
        validity_days: form.validity_days,
      };
      await issueCertificate(request);
      successCount++;
    } catch (error) {
      failCount++;
      errors.push(
        $t("certificate.issueDialog.batchIssueFailed", {
          name: preset.service_name,
        }),
      );
      logger.error(
        `Failed to issue certificate for ${preset.service_name}:`,
        error,
      );
    }
  }
  batchSubmitting.value = false;
  if (failCount === 0) {
    ElMessage.success(
      $t("certificate.messages.batchIssueSuccess", { count: successCount }),
    );
  } else if (successCount === 0) {
    ElMessage.error($t("errorPolicy.certificate"));
  } else {
    ElMessage.warning(
      $t("certificate.messages.batchIssuePartial", { successCount, failCount }),
    );
  }
  if (errors.length > 0) {
    logger.error("Batch issue errors:", errors);
  }
  visible.value = false;
  emit("success");
  resetForm();
};
const resetForm = () => {
  form.subject = "";
  form.dns_names = [];
  form.validity_days = 365;
  dnsNamesText.value = "";
  selectedPreset.value = "";
  formRef.value?.resetFields();
};
watch(visible, (val) => {
  if (val) {
    loadPresets();
  } else {
    resetForm();
  }
});
onMounted(() => {
  loadPresets();
});
</script>

<style scoped lang="scss">
.dialog-footer {
  display: flex;
  justify-content: space-between;
  align-items: center;
  width: 100%;
}
</style>
