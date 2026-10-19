<template>
  <span class="list-filter-chip" :title="title || text">
    <span class="list-filter-chip__text">{{ text }}</span>
    <button
      type="button"
      class="list-filter-chip__remove"
      :aria-label="removeLabel"
      @click="emit('remove')"
    >
      <ArtSvgIcon icon="ri:close-line" />
    </button>
  </span>
</template>

<script setup lang="ts">
defineOptions({ name: "ListFilterChip" });

defineProps<{
  text: string;

  title?: string;
  removeLabel?: string;
}>();

const emit = defineEmits<{
  remove: [];
}>();
</script>

<style scoped lang="scss">
.list-filter-chip {
  display: inline-flex;
  align-items: center;
  gap: 2px;





  max-width: 168px;
  min-width: 0;
  flex: 0 1 auto;
  padding: 2px 4px 2px 10px;
  border: 1px solid color-mix(in srgb, var(--theme-color) 22%, transparent);

  border-radius: var(--el-border-radius-base);
  background: color-mix(in srgb, var(--theme-color) 10%, transparent);
  color: var(--art-gray-900);
  font-size: 12px;
  line-height: 1.4;
}

.list-filter-chip__text {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}

.list-filter-chip__remove {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
  padding: 0;
  border: none;
  border-radius: var(--el-border-radius-base);
  background: transparent;
  color: var(--art-gray-600);
  cursor: pointer;
  transition:
    background-color 0.12s ease,
    color 0.12s ease;

  &:hover {
    background: color-mix(in srgb, var(--theme-color) 18%, transparent);
    color: var(--art-gray-900);
  }
}
</style>
