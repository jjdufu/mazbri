<template>
  <button
    type="button"
    class="wb-adapter-chip"
    :title="chipTitle"
    :aria-label="chipAriaLabel"
    @click="emit('edit')"
  >
    <span class="wb-adapter-chip__text">{{ displayName }}</span>
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

const adapterCode = computed(() => props.adapter || "http_json");

/** 完整可见名 + 适配器码；窄宽 ellipsis 时靠 title/aria，禁止无 title 硬截断 */
const chipTitle = computed(
  () => `${displayName.value} (${adapterCode.value}) · ${$t("upstreamConnector.adapterChipHint")}`,
);

const chipAriaLabel = computed(
  () => `${displayName.value} (${adapterCode.value})`,
);
</script>

<style scoped lang="scss">
.wb-adapter-chip {
  flex: 0 1 auto;
  flex-shrink: 1;
  min-width: 4.5rem;
  max-width: 16rem;
  height: auto;
  min-height: var(--wb-urlbar-h, 32px);
  padding: 4px 10px;
  border: 1px solid var(--wb-divider-subtle, var(--el-border-color-extra-light));
  border-radius: var(--wb-radius, var(--art-form-control-radius));
  background: var(--wb-surface-input, var(--el-fill-color-light));
  color: var(--wb-text-muted, var(--el-text-color-secondary));
  font-size: var(--wb-fs-body, 13px);
  font-weight: 600;
  line-height: 1.25;
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  text-align: center;
  &:hover {
    color: var(--wb-text, var(--el-text-color-primary));
    background: var(--wb-surface-hover, var(--el-fill-color));
  }
}
.wb-adapter-chip__text {
  min-width: 0;
  overflow-wrap: anywhere;
  word-break: break-word;
  white-space: normal;
}
</style>
