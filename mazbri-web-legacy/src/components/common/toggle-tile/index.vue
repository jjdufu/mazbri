<template>
  <div class="common-toggle-tile">
    <div class="common-toggle-tile__label">
      <slot name="label">
        {{ label }}
      </slot>
    </div>
    <div class="common-toggle-tile__control">
      <slot />
    </div>
  </div>
</template>

<script setup lang="ts">
defineOptions({ name: "CommonToggleTile" });

withDefaults(
  defineProps<{
    label?: string;
  }>(),
  {
    label: "",
  },
);
</script>

<style scoped lang="scss">
.common-toggle-tile {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 14px;
  min-height: 44px;
  padding: 10px 12px;
  background: var(--el-fill-color-blank);
  border-radius: var(--el-border-radius-base);
  border: 1px solid var(--el-border-color-lighter);
}

.common-toggle-tile__label {
  font-size: 13px;
  line-height: 1.4;
  color: var(--el-text-color-regular);
}

.common-toggle-tile__control {
  display: flex;
  align-items: center;
}
</style>
