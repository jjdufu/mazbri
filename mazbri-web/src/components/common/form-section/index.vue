<template>
  <section
    class="common-form-section"
    :class="{ 'common-form-section--first': first }"
  >
    <div
      v-if="title || description || $slots.actions"
      class="common-form-section__heading"
    >
      <div
        v-if="title || description"
        class="common-form-section__heading-main"
      >
        <div v-if="title" class="common-form-section__title">{{ title }}</div>
        <p v-if="description" class="common-form-section__description">
          {{ description }}
        </p>
      </div>
      <slot name="actions" />
    </div>
    <slot />
  </section>
</template>

<script setup lang="ts">
defineOptions({ name: "CommonFormSection" });
withDefaults(
  defineProps<{
    title?: string;
    description?: string;
    first?: boolean;
  }>(),
  {
    title: "",
    description: "",
    first: false,
  },
);
</script>

<style scoped lang="scss">
.common-form-section {
  margin-bottom: 24px;

  &:last-child {
    margin-bottom: 0;
  }
}

.common-form-section--first {
  padding-top: 0;
  border-top: none;
}

.common-form-section__heading {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 12px;
  margin-bottom: 12px;
  padding-bottom: 8px;
  border-bottom: 1px solid var(--el-border-color-lighter);
}

.common-form-section__heading-main {
  min-width: 0;
}

.common-form-section__title {
  font-size: 14px;
  font-weight: 600;
  color: var(--el-text-color-primary);
}

.common-form-section__description {
  margin: 6px 0 0;
  color: var(--el-text-color-secondary);
  font-size: 12px;
  line-height: 1.6;
}
</style>
