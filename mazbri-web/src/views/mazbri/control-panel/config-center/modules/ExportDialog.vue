<template>
  <EditDialog
    v-model="visible"
    :title="$t('configCenter.exportDialog.title')"
    width="500px"
    :confirm-text="$t('configCenter.exportDialog.confirm')"
    :cancel-text="$t('common.cancel')"
    :saving="exporting"
    :confirm-disabled="authorizationDisabled || services.length === 0"
    :confirm-disabled-reason="
      authorizationDisabled ? authorizationDisabledReason : undefined
    "
    @confirm="handleExport"
  >
    <ElForm label-width="80px">
      <ElFormItem :label="$t('configCenter.exportDialog.fields.services')">
        <ElSelect
          v-model="selectedServices"
          multiple
          :placeholder="$t('configCenter.exportDialog.placeholders.services')"
          :disabled="authorizationDisabled"
          style="width: 100%"
        >
          <ElOption v-for="s in services" :key="s" :label="s" :value="s" />
        </ElSelect>
      </ElFormItem>
    </ElForm>
  </EditDialog>
</template>

<script setup lang="ts">
import { ElMessage } from "element-plus";
import { saveAs } from "file-saver";
import { $t } from "@/locales";
import { exportConfigs, fetchServiceList } from "@/api/config-center";
import EditDialog from "@/components/common/EditDialog.vue";
import { getLicenseDisabledReason } from "@/composables/useLicenseGate";
import { logger } from "@/utils/sys";
import { runWithLoading } from "@/composables/useAsyncState";
interface Props {
  modelValue: boolean;
  authorizationDisabled?: boolean;
  authorizationDisabledReason?: string;
}
const props = defineProps<Props>();
const emit = defineEmits<{
  "update:modelValue": [value: boolean];
  success: [];
}>();
const visible = computed({
  get: () => props.modelValue,
  set: (val) => emit("update:modelValue", val),
});
const authorizationDisabled = computed(
  () => props.authorizationDisabled ?? false,
);
const authorizationDisabledReason = computed(
  () =>
    props.authorizationDisabledReason ?? getLicenseDisabledReason(),
);
const services = ref<string[]>([]);
const selectedServices = ref<string[]>([]);
const exporting = ref(false);
watch(visible, async (val) => {
  if (val) {
    selectedServices.value = [];
    try {
      const res = await fetchServiceList();
      services.value = Array.isArray(res.services)
        ? res.services.filter(
            (service): service is string => typeof service === "string",
          )
        : [];
    } catch (error: unknown) {
      logger.error("Failed to load services:", error);
    }
  }
});
const handleExport = async () => {
  if (authorizationDisabled.value) {
    ElMessage.warning(authorizationDisabledReason.value);
    return;
  }
  await runWithLoading(exporting, async () => {
    const blob = await exportConfigs({
      service_names:
        selectedServices.value.length > 0 ? selectedServices.value : undefined,
    });
    const timestamp = new Date().toISOString().slice(0, 10);
    const serviceSuffix =
      selectedServices.value.length > 0
        ? `-${selectedServices.value.slice(0, 2).join("-")}`
        : "-all";
    const filename = `configs${serviceSuffix}-${timestamp}.zip`;
    saveAs(blob as unknown as Blob, filename);
    ElMessage.success($t("configCenter.exportDialog.messages.exportSuccess"));
    visible.value = false;
    emit("success");
  }).catch((error: unknown) => {
    logger.error("Failed to export configs:", error);
  });
};
</script>
