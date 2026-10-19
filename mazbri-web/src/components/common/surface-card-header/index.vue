<template>
  <div :class="headerClasses">
    <div
      v-if="$slots.left || title || description || showTotal"
      class="surface-card-header-main"
    >
      <slot name="left">
        <div
          v-if="title || description || showTotal"
          class="surface-card-left"
        >
          <div v-if="title || description" class="surface-card-title">
            <p v-if="title" class="surface-card-heading">{{ title }}</p>
            <p v-if="description" class="surface-card-description">
              {{ description }}
            </p>
          </div>
          <ElTag v-if="showTotal" type="info" size="small">
            {{ totalLabel }}
          </ElTag>
        </div>
      </slot>
    </div>

    <div v-if="$slots.actions || meta" class="surface-card-actions">
      <span v-if="meta" class="surface-card-meta">{{ meta }}</span>
      <slot name="actions" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

defineOptions({ name: "SurfaceCardHeader" });

const props = withDefaults(
  defineProps<{
    title?: string;
    description?: string;
    meta?: string;

    total?: number | null;
    align?: "center" | "start";
    stackOnMobile?: boolean;
  }>(),
  {
    title: "",
    description: "",
    meta: "",
    total: null,
    align: "center",
    stackOnMobile: false,
  },
);

const { t, n } = useI18n();

const showTotal = computed(
  () => props.total != null && Number.isFinite(props.total),
);

const totalLabel = computed(() => {
  const value = Math.max(0, Math.floor(Number(props.total) || 0));

  const formatted = n(value);
  return t("common.pagination.totalCount", { n: formatted });
});

const headerClasses = computed(() => [
  "surface-card-header",
  `surface-card-header--${props.align}`,
  {
    "surface-card-header--stack-mobile": props.stackOnMobile,
  },
]);
</script>

<style scoped>
.surface-card-header--start {
  align-items: flex-start;
}

.surface-card-meta {
  color: var(--art-gray-600);
  font-size: 12px;
  line-height: 1.6;
}

@media (max-width: 768px) {
  .surface-card-header--stack-mobile {
    flex-direction: column;
    align-items: flex-start;
  }

  .surface-card-header--stack-mobile .surface-card-header-main,
  .surface-card-header--stack-mobile .surface-card-actions {
    width: 100%;
  }
}
</style>
