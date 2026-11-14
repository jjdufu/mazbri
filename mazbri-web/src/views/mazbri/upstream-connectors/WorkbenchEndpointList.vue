<template>
  <div class="wb-eps">
    <div class="wb-eps__head">
      <span class="wb-eps__title">{{ $t("upstreamConnector.endpoints") }}</span>
      <ElButton
        v-if="!readOnly"
        link
        type="primary"
        :aria-label="$t('common.actions.add')"
        @click="emit('add')"
      >
        <ArtSvgIcon icon="ri:add-circle-line" />
      </ElButton>
    </div>
    <ElInput
      v-model="q"
      size="small"
      clearable
      :aria-label="$t('upstreamConnector.searchEndpoints')"
      :placeholder="$t('upstreamConnector.searchEndpoints')"
      class="wb-eps__search"
    >
      <template #prefix>
        <ArtSvgIcon icon="ri:search-line" />
      </template>
    </ElInput>
    <VueDraggable
      class="wb-eps__list"
      :model-value="filtered"
      :animation="250"
      handle=".wb-eps__grip"
      draggable=".wb-eps__item"
      ghost-class="wb-eps__ghost"
      chosen-class="wb-eps__chosen"
      drag-class="wb-eps__drag"
      :disabled="readOnly || endpoints.length <= 1 || Boolean(q.trim())"
      :custom-update="keepSortableDom"
      @end="onSortEnd"
    >
      <template v-for="(ep, visIdx) in filtered" :key="ep.id || originalIndex(ep)">
      <div v-if="tagHeading(visIdx)" class="wb-eps__tag">{{ tagHeading(visIdx) }}</div>
      <div
        class="wb-eps__item"
        :class="{ active: originalIndex(ep) === activeIndex }"
        @click="emit('select', originalIndex(ep))"
      >
        <div class="wb-eps__line">
          <span
            v-if="!readOnly && endpoints.length > 1 && !q.trim()"
            class="wb-eps__grip"
            :title="$t('upstreamConnector.kvDrag')"
            @click.stop
          >
            <ArtSvgIcon icon="lucide:grip-vertical" />
          </span>
          <span class="wb-eps__method" :data-tone="methodTone(ep.method)">
            {{ displayMethodBadge(ep.method) }}
          </span>
          <div class="wb-eps__meta">
            <ElInput
              v-if="editingName === originalIndex(ep)"
              ref="nameInputRef"
              v-model="ep.name"
              size="small"
              class="wb-eps__name-input"
              :aria-label="$t('upstreamConnector.fields.epName')"
              :placeholder="$t('upstreamConnector.fields.epName')"
              @click.stop
              @keydown.enter.prevent="stopRename"
              @blur="stopRename"
            />
            <div
              v-else
              class="wb-eps__name"
              @click.stop="!readOnly && startRename(originalIndex(ep))"
            >
              {{ ep.name || ep.path || "endpoint" }}
            </div>
            <div class="wb-eps__path">{{ ep.path }}</div>
          </div>
          <button
            type="button"
            class="wb-eps__gate"
            :class="{
              on: ep.gate !== false,
              ok: ep.gate !== false && gateOk(ep),
              off: ep.gate === false,
            }"
            :disabled="readOnly"
            :title="
              ep.gate === false
                ? $t('upstreamConnector.gateOffHint')
                : $t('upstreamConnector.gateOnHint')
            "
            @click.stop="!readOnly && emit('toggle-gate', originalIndex(ep))"
          />
          <HoppMenu v-if="!readOnly" placement="bottom-end" @click.stop>
            <template #trigger>
              <button
                type="button"
                class="wb-eps__more"
                :aria-label="$t('upstreamConnector.sessionActions')"
                @click.stop
              >
                <ArtSvgIcon icon="ri:more-2-fill" />
              </button>
            </template>
            <button
              type="button"
              class="hopp-menu-item"
              @click="onMore('duplicate', originalIndex(ep))"
            >
              {{ $t("upstreamConnector.duplicate") }}
            </button>
            <button
              type="button"
              class="hopp-menu-item"
              @click="onMore('channel-map', originalIndex(ep))"
            >
              {{ $t("upstreamConnector.channelMapping") }}
            </button>
            <button
              v-if="endpoints.length > 1"
              type="button"
              class="hopp-menu-item is-divided is-danger"
              @click="onMore('delete', originalIndex(ep))"
            >
              {{ $t("common.actions.delete") }}
            </button>
          </HoppMenu>
        </div>
        <div
          v-if="originalIndex(ep) === activeIndex"
          class="wb-eps__map"
          @click.stop
        >
          <label class="wb-eps__map-row">
            <span class="wb-eps__map-label">{{
              $t("upstreamConnector.fields.channelCode")
            }}</span>
            <input
              ref="mapCodeRef"
              v-model="ep.channel_code"
              class="wb-eps__map-input"
              :disabled="readOnly"
              :placeholder="suggestChannelCode(ep, originalIndex(ep))"
              :aria-label="$t('upstreamConnector.fields.channelCode')"
              :title="$t('upstreamConnector.fields.channelCode')"
              spellcheck="false"
            />
          </label>
          <label class="wb-eps__map-row">
            <ElTooltip
              :content="$t('upstreamConnector.pathPrefixTooltip')"
              placement="top"
            >
              <span class="wb-eps__map-label is-help">{{
                $t("upstreamConnector.gatewayPrefixLabel")
              }}</span>
            </ElTooltip>
            <input
              v-model="ep.path_prefix"
              class="wb-eps__map-input"
              :disabled="readOnly"
              :placeholder="ep.path || '/'"
              :aria-label="$t('upstreamConnector.fields.pathPrefix')"
              :title="$t('upstreamConnector.pathPrefixTooltip')"
              spellcheck="false"
            />
          </label>
        </div>
      </div>
      </template>
      <ElEmpty
        v-if="!filtered.length"
        :description="$t('upstreamConnector.noEndpoints')"
        :image-size="48"
      />
    </VueDraggable>
    <div v-if="endpoints.length" class="wb-eps__gates">
      <span>{{ $t("upstreamConnector.gateShort") }}</span>
      <ElTag
        size="small"
        :type="gate.total > 0 && gate.ok === gate.total ? 'success' : 'warning'"
        effect="plain"
      >
        {{ gate.ok }}/{{ gate.total }}
      </ElTag>
    </div>
    <div v-if="runs.length" class="wb-eps__runs">
      <button
        type="button"
        class="wb-eps__runs-toggle"
        @click="runsOpen = !runsOpen"
      >
        <ArtSvgIcon :icon="runsOpen ? 'ri:arrow-down-s-line' : 'ri:arrow-right-s-line'" />
        {{ $t("upstreamConnector.sessionRuns") }}
        <span class="wb-eps__runs-n">{{ runs.length }}</span>
      </button>
      <div v-if="runsOpen" class="wb-eps__runs-list">
        <button
          v-for="r in runs"
          :key="r.id"
          type="button"
          class="wb-eps__run"
          :class="{ active: activeRunId === r.id }"
          @click="emit('select-run', r)"
        >
          <span class="mode">{{ runModeLabel(r) }}</span>
          <span class="ep">{{ r.endpoint_key || "—" }}</span>
          <span class="ms">{{ r.latency_ms ?? "—" }}ms</span>
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import type { InputInstance } from "element-plus";
import { VueDraggable } from "vue-draggable-plus";
import HoppMenu from "@/components/common/hopp-menu/index.vue";
import { $t } from "@/locales";
import type {
  ConnectorEndpoint,
  ConnectorRun,
} from "@/api/upstream-connectors";
import {
  BYPASS_MGMT_PROBE_PATH,
  displayMethodBadge,
  methodTone,
  suggestChannelCode,
  type GateEpStatus,
} from "./workbenchHelpers";

