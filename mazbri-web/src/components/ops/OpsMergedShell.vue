<template>
  <OpsTabPage
    :title="title"
    :tabs="visibleTabs"
    :model-value="activeTab"
    @update:model-value="handleTabChange"
  >
    <template v-if="$slots.actions" #actions>
      <slot name="actions" />
    </template>

    <template
      v-for="tab in visibleTabs"
      :key="tab.id"
      #[tab.id]="slotProps"
    >
      <slot :name="tab.id" v-bind="slotProps">
        <p class="ops-merged-shell__placeholder">
          {{ placeholder }}
        </p>
      </slot>
    </template>

    <template #empty>
      <slot name="empty">
        <p class="ops-merged-shell__placeholder">{{ emptyText }}</p>
      </slot>
    </template>
  </OpsTabPage>
</template>

<script setup lang="ts">
import { computed, toRef } from "vue";
import { useI18n } from "vue-i18n";
import OpsTabPage from "@/components/ops/OpsTabPage.vue";
import { useOpsTab, type OpsTabDef } from "@/composables/ops/useOpsTab";

defineOptions({ name: "OpsMergedShell" });

const props = withDefaults(
  defineProps<{
    title: string;
    tabs: OpsTabDef[];
    defaultTab?: string;
    queryKey?: string;
    /** 空壳占位文案；缺省用 i18n。 */
    placeholder?: string;
    emptyText?: string;
  }>(),
  {
    defaultTab: undefined,
    queryKey: "tab",
    placeholder: undefined,
    emptyText: undefined,
  },
);

const { t } = useI18n();

const tabsRef = toRef(props, "tabs");
const { activeTab, visibleTabs, handleTabChange } = useOpsTab({
  tabs: tabsRef,
  queryKey: props.queryKey,
  defaultTab: props.defaultTab,
});

const placeholder = computed(
  () => props.placeholder ?? t("ops.mergedShell.placeholder"),
);
const emptyText = computed(
  () => props.emptyText ?? t("ops.mergedShell.empty"),
);
</script>

<style scoped lang="scss">
.ops-merged-shell__placeholder {
  margin: 0;
  padding: 12px 0;
  color: var(--el-text-color-secondary);
  font-size: 13px;
  line-height: 1.5;
}
</style>
