<template>
  <span v-if="hint" class="call-monitor-mode-hint">
    <template v-if="hint.kind === 'latest'">
      <span>{{ $t("operations.monitoring.mode.latestPrefix", { asOf: hint.asOf }) }}</span>
      <strong class="call-monitor-mode-hint__size">{{
        sizeLabel
      }}</strong>
      <span>{{ $t("operations.monitoring.mode.latestSuffix") }}</span>
      <span class="call-monitor-mode-hint__countdown"
        >{{ hint.refreshIn }}s</span
      >
    </template>
    <template v-else>{{ hint.text }}</template>
  </span>
</template>

<script setup lang="ts">
import { computed } from "vue";

export type CallMonitorModeHintPayload =
  | {
      kind: "latest";
      asOf: string;
      size: number;

      refreshIn: number;
    }
  | {
      kind: "text";
      text: string;
    };

const props = defineProps<{
  hint: CallMonitorModeHintPayload | null;
}>();

const sizeLabel = computed(() => {
  if (!props.hint || props.hint.kind !== "latest") return "";
  return new Intl.NumberFormat().format(props.hint.size);
});
</script>

<style scoped lang="scss">
.call-monitor-mode-hint {
  display: inline;
  color: inherit;
  font: inherit;
}

.call-monitor-mode-hint__size {
  margin: 0 2px;
  color: var(--theme-color);
  font-weight: 700;
  font-variant-numeric: tabular-nums;
}

.call-monitor-mode-hint__countdown {
  margin-left: 8px;
  color: var(--art-gray-500);
  font-variant-numeric: tabular-nums;
  font-feature-settings: "tnum";
}
</style>
