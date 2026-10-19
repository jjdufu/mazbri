<template>
  <CommonFormSection
    :title="title"
    :description="description"
    :first="first"
    :class="sectionClass"
  >
    <ElForm :model="model" :label-position="labelPosition" :class="formClass">
      <CommonFormGrid
        :class="gridClass"
        :columns="columns"
        :compact="compact"
        :single="single"
        :min-width="minWidth"
        :gap="gap"
        :max-width-mode="maxWidthMode"
      >
        <slot />
      </CommonFormGrid>
    </ElForm>
  </CommonFormSection>
</template>

<script setup lang="ts">
import CommonFormGrid from "@/components/common/form-grid/index.vue";
import CommonFormSection from "@/components/common/form-section/index.vue";

defineOptions({ name: "CommonFormSectionGrid" });

withDefaults(
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
    sectionClass?: string | string[] | Record<string, boolean> | null;
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
    gap: "14px",
    maxWidthMode: "1fr",
    sectionClass: "",
    formClass: "",
    gridClass: "",
  },
);
</script>
