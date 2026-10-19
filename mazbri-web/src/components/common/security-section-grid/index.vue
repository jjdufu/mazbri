<template>
  <CommonFormSectionGrid
    :title="title"
    :description="description"
    :first="first"
    :model="model"
    :label-position="labelPosition"
    :columns="columns"
    :compact="compact"
    :single="single"
    :min-width="minWidth"
    :gap="gap"
    :max-width-mode="maxWidthMode"
    :section-class="sectionClasses"
    :form-class="formClass"
    :grid-class="gridClass"
  >
    <slot />
  </CommonFormSectionGrid>
</template>

<script setup lang="ts">
import { computed } from "vue";
import CommonFormSectionGrid from "@/components/common/form-section-grid/index.vue";

defineOptions({ name: "CommonSecuritySectionGrid" });

const props = withDefaults(
  defineProps<{
    title?: string;
    description?: string;
    first?: boolean;
    model?: Record<string, unknown>;
    labelPosition?: "top" | "left" | "right";
    columns?: number;
    compact?: boolean;
    single?: boolean;
    minWidth?: number;
    gap?: string;
    maxWidthMode?: "1fr" | "max-content";
    locked?: boolean;
    subtle?: boolean;
    sectionClass?: string;
    formClass?: string | string[] | Record<string, boolean> | null;
    gridClass?: string | string[] | Record<string, boolean> | null;
  }>(),
  {
    title: "",
    description: "",
    first: false,
    model: () => ({}),
    labelPosition: "top",
    columns: 2,
    compact: false,
    single: false,
    minWidth: 180,
    gap: "0 16px",
    maxWidthMode: "1fr",
    locked: false,
    subtle: false,
    sectionClass: "",
    formClass: "",
    gridClass: "security-form-grid",
  },
);

const sectionClasses = computed(() =>
  [
    "security-section",
    props.sectionClass,
    props.locked ? "security-section--locked" : "",
    props.subtle ? "security-section--subtle" : "",
  ]
    .filter(Boolean)
    .join(" "),
);
</script>
