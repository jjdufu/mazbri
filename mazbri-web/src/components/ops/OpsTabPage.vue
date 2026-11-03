<template>
  <OpsPage :title="title">
    <template v-if="$slots.title" #title>
      <slot name="title" />
    </template>
    <template #actions>
      <slot name="actions" />
    </template>

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
        lazy
      >
        <!-- 仅挂载当前激活面板，避免无权限/非激活 Tab 触发请求 -->
        <slot
          v-if="modelValue === tab.id"
          :name="tab.id"
          :tab="tab"
        />
      </ElTabPane>
    </ElTabs>

    <div v-else class="ops-tab-page__empty">
      <slot name="empty" />
    </div>
  </OpsPage>
</template>

<script setup lang="ts">
import OpsPage from "@/components/ops/OpsPage.vue";
import type { OpsTabDef } from "@/utils/ops/tabs";

defineOptions({ name: "OpsTabPage" });

const props = withDefaults(
  defineProps<{
    title?: string;
    /** 已经按角色过滤后的可见 Tab。 */
    tabs: OpsTabDef[];
    modelValue?: string;
  }>(),
  {
    title: undefined,
    modelValue: undefined,
  },
);

const emit = defineEmits<{
  "update:modelValue": [string];
  change: [string];
}>();

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
.ops-tab-page__tabs {
  min-width: 0;
}

.ops-tab-page__empty {
  color: var(--el-text-color-secondary);
  font-size: 13px;
}
</style>
