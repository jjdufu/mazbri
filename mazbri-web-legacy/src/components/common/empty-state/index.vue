<template>
  <div class="empty-state" :class="{ 'empty-state--compact': compact }">
    <ElEmpty :image-size="imageSize" :description="slotDescription ? undefined : resolvedDescription">
      <template v-if="slotDescription" #description>
        <slot name="description" />
      </template>
      <template v-if="$slots.default" #default>
        <slot />
      </template>
      <template v-if="$slots.image" #image>
        <slot name="image" />
      </template>
    </ElEmpty>
  </div>
</template>

<script setup lang="ts">
import { computed, useSlots } from "vue";
import { useI18n } from "vue-i18n";

defineOptions({ name: "EmptyState" });

const props = withDefaults(
  defineProps<{
    /** 空态说明；缺省用 common.feedback.noData。有 #description 插槽时忽略。 */
    description?: string;
    imageSize?: number;
    compact?: boolean;
  }>(),
  {
    description: undefined,
    imageSize: 72,
    compact: false,
  },
);

const slots = useSlots();
const slotDescription = computed(() => Boolean(slots.description));
const { t } = useI18n();
const resolvedDescription = computed(
  () => props.description ?? t("common.feedback.noData"),
);
</script>

<style scoped lang="scss">
.empty-state {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: var(--ops-card-min-height, 180px);
  padding: var(--ops-space-md, 16px);
  box-sizing: border-box;
}

.empty-state--compact {
  min-height: 120px;
  padding: var(--ops-space-sm, 12px);
}
</style>
