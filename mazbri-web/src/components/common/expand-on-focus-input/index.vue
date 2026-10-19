<template>
  <ElInput
    v-bind="forwardedAttrs"
    :model-value="modelValue"
    class="expand-on-focus-input"
    :style="cssVars"
    data-eof-managed="component"
    @update:model-value="onUpdate"
  >
    <template v-for="(_, name) in $slots" #[name]="slotData">
      <slot :name="name" v-bind="slotData || {}" />
    </template>
  </ElInput>
</template>

<script setup lang="ts">
import { computed, useAttrs } from "vue";

defineOptions({
  name: "ExpandOnFocusInput",
  inheritAttrs: false,
});

const props = withDefaults(
  defineProps<{
    modelValue?: string | number | null;

    compactWidth?: number;

    expandWidth?: number;
  }>(),
  {
    modelValue: "",
    compactWidth: 120,
    expandWidth: 360,
  },
);

const emit = defineEmits<{
  (e: "update:modelValue", value: string | number | null | undefined): void;
}>();

const attrs = useAttrs();

const forwardedAttrs = computed(() => {
  const { class: _c, style: _s, ...rest } = attrs as Record<string, unknown>;
  return rest;
});

const cssVars = computed(() => ({
  "--eof-compact-width": `${props.compactWidth}px`,
  "--eof-expand-width": `${props.expandWidth}px`,
}));

const onUpdate = (value: string | number | null | undefined) => {
  emit("update:modelValue", value);
};
</script>
