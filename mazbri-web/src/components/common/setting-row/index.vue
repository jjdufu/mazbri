<template>
  <div :class="rowClasses">
    <div v-if="$slots.label || label" class="common-setting-row__label">
      <slot name="label">
        {{ label }}
      </slot>
    </div>
    <div class="common-setting-row__control">
      <slot name="control">
        <slot />
      </slot>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";

defineOptions({ name: "CommonSettingRow" });

const props = withDefaults(
  defineProps<{
    label?: string;
    mode?: "stack" | "inline";
    mobileStack?: boolean;
  }>(),
  {
    label: "",
    mode: "stack",
    mobileStack: false,
  },
);

const rowClasses = computed(() => ({
  "common-setting-row": true,
  [`common-setting-row--${props.mode}`]: true,
  "common-setting-row--mobile-stack": props.mobileStack,
}));
</script>

<style scoped lang="scss">
.common-setting-row {
  display: flex;
  min-width: 0;
}

.common-setting-row--stack {
  align-items: flex-start;
  gap: 0;
  margin-bottom: 16px;
}

.common-setting-row--stack:last-child {
  margin-bottom: 0;
}

.common-setting-row--inline {
  align-items: center;
  gap: 16px;
}

.common-setting-row__label {
  flex-shrink: 0;
  width: 100px;
  font-size: 14px;
  color: var(--el-text-color-regular);
}

.common-setting-row--stack .common-setting-row__label {
  padding-top: 6px;
}

.common-setting-row--inline .common-setting-row__label {
  line-height: 1.4;
}

.common-setting-row__control {
  flex: 1;
  min-width: 0;
}

.common-setting-row--stack .common-setting-row__control {
  flex: 1;
}

@media (max-width: 768px) {
  .common-setting-row--mobile-stack {
    flex-direction: column;
    align-items: flex-start;
  }

  .common-setting-row--mobile-stack .common-setting-row__label {
    width: auto;
    padding-top: 0;
  }
}
</style>