const props = withDefaults(
  defineProps<{
    endpoints: ConnectorEndpoint[];
    activeIndex: number;
    gateItems: GateEpStatus[];
    gate: { ok: number; total: number };
    runs?: ConnectorRun[];
    activeRunId?: string | null;
    /** 观察者只读：隐藏导入导出、增删改与拖拽。 */
    readOnly?: boolean;
  }>(),
  { runs: () => [], activeRunId: null, readOnly: false },
);

const emit = defineEmits<{
  add: [];
  select: [number];
  "toggle-gate": [number];
  action: [{ cmd: string; index: number }];
  reorder: [{ from: number; to: number }];
  "select-run": [ConnectorRun];
}>();

const runsOpen = ref(true);

const q = ref("");
const editingName = ref<number | null>(null);
const nameInputRef = ref<InputInstance | null>(null);
const mapCodeRef = ref<HTMLInputElement | null>(null);

function runModeLabel(r: ConnectorRun): string {
  if (r.mode === "live" && r.probe_path === BYPASS_MGMT_PROBE_PATH) {
    return $t("upstreamConnector.modeBadge.business");
  }
  const key = `upstreamConnector.modeBadge.${r.mode}`;
  const t = $t(key);
  return t === key ? r.mode : t;
}

const filtered = computed(() => {
  const s = q.value.trim().toLowerCase();
  if (!s) return props.endpoints;
  return props.endpoints.filter((ep) => {
    const blob = `${ep.method || ""} ${ep.name || ""} ${ep.path || ""}`.toLowerCase();
    return blob.includes(s);
  });
});

