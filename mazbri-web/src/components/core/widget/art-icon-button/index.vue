<template>
  <div
    class="size-8.5 inline-flex flex-cc c-p text-g-600 dark:text-g-800 text-xl rounded tad-300 hover:bg-hover-color"
    :class="{ 'rounded-full': circle }"
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
