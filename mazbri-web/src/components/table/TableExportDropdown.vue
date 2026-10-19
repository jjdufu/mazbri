<template>
  <ElTooltip :disabled="!disabled || !disabledReason" :content="disabledReason">
    <span>
      <ElDropdown trigger="click" :disabled="disabled" @command="handleCommand">
        <ElButton :disabled="disabled" :loading="loading" :type="buttonType">
          <template #icon><ArtSvgIcon :icon="icon" /></template>
          {{ resolvedLabel }}
          <ArtSvgIcon icon="ri:arrow-down-s-line" />
        </ElButton>
        <template #dropdown>
          <ElDropdownMenu>
            <ElDropdownItem
              v-for="item in items"
              :key="item.command"
              :command="item.command"
              :disabled="item.disabled"
            >
              {{ item.label }}
            </ElDropdownItem>
          </ElDropdownMenu>
        </template>
      </ElDropdown>
    </span>
  </ElTooltip>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

defineOptions({ name: "TableExportDropdown" });

export interface TableExportDropdownItem {
  command: string;
  label: string;
  disabled?: boolean;
}

const props = withDefaults(
  defineProps<{
    items: TableExportDropdownItem[];
    label?: string;
    disabled?: boolean;
    disabledReason?: string;
    loading?: boolean;
    buttonType?: "primary" | "success" | "warning" | "danger" | "info";
    icon?: string;
  }>(),
  {
    disabled: false,
    disabledReason: undefined,
    loading: false,
    label: undefined,
    buttonType: undefined,
    icon: "ri:download-line",
  },
);
const emit = defineEmits<{
  command: [command: string];
}>();
const { t } = useI18n();
const resolvedLabel = computed(() => props.label ?? t("common.actions.export"));
const handleCommand = (command: string) => {
  emit("command", command);
};
</script>
