<template>
  <CommonFormSection
    :title="title"
    :description="description"
    :first="first"
    :class="sectionClasses"
  >
    <slot />
  </CommonFormSection>
</template>

<script setup lang="ts">
import { computed } from "vue";
import CommonFormSection from "@/components/common/form-section/index.vue";

defineOptions({ name: "CommonPolicySection" });

const props = withDefaults(
  defineProps<{
    title?: string;
    description?: string;
    first?: boolean;
    locked?: boolean;
    subtle?: boolean;
    sectionClass?: string | string[] | Record<string, boolean> | null;
  }>(),
  {
    title: "",
    description: "",
    first: false,
    locked: false,
    subtle: false,
    sectionClass: "",
  },
);

const sectionClasses = computed(() => [
  props.sectionClass,
  {
    "security-section--locked": props.locked,
    "security-section--subtle": props.subtle,
  },
]);
</script>
