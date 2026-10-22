<template>
  <ElFormItem
    :label="$t('configCenter.serviceNameSelector.label')"
    prop="service_name"
    required
  >
    <ElSelect
      :model-value="modelValue"
      :placeholder="$t('configCenter.serviceNameSelector.placeholder')"
      :disabled="disabled"
      filterable
      class="w-full"
      @update:model-value="onSelect"
    >
      <ElOption
        v-for="name in options"
        :key="name"
        :label="
          name === globalName
            ? $t('configCenter.serviceNameSelector.global')
            : name
        "
        :value="name"
        :disabled="existingNames.includes(name)"
      />
    </ElSelect>
  </ElFormItem>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { fetchCanonicalServiceNames } from "@/api/service-names";

defineOptions({ name: "ServiceNameSelector" });

const props = withDefaults(
  defineProps<{
    modelValue: string;
    disabled?: boolean;
    includeGlobal?: boolean;
    existingNames?: string[];
  }>(),
  { disabled: false, includeGlobal: true, existingNames: () => [] },
);
const emit = defineEmits<{
  "update:modelValue": [value: string];
  change: [value: string];
}>();

const services = ref<string[]>([]);
const globalName = ref("global");

onMounted(async () => {
  const data = await fetchCanonicalServiceNames();
  services.value = data.services;
  globalName.value = data.global;
});

const options = computed(() =>
  props.includeGlobal
    ? [globalName.value, ...services.value]
    : [...services.value],
);

const onSelect = (value: string) => {
  emit("update:modelValue", value);
  emit("change", value);
};
</script>
