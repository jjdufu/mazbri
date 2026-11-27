<template>
  <div
    class="art-icon-button size-8.5 inline-flex flex-cc c-p text-g-600 dark:text-g-800 text-xl tad-300 hover:bg-hover-color"
    :class="{ 'rounded-full': circle }"
    :style="!circle ? { borderRadius: 'var(--art-form-control-radius-sm)' } : undefined"
  >
    <ArtSvgIcon :icon="icon"></ArtSvgIcon>
    <slot></slot>
  </div>
</template>

<script lang="ts" setup>
defineOptions({ name: "ArtIconButton" });
interface Props {
  icon: string;
  circle?: boolean;
}
withDefaults(defineProps<Props>(), {});
</script>
