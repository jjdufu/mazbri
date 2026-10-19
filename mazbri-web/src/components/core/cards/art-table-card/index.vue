<template>
  <ElCard shadow="never" class="art-table-card surface-panel-card">
    <template v-if="$slots.header" #header>
      <slot name="header" />
    </template>

    <slot />
  </ElCard>
</template>

<script setup lang="ts">
defineOptions({ name: "ArtTableCard" });
</script>
