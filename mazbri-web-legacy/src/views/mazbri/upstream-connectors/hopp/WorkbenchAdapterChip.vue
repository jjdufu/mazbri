<template>
  <HoppMenu
    v-model:visible="menuOpen"
    :append-to="appendTo"
    placement="bottom-end"
  >
    <template #trigger>
      <button
        type="button"
        class="wb-adapter-chip"
        :class="{
          'wb-adapter-chip--risk': isJobPoll,
          'wb-adapter-chip--pool': isWsUnary,
        }"
        :title="chipTitle"
        :aria-label="chipAriaLabel"
        :aria-expanded="menuOpen"
      >
        <span class="wb-adapter-chip__text">{{ displayName }}</span>
        <span v-if="isJobPoll" class="wb-adapter-chip__risk">{{
          $t("upstreamConnector.jobPollRiskTag")
        }}</span>
        <span v-else-if="isWsUnary" class="wb-adapter-chip__pool">{{
          $t("upstreamConnector.wsUnaryPoolTag")
        }}</span>
        <span class="hopp-menu-caret" aria-hidden="true" />
      </button>
    </template>
    <button
      v-for="item in EGRESS_ADAPTERS"
      :key="item"
      type="button"
      class="hopp-menu-item"
      :class="{ 'is-current': item === adapterCode }"
      :disabled="disabled"
      @click="onPickAdapter(item)"
    >
      {{ adapterLabel(item) }}
    </button>
    <div class="hopp-menu-divider" />
    <button
      type="button"
      class="hopp-menu-item"
      @click="onOpenSpec"
    >
      {{ $t("upstreamConnector.adapterChipHint") }}
    </button>
  </HoppMenu>
</template>

<script setup lang="ts">
import HoppMenu from "@/components/common/hopp-menu/index.vue";
import { $t } from "@/locales";
import { EGRESS_ADAPTERS } from "../workbenchHelpers";

const props = withDefaults(
  defineProps<{
    adapter?: string;
    encoding?: string;
    appendTo?: HTMLElement | string;
    disabled?: boolean;
  }>(),
  { adapter: "http_json", encoding: "", appendTo: undefined, disabled: false },
);

const emit = defineEmits<{
  edit: [];
  select: [string];
}>();

const menuOpen = ref(false);

function adapterLabel(a: string): string {
  const key = `upstreamConnector.adapterNames.${a}`;
  const t = $t(key);
  return t === key ? a : t;
}

function onPickAdapter(next: string) {
  menuOpen.value = false;
  if (props.disabled) return;
  emit("select", next);
}

function onOpenSpec() {
  menuOpen.value = false;
  emit("edit");
}

const isJobPoll = computed(() => (props.adapter || "http_json") === "job_poll");
const isWsUnary = computed(() => (props.adapter || "http_json") === "ws_unary");

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
const chipTitle = computed(() => {
  const base = `${displayName.value} (${adapterCode.value}) · ${$t("upstreamConnector.adapterChipHint")}`;
  if (isJobPoll.value) {
    return `${base} · ${$t("upstreamConnector.jobPollRiskTag")}`;
  }
  if (isWsUnary.value) {
    return `${base} · ${$t("upstreamConnector.wsUnaryPoolTag")} · ${$t("upstreamConnector.wsUnaryPoolHint")}`;
  }
  return base;
});

const chipAriaLabel = computed(() => {
  const base = `${displayName.value} (${adapterCode.value})`;
  if (isJobPoll.value) {
    return `${base}, ${$t("upstreamConnector.jobPollRiskTag")}`;
  }
  if (isWsUnary.value) {
    return `${base}, ${$t("upstreamConnector.wsUnaryPoolTag")}`;
  }
  return base;
});
</script>

<style scoped lang="scss">
.wb-adapter-chip {
  flex: 0 1 auto;
  flex-shrink: 1;
  min-width: 4.5rem;
  max-width: 18rem;
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
  gap: 6px;
  text-align: center;
  &:hover {
    color: var(--wb-text, var(--el-text-color-primary));
    background: var(--wb-surface-hover, var(--el-fill-color));
  }
  &--risk {
    border-color: color-mix(
      in srgb,
      var(--wb-danger, var(--el-color-danger)) 45%,
      var(--wb-divider-subtle, var(--el-border-color-extra-light))
    );
    color: var(--wb-danger, var(--el-color-danger));
  }
  &--pool {
    border-color: color-mix(
      in srgb,
      var(--wb-accent, var(--el-color-primary)) 40%,
      var(--wb-divider-subtle, var(--el-border-color-extra-light))
    );
  }
}
.wb-adapter-chip__text {
  min-width: 0;
  overflow-wrap: anywhere;
  word-break: break-word;
  white-space: normal;
}
.wb-adapter-chip__risk {
  flex: 0 0 auto;
  padding: 1px 6px;
  border-radius: 999px;
  font-size: 11px;
  font-weight: 700;
  line-height: 1.3;
  color: var(--wb-danger, var(--el-color-danger));
  background: color-mix(
    in srgb,
    var(--wb-danger, var(--el-color-danger)) 12%,
    transparent
  );
}
.wb-adapter-chip__pool {
  flex: 0 0 auto;
  padding: 1px 6px;
  border-radius: 999px;
  font-size: 11px;
  font-weight: 700;
  line-height: 1.3;
  color: var(--wb-accent, var(--el-color-primary));
  background: color-mix(
    in srgb,
    var(--wb-accent, var(--el-color-primary)) 12%,
    transparent
  );
}
</style>
