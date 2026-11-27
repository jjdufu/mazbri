<template>
  <HoppSelect
    :model-value="modelValue"
    :options="options"
    :append-to="appendTo"
    :disabled="disabled"
    :title="title"
    :placeholder="placeholder"
    @update:model-value="emit('update:modelValue', $event)"
  />
</template>

<script lang="ts">
export type { HoppSelectOption } from "@/components/common/hopp-select/index.vue";
</script>

<script setup lang="ts">
import HoppSelect from "@/components/common/hopp-select/index.vue";
import type { HoppSelectOption } from "@/components/common/hopp-select/index.vue";

withDefaults(
  defineProps<{
    modelValue: string;
    options: HoppSelectOption[];
    appendTo?: HTMLElement | string;
    disabled?: boolean;
    title?: string;
    placeholder?: string;
  }>(),
  {
    appendTo: undefined,
    disabled: false,
    title: "",
    placeholder: "",
  },
);

const emit = defineEmits<{
  "update:modelValue": [string];
}>();
</script>
