<template>
  <button
    type="button"
    class="wb-adapter-chip"
    :title="hint"
    @click="emit('edit')"
  >
    {{ displayName }}
  </button>
</template>

<script setup lang="ts">
import { $t } from "@/locales";

const props = withDefaults(
  defineProps<{
    adapter?: string;
    encoding?: string;
  }>(),
  { adapter: "http_json", encoding: "" },
);

const emit = defineEmits<{
  edit: [];
}>();

const displayName = computed(() => {
  const enc = (props.encoding || "").trim().toLowerCase();
  if ((props.adapter || "http_json") === "http_json" && enc === "form") {
    return $t("upstreamConnector.adapterEncodingForm");
  }
  const a = props.adapter || "http_json";
  const key = `upstreamConnector.adapterNames.${a}`;
  const t = $t(key);
  return t === key ? a : t;
});

const hint = computed(() => {
  const a = props.adapter || "http_json";
  return `${$t("upstreamConnector.adapterChipHint")} (${a})`;
});
</script>

<style scoped lang="scss">
.wb-adapter-chip {
  flex-shrink: 1;
  min-width: 0;
  height: var(--wb-urlbar-h, 32px);
  max-width: 9rem;
  padding: 0 10px;
  border: 1px solid var(--wb-divider-subtle, var(--el-border-color-extra-light));
  border-radius: var(--wb-radius, var(--art-form-control-radius));
  background: var(--wb-surface-input, var(--el-fill-color-light));
  color: var(--wb-text-muted, var(--el-text-color-secondary));
  font-size: 12px;
  font-weight: 600;
  line-height: 1;
  cursor: pointer;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  &:hover {
    color: var(--wb-text, var(--el-text-color-primary));
    background: var(--wb-surface-hover, var(--el-fill-color));
  }
}
</style>
