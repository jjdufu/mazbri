<template>
  <span v-if="text" class="copyable-text" :class="{ 'is-mono': mono }">
    <ElTooltip
      :content="text"
      placement="top"
      :show-after="200"
      :disabled="!showTooltip"
    >
      <RouterLink
        v-if="routeLink"
        :to="routeLink"
        class="copyable-text__value copyable-text__link"
        >{{ shown }}</RouterLink
      >
      <a
        v-else-if="hrefLink"
        :href="hrefLink"
        target="_blank"
        rel="noopener noreferrer"
        class="copyable-text__value copyable-text__link"
        >{{ shown }}</a
      >
      <span
        v-else
        class="copyable-text__value"
        :title="showTooltip ? undefined : text"
        >{{ shown }}</span
      >
    </ElTooltip>
    <button
      v-if="!mask"
      type="button"
      class="copyable-text__btn"
      :class="{ 'is-done': copied }"
      :title="$t('common.actions.copy')"
      :aria-label="
        label
          ? `${$t('common.actions.copy')} ${label}`
          : $t('common.actions.copy')
      "
      @click.stop.prevent="onCopy"
    >
      <ArtSvgIcon :icon="copied ? 'ri:check-line' : 'ri:file-copy-line'" />
    </button>
  </span>
  <span v-else class="copyable-text__empty">{{ EMPTY_PLACEHOLDER }}</span>
</template>

<script setup lang="ts">
import type { RouteLocationRaw } from "vue-router";
import { EMPTY_PLACEHOLDER } from "@/utils/format";
import { copyWithFeedback } from "@/utils/ui/copy-feedback";
import { abbreviateId } from "./abbreviate";

defineOptions({ name: "CopyableText" });

const props = withDefaults(
  defineProps<{
    /** 完整值：复制时只取它，不取 DOM 文本。 */
    value?: string | number | null;
    /** 显示文本（默认显示完整值；脱敏字段传脱敏后的文本）。 */
    display?: string;
    /** 列表里显示 8…6 缩写，tooltip 显示全文。 */
    compact?: boolean;
    /** 脱敏字段：只显示 display，不提供复制按钮。 */
    mask?: boolean;
    /** 可跳转的 ID：站内路由，或 http(s) 外链（新标签页打开）。 */
    link?: RouteLocationRaw | null;
    /** 字段名，用于按钮的 aria-label。 */
    label?: string;
    mono?: boolean;
  }>(),
  {
    value: null,
    display: "",
    compact: false,
    mask: false,
    link: null,
    label: "",
    mono: true,
  },
);

const COPIED_MS = 1000;
const copied = ref(false);
let timer: ReturnType<typeof setTimeout> | null = null;

const text = computed(() =>
  props.value === null || props.value === undefined
    ? ""
    : String(props.value).trim(),
);
const shown = computed(() => {
  if (props.display !== undefined && props.display !== "") return props.display;
  if (props.mask) return "******";
  return props.compact ? abbreviateId(text.value) : text.value;
});
const showTooltip = computed(() => !props.mask && shown.value !== text.value);
const routeLink = computed(() =>
  props.link &&
  !(typeof props.link === "string" && /^https?:\/\//i.test(props.link))
    ? props.link
    : null,
);
const hrefLink = computed(() =>
  typeof props.link === "string" && /^https?:\/\//i.test(props.link)
    ? props.link
    : null,
);

async function onCopy() {
  if (props.mask || !text.value) return;
  if (await copyWithFeedback(text.value)) {
    copied.value = true;
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => {
      copied.value = false;
      timer = null;
    }, COPIED_MS);
  }
}

onBeforeUnmount(() => {
  if (timer) clearTimeout(timer);
});
</script>

<style scoped lang="scss">
.copyable-text {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  max-width: 100%;
  min-width: 0;
  vertical-align: middle;

  &.is-mono .copyable-text__value {
    font-family: var(--el-font-family-mono, monospace);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
}

.copyable-text__value {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.copyable-text__link {
  color: var(--el-color-primary);
  text-decoration: none;

  &:hover {
    text-decoration: underline;
  }
}

.copyable-text__btn {
  display: inline-flex;
  flex: none;
  align-items: center;
  justify-content: center;
  padding: 2px;
  border: none;
  border-radius: var(--art-form-control-radius-sm, 4px);
  background: transparent;
  color: var(--el-text-color-secondary);
  font-size: 13px;
  cursor: pointer;
  opacity: 0.65;
  transition:
    opacity 0.15s ease,
    color 0.15s ease,
    background-color 0.15s ease;

  &:hover,
  &:focus-visible {
    opacity: 1;
    color: var(--el-color-primary);
    background: var(--el-fill-color-light);
  }

  &.is-done {
    opacity: 1;
    color: var(--el-color-success);
  }
}
</style>
