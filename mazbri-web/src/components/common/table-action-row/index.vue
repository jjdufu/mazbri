<template>
  <div class="common-table-action-row" :style="{ gap }">
    <slot />
  </div>
</template>

<script setup lang="ts">
defineOptions({ name: "CommonTableActionRow" });

withDefaults(
  defineProps<{
    gap?: string;
  }>(),
  {
    gap: "12px",
  },
);
</script>

<style scoped lang="scss">
.common-table-action-row {
  display: flex;
  align-items: center;
  flex-wrap: nowrap;
  white-space: nowrap;

  :deep(.el-button.is-link) {
    position: relative;
    padding: 4px 8px;
    height: 28px;
    font-size: 13px;

    border-radius: var(--art-form-control-radius-sm, var(--el-border-radius-base));
    transition: all 0.2s ease;
    margin-left: 0 !important;

    &:hover {
      background-color: var(--el-fill-color-light);
      text-decoration: none;
    }

    &::after {
      content: "";
      position: absolute;
      top: -4px;
      bottom: -4px;
      left: -6px;
      right: -6px;
      cursor: pointer;
    }
  }
}
</style>
