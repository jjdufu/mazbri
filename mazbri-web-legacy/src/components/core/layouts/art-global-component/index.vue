<template>
  <component
    :is="componentConfig.component"
    v-for="componentConfig in enabledComponents"
    :key="componentConfig.key"
  />
</template>

<script setup lang="ts">
import { getEnabledGlobalComponents } from "@/config/modules/component";
defineOptions({ name: "ArtGlobalComponent" });
const props = withDefaults(
  defineProps<{
    isFullPage?: boolean;
  }>(),
  {
    isFullPage: false,
  },
);
const enabledComponents = computed(() =>
  getEnabledGlobalComponents().filter(
    (item) => !(props.isFullPage && item.key === "fireworks-effect"),
  ),
);
</script>
