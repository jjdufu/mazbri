<template>
  <CommonInlineRow
    :gap="gap"
    :min-height="minHeight"
    :wrap="wrap"
    :align="align"
  >
    <slot />
  </CommonInlineRow>
</template>

<script setup lang="ts">
import CommonInlineRow from "@/components/common/inline-row/index.vue";

defineOptions({ name: "CommonSwitchRow" });

withDefaults(
  defineProps<{
    gap?: string;
    minHeight?: string;
    wrap?: boolean;
    align?: "center" | "start" | "end" | "stretch";
  }>(),
  {
    gap: "10px",
    minHeight: "32px",
    wrap: false,
    align: "center",
  },
);
</script>