function originalIndex(ep: ConnectorEndpoint) {
  return props.endpoints.indexOf(ep);
}

function tagHeading(visIdx: number): string {
  const ep = filtered.value[visIdx];
  const tag = ep?.tag?.trim();
  if (!tag) return "";
  if (visIdx === 0) return tag;
  const prev = filtered.value[visIdx - 1]?.tag?.trim() || "";
  return prev === tag ? "" : tag;
}

function keepSortableDom(): void {
  /* Keep Sortable DOM during drag so sibling rows can animate. */
}

function onSortEnd(evt: {
  oldIndex?: number;
  newIndex?: number;
  oldDraggableIndex?: number;
  newDraggableIndex?: number;
}): void {
  const fromVis = evt.oldDraggableIndex ?? evt.oldIndex;
  const toVis = evt.newDraggableIndex ?? evt.newIndex;
  if (fromVis == null || toVis == null || fromVis === toVis) return;
  const fromEp = filtered.value[fromVis];
  const toEp = filtered.value[toVis];
  if (!fromEp || !toEp) return;
  emit("reorder", { from: originalIndex(fromEp), to: originalIndex(toEp) });
}

function gateOk(ep: ConnectorEndpoint) {
  const id = ep.id;
  const hit = props.gateItems.find((g) => g.id === id || g.name === ep.name);
  return hit?.ok;
}

function startRename(idx: number) {
  if (props.readOnly) return;
  emit("select", idx);
  editingName.value = idx;
  void nextTick(() => {
    nameInputRef.value?.focus();
    nameInputRef.value?.input?.select();
  });
}

function stopRename() {
  editingName.value = null;
}

function onMore(cmd: string, index: number) {
  if (cmd === "channel-map") {
    void focusChannelMap(index);
    return;
  }
  emit("action", { cmd, index });
}

async function focusChannelMap(index: number) {
  emit("select", index);
  await nextTick();
  await nextTick();
  mapCodeRef.value?.focus();
}

watch(
  () => props.activeIndex,
  (idx) => {
    if (editingName.value != null && editingName.value !== idx) {
      editingName.value = null;
    }
  },
);
</script>

