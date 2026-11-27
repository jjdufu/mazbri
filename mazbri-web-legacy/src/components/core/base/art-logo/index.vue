<template>
  <div class="flex-cc">
    <img
      :style="logoStyle"
      src="@imgs/common/logo.webp"
      alt="logo"
      class="w-full h-full"
    />
  </div>
</template>

<script setup lang="ts">
defineOptions({ name: "ArtLogo" });
interface Props {
  size?: number | string;
}
const props = withDefaults(defineProps<Props>(), {
  size: 36,
});
const logoStyle = computed(() => ({ width: `${props.size}px` }));
</script>
