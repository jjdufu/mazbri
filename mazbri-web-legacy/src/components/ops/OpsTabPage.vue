<template>
  <OpsPage :title="title" :subtitle="subtitle">
    <div v-if="tabs.length > 0 || $slots.actions" class="ops-tab-page__header">
      <ElTabs
        v-if="tabs.length > 0"
        :model-value="modelValue"
        class="ops-tab-page__tabs"
        @update:model-value="onTabUpdate"
      >
        <ElTabPane
          v-for="tab in tabs"
          :key="tab.id"
          :name="tab.id"
          :label="tab.label ?? tab.id"
          :disabled="tab.disabled"
        />
      </ElTabs>
      <div ref="headerActionsEl" class="ops-tab-page__actions">
        <slot name="actions" />
      </div>
    </div>

    <template v-if="tabs.length > 0">
      <div v-if="$slots.default" class="ops-tab-page__pane">
        <slot />
      </div>
      <template v-else>
        <template v-for="tab in tabs" :key="tab.id">
          <!-- 仅挂载当前激活面板，避免无权限/非激活 Tab 触发请求 -->
          <div v-if="modelValue === tab.id" class="ops-tab-page__pane">
            <slot :name="tab.id" :tab="tab" />
          </div>
        </template>
      </template>
    </template>

    <div v-else class="ops-tab-page__empty">
      <slot name="empty" />
    </div>
  </OpsPage>
</template>

<script setup lang="ts">
import { provide, ref } from "vue";
import OpsPage from "@/components/ops/OpsPage.vue";
import type { OpsTabDef } from "@/utils/ops/tabs";

defineOptions({ name: "OpsTabPage" });

const props = withDefaults(
  defineProps<{
    title?: string;
    subtitle?: string;
    /** 已经按角色过滤后的可见 Tab。 */
    tabs: OpsTabDef[];
    modelValue?: string;
  }>(),
  {
    title: undefined,
    subtitle: undefined,
    modelValue: undefined,
  },
);

const emit = defineEmits<{
  "update:modelValue": [string];
  change: [string];
}>();

const headerActionsEl = ref<HTMLElement | null>(null);
provide("opsTabHeaderActionsEl", headerActionsEl);

function onTabUpdate(value: string | number) {
  const next = String(value);
  if (!props.tabs.some((tab) => tab.id === next)) {
    return;
  }
  emit("update:modelValue", next);
  emit("change", next);
}
</script>

<style scoped lang="scss">
.ops-tab-page__header {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 8px 16px;
  min-width: 0;
}

.ops-tab-page__tabs {
  min-width: 0;

  :deep(.el-tabs__header) {
    margin: 0 !important;
  }

  :deep(.el-tabs__nav-wrap::after) {
    display: none !important;
  }

  :deep(.el-tabs__content) {
    display: none !important;
  }
}

.ops-tab-page__actions {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--ops-header-actions-gap, 8px);
  margin-left: auto;
  padding-bottom: 4px;

  &:empty {
    display: none;
  }
}

.ops-tab-page__pane {
  display: flex;
  flex-direction: column;
  gap: var(--ops-section-gap, 16px);
  min-width: 0;
}

.ops-tab-page__empty {
  color: var(--el-text-color-secondary);
  font-size: 13px;
}
</style>