<style scoped lang="scss">
.wb-eps {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
  padding: 10px 10px 8px;
}
.wb-eps__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 8px;
  flex-shrink: 0;
}
.wb-eps__title {
  font-weight: 600;
  font-size: 13px;
}
.wb-eps__search {
  margin-bottom: 8px;
  flex-shrink: 0;
}
.wb-eps__list {
  flex: 1;
  min-height: 0;
  overflow: auto;
}
.wb-eps__tag {
  font-size: var(--wb-fs-tiny, 11px);
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  color: var(--wb-text-muted, var(--el-text-color-secondary));
  padding: 8px 8px 4px;
}
.wb-eps__item {
  display: flex;
  flex-direction: column;
  align-items: stretch;
  gap: 6px;
  padding: 8px 6px;
  border-radius: var(--wb-radius, var(--art-form-control-radius));
  border-left: 3px solid transparent;
  cursor: pointer;
  margin-bottom: 2px;
  min-width: 0;
  color: var(--el-text-color-primary);
  &.active {
    background: color-mix(
      in srgb,
      var(--el-color-primary) 14%,
      var(--el-bg-color-overlay)
    );
    border-left-color: var(--el-color-primary);
    color: var(--el-text-color-primary);
  }
  &:hover {
    background: var(--el-fill-color-light);
  }
  &.active:hover {
    background: color-mix(
      in srgb,
      var(--el-color-primary) 18%,
      var(--el-bg-color-overlay)
    );
  }
  &:hover .wb-eps__grip {
    opacity: 1;
  }
}
.wb-eps__chosen {
  background: var(--el-fill-color-light);
}
.wb-eps__ghost {
  cursor: move;
  opacity: 1;
}
.wb-eps__drag {
  cursor: grabbing;
}
.wb-eps__grip {
  width: 16px;
  flex-shrink: 0;
  border: none;
  background: transparent;
  color: var(--wb-text-muted, var(--el-text-color-secondary));
  opacity: 0;
  cursor: grab;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  font-size: 12px;
  user-select: none;
  &:active {
    cursor: grabbing;
  }
}
.wb-eps__item.wb-eps__chosen .wb-eps__grip,
.wb-eps__item.wb-eps__drag .wb-eps__grip {
  cursor: grabbing;
  opacity: 1;
}
.wb-eps__line {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}
.wb-eps__method {
  font-size: var(--wb-fs-tiny, 11px);
  font-weight: 700;
  letter-spacing: 0.02em;
  width: 3.25rem;
  flex-shrink: 0;
  text-align: center;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  &[data-tone="get"] {
    color: var(--wb-method-get);
  }
  &[data-tone="post"] {
    color: var(--wb-method-post);
  }
  &[data-tone="put"] {
    color: var(--wb-method-put);
  }
  &[data-tone="patch"] {
    color: var(--wb-method-patch);
  }
  &[data-tone="delete"] {
    color: var(--wb-method-delete);
  }
  &[data-tone="head"] {
    color: var(--wb-method-head);
  }
  &[data-tone="options"] {
    color: var(--wb-method-options);
  }
  &[data-tone="default"] {
    color: var(--wb-method-default);
  }
}
.wb-eps__meta {
  min-width: 0;
  flex: 1;
}
.wb-eps__name {
  font-size: 13px;
  font-weight: 500;
  color: var(--el-text-color-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.wb-eps__name-input {
  width: 100%;
  :deep(.el-input__wrapper) {
    padding: 1px 8px;
  }
}
.wb-eps__path {
  font-size: var(--wb-fs-tiny, 11px);
  color: var(--el-text-color-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.wb-eps__map {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding-left: 8px;
  min-width: 0;
}
.wb-eps__map-row {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}
.wb-eps__map-label {
  flex: 0 0 auto;
  font-size: 11px;
  color: var(--el-text-color-secondary);
  white-space: nowrap;
  &.is-help {
    border-bottom: 1px dashed var(--el-border-color);
    cursor: help;
  }
}
.wb-eps__map-input {
  flex: 1;
  width: 100%;
  min-width: 0;
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 3px;
  outline: none;
  background: var(--el-fill-color-blank);
  font-size: var(--wb-fs-tiny, 11px);
  line-height: 18px;
  padding: 2px 6px;
  color: var(--el-text-color-primary);
  &:focus {
    border-color: var(--el-color-primary);
  }
  &::placeholder {
    color: var(--el-text-color-placeholder);
  }
}
.wb-eps__gate {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  border: 1.5px solid var(--el-color-warning);
  background: var(--el-color-warning);
  flex-shrink: 0;
  padding: 0;
  cursor: pointer;
  &.ok {
    border-color: var(--el-color-success);
    background: var(--el-color-success);
  }
  &.off {
    background: transparent;
    border-color: var(--el-border-color);
  }
}
.wb-eps__more {
  opacity: 0.4;
  border: none;
  background: transparent;
  color: inherit;
  cursor: pointer;
  padding: 4px;
  display: inline-flex;
  align-items: center;
  line-height: 1;
}
.wb-eps__item:hover .wb-eps__more {
  opacity: 1;
}
.wb-eps__gates {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding-top: 8px;
  font-size: 12px;
  color: var(--el-text-color-secondary);
  border-top: 1px solid var(--el-border-color-extra-light);
  flex-shrink: 0;
}
.wb-eps__runs {
  flex-shrink: 0;
  margin-top: 8px;
  padding-top: 8px;
  border-top: 1px solid var(--el-border-color-extra-light);
  min-height: 0;
}
.wb-eps__runs-toggle {
  display: flex;
  align-items: center;
  gap: 4px;
  width: 100%;
  border: none;
  background: transparent;
  padding: 0;
  font-size: 12px;
  color: var(--el-text-color-secondary);
  cursor: pointer;
}
.wb-eps__runs-n {
  margin-left: auto;
}
.wb-eps__runs-list {
  max-height: 140px;
  overflow: auto;
  margin-top: 6px;
}
.wb-eps__run {
  display: flex;
  gap: 6px;
  width: 100%;
  border: none;
  background: transparent;
  padding: 4px 2px;
  font-size: var(--wb-fs-tiny, 11px);
  color: var(--el-text-color-secondary);
  cursor: pointer;
  text-align: left;
  .ep {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  &.active,
  &:hover {
    color: var(--el-text-color-primary);
  }
}
</style>
