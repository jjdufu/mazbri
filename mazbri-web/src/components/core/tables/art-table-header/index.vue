<template>
  <div v-if="hasLeftSlot" id="art-table-header" class="flex-cb max-md:!block">
    <div class="flex-wrap">
      <slot name="left"></slot>
    </div>
  </div>
</template>

<script lang="ts" setup>
defineOptions({ name: "ArtTableHeader" });
const slots = useSlots();
const hasLeftSlot = computed(() => Boolean(slots.left));
</script>
