<template>
  <ElDropdown trigger="click" :disabled="disabled" @command="handleCommand">
    <ElButton :type="buttonType" :disabled="disabled">
      <template #icon><ArtSvgIcon :icon="icon" /></template>
      {{ resolvedLabel }} <ArtSvgIcon icon="ri:arrow-down-s-line" />
    </ElButton>
    <template #dropdown>
      <ElDropdownMenu>
        <ElDropdownItem command="all" :disabled="totalCount === 0">
          {{ allText }}
        </ElDropdownItem>
        <ElDropdownItem command="selected" :disabled="selectedCount === 0">
          {{ selectedText }}
        </ElDropdownItem>
      </ElDropdownMenu>
    </template>
  </ElDropdown>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
defineOptions({ name: "TableDeleteDropdown" });
const props = withDefaults(
  defineProps<{
    totalCount: number;
    selectedCount: number;
    label?: string;
    buttonType?: "primary" | "success" | "warning" | "danger" | "info";
    icon?: string;
    disabled?: boolean;
  }>(),
  {
    label: undefined,
    buttonType: "danger",
    icon: "ri:delete-bin-line",
    disabled: false,
  },
);
const { t } = useI18n();
const emit = defineEmits<{
  command: [type: "all" | "selected"];
}>();
const resolvedLabel = computed(
  () => props.label ?? t("common.tableDeleteDropdown.label"),
);
const allText = computed(() =>
  t("common.tableDeleteDropdown.all", { label: resolvedLabel.value }),
);
const selectedText = computed(() =>
  t("common.tableDeleteDropdown.selected", {
    label: resolvedLabel.value,
    count: props.selectedCount,
  }),
);
const handleCommand = (command: "all" | "selected") => {
  emit("command", command);
};
</script>
