<template>
  <div :class="headerClasses">
    <div class="common-drawer-header__main">
      <div v-if="title" class="common-drawer-header__title">{{ title }}</div>
      <div v-if="subtitle" class="common-drawer-header__subtitle">
        {{ subtitle }}
      </div>
    </div>
    <div v-if="$slots.actions" class="common-drawer-header__actions">
      <slot name="actions" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";

defineOptions({ name: "CommonDrawerHeader" });

const props = withDefaults(
  defineProps<{
    title?: string;
    subtitle?: string;
    align?: "start" | "center";
  }>(),
  {
    title: "",
    subtitle: "",
    align: "start",
  },
);

const headerClasses = computed(() => [
  "common-drawer-header",
  `common-drawer-header--${props.align}`,
]);
</script>

<style scoped lang="scss">
.common-drawer-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 16px;
  width: 100%;
}

.common-drawer-header--center {
  align-items: center;
}

.common-drawer-header__main {
  min-width: 0;
}

.common-drawer-header__title {
  font-size: 16px;
  font-weight: 600;
}

.common-drawer-header__subtitle {
  margin-top: 4px;
  color: var(--el-text-color-secondary);
  font-size: 12px;
}

.common-drawer-header__actions {
  flex: none;
}
</style>
