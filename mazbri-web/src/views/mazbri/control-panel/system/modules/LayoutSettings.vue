<template>
  <CommonSettingRow
    mode="inline"
    mobile-stack
    :label="$t('systemSettings.layout.containerWidth')"
  >
    <ElSegmented
      :model-value="modelValue.containerWidth"
      :options="containerSegmentedOptions"
      @change="handleContainerWidthChange"
    />
  </CommonSettingRow>
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

interface ContainerWidthOption {
  value: "100%" | "1200px";
  label: string;
  icon: string;
}

interface BoxStyleOption {
  type: string;
  label: string;
}

interface LayoutSettingsModel {
  containerWidth: "100%" | "1200px";
  boxBorderMode: boolean;
}

const props = defineProps<{
  modelValue: LayoutSettingsModel;
  containerWidthOptions: ContainerWidthOption[];
  boxStyleOptions: BoxStyleOption[];
}>();

const emit = defineEmits<{
  (e: "update:modelValue", value: LayoutSettingsModel): void;
}>();

const containerSegmentedOptions = computed(() =>
  props.containerWidthOptions.map((opt) => ({
    label: opt.label,
    value: opt.value,
  })),
);

const boxStyleSegmentedOptions = computed(() =>
  props.boxStyleOptions.map((opt) => ({
    label: opt.label,
    value: opt.type,
  })),
);

const handleContainerWidthChange = (value: "100%" | "1200px") => {
  emit("update:modelValue", { ...props.modelValue, containerWidth: value });
};

const handleBoxStyleChange = (type: string) => {
  emit("update:modelValue", {
    ...props.modelValue,
    boxBorderMode: type === "border-mode",
  });
};
</script>
