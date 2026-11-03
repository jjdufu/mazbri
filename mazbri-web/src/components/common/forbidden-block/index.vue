<template>
  <div class="forbidden-block" role="alert">
    <ElResult
      icon="warning"
      :title="resolvedTitle"
      :sub-title="resolvedDescription"
    >
      <template v-if="$slots.extra" #extra>
        <slot name="extra" />
      </template>
    </ElResult>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

defineOptions({ name: "ForbiddenBlock" });

const props = withDefaults(
  defineProps<{
    title?: string;
    description?: string;
  }>(),
  {
    title: undefined,
    description: undefined,
  },
);

const { t } = useI18n();
const resolvedTitle = computed(
  () => props.title ?? t("layout.forbidden.title"),
);
const resolvedDescription = computed(
  () => props.description ?? t("layout.forbidden.description"),
);
</script>

<style scoped lang="scss">
.forbidden-block {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: var(--ops-card-min-height, 180px);
  padding: var(--ops-space-md, 16px);
  box-sizing: border-box;
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 8px;
  background: var(--el-bg-color);
}
</style>
