<template>
  <EditDialog
    v-model="visible"
    :title="$t('certificate.issueDialog.title')"
    width="600px"
  >
    <ElForm ref="formRef" :model="form" :rules="rules" label-width="100px">
      <ElFormItem :label="$t('certificate.issueDialog.service')" prop="subject">
        <ElSelect
          v-model="form.subject"
          :placeholder="$t('certificate.issueDialog.service')"
          filterable
          style="width: 100%"
        >
          <ElOption
            v-for="name in services"
            :key="name"
            :label="name"
            :value="name"
            :disabled="isSubjectIssued(name)"
          >
            <span
              :class="{ 'line-through text-gray-400': isSubjectIssued(name) }"
            >
              {{ name }}
            </span>
            <CommonInlineHint
              v-if="isSubjectIssued(name)"
              class="ml-2"
              tone="danger"
            >
              ({{ $t("certificate.issueDialog.issued") }})
            </CommonInlineHint>
          </ElOption>
        </ElSelect>
        <CommonInlineHint block class="mt-1">
          {{ $t("certificate.issueDialog.sanAutoTip") }}
        </CommonInlineHint>
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
                  authorizationDisabled || unissuedServices.length === 0
                "
                @click="handleBatchIssue"
              >
                {{
                  $t("certificate.issueDialog.issueAllUnissued", {
                    count: unissuedServices.length,
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
import { issueCertificate } from "@/api/certificate";
import { fetchCanonicalServiceNames } from "@/api/service-names";
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
const services = ref<string[]>([]);
const form = reactive<{
  subject: string;
  validity_days: number;
}>({
  subject: "",
  validity_days: 365,
});
const isSubjectIssued = (subject: string | undefined): boolean => {
  if (!subject) return false;
  return props.issuedSubjects.includes(subject);
};
const unissuedServices = computed(() =>
  services.value.filter((s) => !isSubjectIssued(s)),
);
const rules: FormRules = {
  subject: [
    {
      required: true,
      message: $t("certificate.issueDialog.validation.serviceRequired"),
      trigger: "change",
    },
  ],
};
const loadServices = async () => {
  try {
    const data = await fetchCanonicalServiceNames();
    services.value = data.mtls_services;
  } catch (error) {
    logger.error("Failed to load service names:", error);
    services.value = [];
  }
};
const handleSubmit = async () => {
  if (props.authorizationDisabled) {
    ElMessage.warning(props.authorizationDisabledReason);
    return;
  }
  const valid = await formRef.value?.validate().catch(() => false);
  if (!valid) return;
  await runWithLoading(submitting, async () => {
    await issueCertificate({
      subject: form.subject,
      validity_days: form.validity_days,
    });
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
  if (unissuedServices.value.length === 0) {
    ElMessage.warning($t("certificate.messages.noServiceToIssue"));
    return;
  }
  batchSubmitting.value = true;
  let successCount = 0;
  let failCount = 0;
  const errors: string[] = [];
  for (const subject of unissuedServices.value) {
    try {
      await issueCertificate({ subject, validity_days: form.validity_days });
      successCount++;
    } catch (error) {
      failCount++;
      errors.push(
        $t("certificate.issueDialog.batchIssueFailed", { name: subject }),
      );
      logger.error(`Failed to issue certificate for ${subject}:`, error);
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
  form.validity_days = 365;
  formRef.value?.resetFields();
};
watch(visible, (val) => {
  if (val) {
    void loadServices();
  } else {
    resetForm();
  }
});
onMounted(() => {
  void loadServices();
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
