<template>
  <CommonSettingRow
    mode="inline"
    mobile-stack
    :label="$t('systemSettings.layout.boxStyle')"
  >
    <ElSegmented
      :model-value="modelValue.boxBorderMode ? 'border-mode' : 'shadow-mode'"
      :options="boxStyleSegmentedOptions"
      @change="handleBoxStyleChange"
    />
  </CommonSettingRow>
</template>

<script setup lang="ts">
import { computed } from "vue";
import CommonSettingRow from "@/components/common/setting-row/index.vue";

interface BoxStyleOption {
  type: string;
  label: string;
}

interface LayoutSettingsModel {
  boxBorderMode: boolean;
}

const props = defineProps<{
  modelValue: LayoutSettingsModel;
  boxStyleOptions: BoxStyleOption[];
}>();

const emit = defineEmits<{
  (e: "update:modelValue", value: LayoutSettingsModel): void;
}>();

const boxStyleSegmentedOptions = computed(() =>
  props.boxStyleOptions.map((opt) => ({
    label: opt.label,
    value: opt.type,
  })),
);

const handleBoxStyleChange = (type: string) => {
  emit("update:modelValue", {
    ...props.modelValue,
    boxBorderMode: type === "border-mode",
  });
};
</script>
