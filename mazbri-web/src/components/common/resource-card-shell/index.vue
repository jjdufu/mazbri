<template>
  <ArtTableCard>
    <template v-if="showHeader" #header>
      <slot name="header">
        <SurfaceCardHeader
          :title="title"
          :description="description"
          :meta="meta"
          :align="headerAlign"
          :stack-on-mobile="stackHeaderOnMobile"
        >
          <template v-if="$slots.headerLeft" #left>
            <slot name="headerLeft" />
          </template>
          <template v-if="$slots.actions" #actions>
            <slot name="actions" />
          </template>
        </SurfaceCardHeader>
      </slot>
    </template>

    <div :class="bodyClass">
      <slot />
    </div>
  </ArtTableCard>
</template>

<script setup lang="ts">
import ArtTableCard from "@/components/core/cards/art-table-card/index.vue";
import SurfaceCardHeader from "@/components/common/surface-card-header/index.vue";

defineOptions({ name: "ResourceCardShell" });

withDefaults(
  defineProps<{
    title?: string;
    description?: string;
    meta?: string;
    headerAlign?: "center" | "start";
    stackHeaderOnMobile?: boolean;
    showHeader?: boolean;
    bodyClass?: string;
  }>(),
  {
    title: "",
    description: "",
    meta: "",
    headerAlign: "center",
    stackHeaderOnMobile: false,
    showHeader: true,
    bodyClass: "",
  },
);
</script>
