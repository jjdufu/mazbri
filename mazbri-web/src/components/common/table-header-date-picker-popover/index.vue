<template>
  <div
    ref="rootRef"
    class="table-header-date-picker-popover"
    :class="{ 'is-expanded': expanded, 'has-value': hasValue }"
    :style="panelStyle"
  >
    <ElTooltip v-if="!expanded" :content="tooltipText" placement="top">
      <button
        type="button"
        class="table-header-date-picker-popover__trigger"
        @click="handleExpand"
      >
        <ArtSvgIcon :icon="icon" />
      </button>
    </ElTooltip>

    <template v-else>
      <button
        type="button"
        class="table-header-date-picker-popover__trigger is-active"
        @click="handleCollapse"
      >
        <ArtSvgIcon :icon="icon" />
        <span class="table-header-date-picker-popover__trigger-text">{{
          activeText
        }}</span>
      </button>

      <div class="table-header-date-picker-popover__panel">
        <DatePicker v-model="modelValue" v-bind="$attrs">
          <template
            v-for="(_, slotName) in slots"
            :key="slotName"
            #[slotName]="slotProps"
          >
            <slot :name="slotName" v-bind="slotProps || {}" />
          </template>
        </DatePicker>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import DatePicker from "@/components/common/DatePicker.vue";
import { useI18n } from "vue-i18n";
defineOptions({
  name: "TableHeaderDatePickerPopover",
  inheritAttrs: false,
});
interface Props {




  width?: number | string;
  tooltip?: string;
  activeText?: string;
  icon?: string;
}
const props = withDefaults(defineProps<Props>(), {

  width: 360,
  tooltip: undefined,
  activeText: undefined,
  icon: "ri:calendar-line",
});
const { t } = useI18n();
const modelValue = defineModel<any>();
const slots = useSlots();
const expanded = ref(false);
const rootRef = ref<HTMLElement | null>(null);
const normalizedWidth = computed(() =>
  typeof props.width === "number" ? `${props.width}px` : props.width,
);
const panelStyle = computed(() => ({
  "--table-header-date-picker-width": normalizedWidth.value,
}));
const tooltipText = computed(
  () => props.tooltip ?? t("common.datePickerPopover.tooltip"),
);
const activeText = computed(
  () => props.activeText ?? t("common.datePickerPopover.collapse"),
);
const hasValue = computed(() => {
  const value = modelValue.value;
  return Array.isArray(value) ? value.some(Boolean) : Boolean(value);
});
const focusDateField = async () => {
  await nextTick();
  const field = rootRef.value?.querySelector<HTMLElement>(
    ".art-date-picker .el-range-input, .art-date-picker input, .art-date-picker",
  );
  field?.focus();
  field?.click();
};
const handleExpand = async () => {
  expanded.value = true;
  await focusDateField();
};
const handleCollapse = () => {
  expanded.value = false;
};
</script>

<style scoped lang="scss">
.table-header-date-picker-popover {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;

  &.is-expanded {
    flex: 1 1 auto;
  }

  &.has-value:not(.is-expanded) {
    .table-header-date-picker-popover__trigger {
      background: color-mix(in srgb, var(--theme-color) 10%, transparent);
      color: var(--theme-color);
    }
  }

  :deep(.art-date-picker) {
    width: 100%;
    min-width: 0;
  }

  :deep(.art-date-picker.el-date-editor--daterange),
  :deep(.art-date-picker.el-date-editor--datetimerange) {
    display: inline-flex;
  }

  :deep(.art-date-picker.el-date-editor--daterange .el-range-input),
  :deep(.art-date-picker.el-date-editor--datetimerange .el-range-input) {
    flex: 1 1 0;
    min-width: 0;
    font-size: 13px;
  }

  :deep(.art-date-picker.el-date-editor--daterange .el-range-separator),
  :deep(.art-date-picker.el-date-editor--datetimerange .el-range-separator) {
    flex: 0 0 auto;
    padding: 0 6px;
  }

  :deep(.art-date-picker.el-date-editor--daterange .el-range__icon),
  :deep(.art-date-picker.el-date-editor--daterange .el-range__close-icon),
  :deep(.art-date-picker.el-date-editor--datetimerange .el-range__icon),
  :deep(.art-date-picker.el-date-editor--datetimerange .el-range__close-icon) {
    flex: 0 0 auto;
  }
}

.table-header-date-picker-popover__trigger {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  width: 36px;
  height: 36px;
  padding: 0;
  border: none;

  border-radius: var(--el-border-radius-base);
  background: transparent;
  color: var(--art-gray-700);
  cursor: pointer;
  transition:
    background-color 0.2s ease,
    color 0.2s ease;

  &:hover {
    background: color-mix(in srgb, var(--theme-color) 8%, transparent);
    color: var(--art-gray-900);
  }
}

.table-header-date-picker-popover__trigger.is-active {
  width: auto;
  padding: 0 10px;
  border-radius: var(--el-border-radius-base);
}

.table-header-date-picker-popover__trigger-text {
  white-space: nowrap;
  font-size: 13px;
  line-height: 1;
}

.table-header-date-picker-popover__panel {
  display: flex;
  flex: 0 1 var(--table-header-date-picker-width);
  width: var(--table-header-date-picker-width);
  min-width: 0;
  max-width: 100%;
}

@media (max-width: 900px) {
  .table-header-date-picker-popover.is-expanded {
    width: 100%;
  }

  .table-header-date-picker-popover__panel {
    width: 100%;
    max-width: none;
  }
}
</style>
