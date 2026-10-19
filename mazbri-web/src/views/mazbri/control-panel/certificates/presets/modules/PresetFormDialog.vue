<template>
  <EditDialog
    v-model="visible"
    :title="
      mode === 'edit'
        ? $t('certificate.presets.form.editTitle')
        : $t('certificate.presets.form.createTitle')
    "
    width="500px"
    :confirm-text="$t('common.confirm')"
    :cancel-text="$t('common.cancel')"
    :saving="props.submitting"
    @confirm="handleSubmit"
  >
    <ElForm ref="formRef" :model="form" :rules="rules" label-width="120px">
      <ElFormItem
        :label="$t('certificate.presets.form.fields.serviceName')"
        prop="service_name"
      >
        <ElInput
          v-model="form.service_name"
          :placeholder="$t('certificate.presets.form.placeholders.serviceName')"
          :disabled="mode === 'edit'"
          clearable
        />
      </ElFormItem>

      <ElFormItem
        :label="$t('certificate.presets.form.fields.dnsNames')"
        prop="dns_names"
      >
        <ElInput
          v-model="dnsNamesText"
          type="textarea"
          :rows="4"
          :placeholder="$t('certificate.presets.form.placeholders.dnsNames')"
          @blur="syncDnsNames"
          @input="syncDnsNames"
        />
        <div v-if="form.dns_names.length > 0" class="dns-preview mt-2">
          <ElTag
            v-for="(name, idx) in form.dns_names"
            :key="name + idx"
            size="small"
            class="mr-1 mb-1"
            closable
            @close="removeDnsName(idx)"
          >
            {{ name }}
          </ElTag>
        </div>
      </ElFormItem>

      <ElFormItem :label="$t('certificate.presets.form.fields.description')">
        <ElInput
          v-model="form.description"
          type="textarea"
          :rows="2"
          :placeholder="$t('certificate.presets.form.placeholders.description')"
        />
      </ElFormItem>
    </ElForm>
  </EditDialog>
</template>

<script setup lang="ts">
import type { FormInstance, FormRules } from "element-plus";
import { $t } from "@/locales";
import type {
  CreatePresetRequest,
  PresetDnsName,
} from "@/types/api/certificate";
import EditDialog from "@/components/common/EditDialog.vue";
import { parseDnsNamesText, stringifyDnsNamesText } from "../usePresetImport";
interface Props {
  modelValue: boolean;
  mode: "create" | "edit";
  preset?: PresetDnsName | null;
  submitting?: boolean;
}
const props = withDefaults(defineProps<Props>(), {
  preset: null,
  submitting: false,
});
const emit = defineEmits<{
  "update:modelValue": [value: boolean];
  submit: [value: CreatePresetRequest];
}>();
const visible = computed({
  get: () => props.modelValue,
  set: (value) => emit("update:modelValue", value),
});
const formRef = ref<FormInstance>();
const dnsNamesText = ref("");
const form = reactive<CreatePresetRequest>({
  service_name: "",
  dns_names: [],
  description: "",
});
const rules: FormRules = {
  service_name: [
    {
      required: true,
      message: $t("certificate.presets.form.validation.serviceNameRequired"),
      trigger: "blur",
    },
  ],
  dns_names: [
    {
      required: true,
      message: $t("certificate.presets.form.validation.dnsNamesRequired"),
      trigger: "change",
      type: "array",
      min: 1,
    },
  ],
};
watch(
  () => [props.modelValue, props.preset] as const,
  ([open]) => {
    if (!open) {
      return;
    }
    if (props.preset) {
      form.service_name = props.preset.service_name;
      form.dns_names = [...props.preset.dns_names];
      form.description = props.preset.description ?? "";
      dnsNamesText.value = stringifyDnsNamesText(props.preset.dns_names);
      return;
    }
    form.service_name = "";
    form.dns_names = [];
    form.description = "";
    dnsNamesText.value = "";
  },
  { immediate: true },
);
const syncDnsNames = () => {
  form.dns_names = parseDnsNamesText(dnsNamesText.value);
};
const removeDnsName = (index: number) => {
  form.dns_names.splice(index, 1);
  dnsNamesText.value = stringifyDnsNamesText(form.dns_names);
};
const handleSubmit = async () => {
  syncDnsNames();
  const valid = await formRef.value?.validate().catch(() => false);
  if (!valid) {
    return;
  }
  emit("submit", {
    service_name: form.service_name.trim(),
    dns_names: [...form.dns_names],
    description: form.description?.trim() ?? "",
  });
};
</script>

<style scoped lang="scss">
.dns-preview {
  min-height: 32px;
  border: 1px solid var(--el-border-color-lighter);
  border-radius: var(--el-border-radius-base);
  padding: 8px;
  background-color: var(--el-fill-color-light);
}
</style>
