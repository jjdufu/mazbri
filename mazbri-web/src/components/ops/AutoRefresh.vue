<template>
  <ElSelect
    v-model="interval"
    class="ops-auto-refresh"
    :placeholder="t('ops.autoRefresh.label')"
    @change="onChange"
  >
    <ElOption
      v-for="opt in options"
      :key="opt.value"
      :label="opt.label"
      :value="opt.value"
    />
  </ElSelect>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";

const props = withDefaults(
  defineProps<{
    modelValue?: number;
    /** Called on each tick when interval > 0. */
  }>(),
  { modelValue: 0 },
);

const emit = defineEmits<{
  "update:modelValue": [number];
  tick: [];
  change: [number];
}>();

const { t } = useI18n();
const interval = ref(props.modelValue);
let timer: ReturnType<typeof setInterval> | null = null;

const options = computed(() => [
  { label: t("ops.autoRefresh.off"), value: 0 },
  { label: t("ops.autoRefresh.thirtySeconds"), value: 30 },
  { label: t("ops.autoRefresh.oneMinute"), value: 60 },
]);

function clearTimer() {
  if (timer) {
    clearInterval(timer);
    timer = null;
  }
}

function arm() {
  clearTimer();
  if (interval.value > 0) {
    timer = setInterval(() => emit("tick"), interval.value * 1000);
  }
}

function onChange() {
  emit("update:modelValue", interval.value);
  emit("change", interval.value);
  arm();
}

watch(
  () => props.modelValue,
  (v) => {
    if (v !== interval.value) {
      interval.value = v;
      arm();
    }
  },
);

onMounted(arm);
onBeforeUnmount(clearTimer);
</script>

<style scoped>
.ops-auto-refresh {
  width: 140px;
}
</style>
