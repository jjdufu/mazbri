<template>
  <OpsPage :title="t('ops.traces.detail.title')">
    <template #actions>
      <div class="ops-trace__toolbar">
        <div class="ops-trace__title">
          <ElButton link type="primary" @click="goBack"
            >← {{ t("ops.traces.detail.back") }}</ElButton
          >
          <code class="ops-trace__id">{{ traceId }}</code>
        </div>
        <div v-if="detail" class="ops-trace__meta">
          <span
            >{{ t("ops.traces.detail.startedAt") }}
            {{ formatNs(detail.start_ns) }}</span
          >
          <span
            >{{ t("ops.traces.columns.duration") }}
            {{ detail.duration_ms.toFixed(2) }} ms</span
          >
          <span
            >{{ t("ops.traces.columns.spans") }} {{ detail.spans.length }}</span
          >
          <span>{{ t("ops.traces.columns.errors") }} {{ errorCount }}</span>
        </div>
      </div>
    </template>

    <ElAlert
      v-if="missing"
      type="warning"
      :closable="false"
      show-icon
      :title="t('ops.traces.fallback.title')"
      :description="t('ops.traces.fallback.description')"
    />

    <div v-if="detail" v-loading="loading" class="ops-trace__body">
      <div class="ops-trace__waterfall">
        <div class="ops-trace__legend">
          <span
            v-for="svc in detail.services"
            :key="svc"
            class="ops-trace__legend-item"
          >
            <i :style="{ background: serviceColor(svc) }"></i>{{ svc }}
          </span>
          <span class="ops-trace__legend-item">
            <i class="is-critical"></i>{{ t("ops.traces.detail.criticalPath") }}
          </span>
          <span class="ops-trace__legend-item"
            ><i class="is-error"></i>{{ t("ops.traces.detail.error") }}</span
          >
          <ElButton size="small" link @click="expandAll">{{
            t("ops.traces.detail.expandAll")
          }}</ElButton>
          <ElButton size="small" link @click="collapseAll">{{
            t("ops.traces.detail.collapseAll")
          }}</ElButton>
        </div>
        <div class="ops-trace__axis">
          <div class="ops-trace__axis-label"></div>
          <div class="ops-trace__axis-track">
            <span
              v-for="tick in ticks"
              :key="tick.percent"
              :style="{ left: `${tick.percent}%` }"
            >
              {{ tick.label }}
            </span>
          </div>
        </div>
        <div
          v-for="row in visibleRows"
          :key="row.span.span_id"
          class="ops-trace__row"
          :class="{
            'is-selected': selectedId === row.span.span_id,
            'is-critical': row.span.critical,
            'is-error': row.span.error,
          }"
          :data-span="row.span.span_name"
          @click="selectSpan(row.span.span_id)"
        >
          <div
            class="ops-trace__label"
            :style="{ paddingLeft: `${row.span.depth * 14 + 4}px` }"
          >
            <span
              v-if="row.span.child_count > 0"
              class="ops-trace__toggle"
              @click.stop="toggle(row.span.span_id)"
              >{{ collapsed.has(row.span.span_id) ? "▸" : "▾" }}</span
            >
            <span v-else class="ops-trace__toggle is-leaf">·</span>
            <i
              class="ops-trace__dot"
              :style="{ background: serviceColor(row.span.service_name) }"
            ></i>
            <span class="ops-trace__svc">{{ row.span.service_name }}</span>
            <span class="ops-trace__name" :title="row.span.span_name">{{
              row.span.span_name
            }}</span>
          </div>
          <div class="ops-trace__track">
            <div
              class="ops-trace__bar"
              :style="{
                left: `${row.left}%`,
                width: `${row.width}%`,
                background: row.span.error
                  ? '#f56c6c'
                  : serviceColor(row.span.service_name),
              }"
            ></div>
            <span
              class="ops-trace__dur"
              :style="{ left: `${Math.min(row.left + row.width, 88)}%` }"
              >{{ formatDurationNs(row.span.duration_ns) }}</span
            >
          </div>
        </div>
      </div>

      <div class="ops-trace__panel">
        <template v-if="selected">
          <h3>{{ selected.span_name }}</h3>
          <div class="ops-trace__panel-meta">
            <ElTag
              size="small"
              :color="serviceColor(selected.service_name)"
              effect="dark"
            >
              {{ selected.service_name }}
            </ElTag>
            <ElTag size="small" type="info">{{ selected.span_kind }}</ElTag>
            <ElTag v-if="selected.error" size="small" type="danger">{{
              selected.status_code
            }}</ElTag>
            <ElTag v-if="selected.critical" size="small" type="warning">
              {{ t("ops.traces.detail.criticalPath") }}
            </ElTag>
          </div>
          <ElDescriptions :column="1" size="small" border>
            <ElDescriptionsItem label="span_id">{{
              selected.span_id
            }}</ElDescriptionsItem>
            <ElDescriptionsItem label="parent">{{
              selected.parent_span_id || "—"
            }}</ElDescriptionsItem>
            <ElDescriptionsItem :label="t('ops.traces.columns.duration')">
              {{ formatDurationNs(selected.duration_ns) }}
            </ElDescriptionsItem>
            <ElDescriptionsItem :label="t('ops.traces.detail.offset')">
              {{
                formatDurationNs(selected.start_ns - (detail?.start_ns ?? 0))
              }}
            </ElDescriptionsItem>
            <ElDescriptionsItem v-if="selected.status_message" label="status">
              {{ selected.status_message }}
            </ElDescriptionsItem>
          </ElDescriptions>

          <ElTabs v-model="panelTab" class="ops-trace__panel-tabs">
            <ElTabPane
              :label="t('ops.traces.detail.attributes')"
              name="attributes"
            >
              <KeyValueList :value="selected.attributes" />
              <h4>{{ t("ops.traces.detail.resource") }}</h4>
              <KeyValueList :value="selected.resource" />
            </ElTabPane>
            <ElTabPane
              :label="`${t('ops.traces.detail.events')} (${spanEvents.length})`"
              name="events"
            >
              <ElEmpty v-if="!spanEvents.length" :image-size="40" />
              <div
                v-for="(ev, i) in spanEvents"
                :key="i"
                class="ops-trace__event"
                :class="{ 'is-error': isErrorEvent(ev) }"
              >
                <strong>{{ ev.name || "event" }}</strong>
                <span class="ops-trace__muted">{{
                  formatEventTime(ev.time)
                }}</span>
                <KeyValueList :value="ev.attributes ?? {}" />
              </div>
            </ElTabPane>
            <ElTabPane
              :label="`${t('ops.traces.detail.links')} (${spanLinks.length + spanLinkedFrom.length})`"
              name="links"
            >
              <ElEmpty
                v-if="!spanLinks.length && !spanLinkedFrom.length"
                :image-size="40"
              />
              <div
                v-for="link in spanLinks"
                :key="`${link.trace_id}-${link.span_id}`"
                class="ops-trace__link"
              >
                <span class="ops-trace__muted">{{
                  t("ops.traces.detail.linksTo")
                }}</span>
                <ElLink
                  type="primary"
                  @click="openLinked(link.trace_id, link.span_id)"
                >
                  {{ link.trace_id }} / {{ link.span_id }}
                </ElLink>
              </div>
              <div
                v-for="link in spanLinkedFrom"
                :key="`${link.trace_id}-${link.span_id}`"
                class="ops-trace__link"
              >
                <span class="ops-trace__muted">{{
                  t("ops.traces.detail.linkedFrom")
                }}</span>
                <ElLink
                  type="primary"
                  @click="openLinked(link.trace_id, link.span_id)"
                >
                  {{ link.service_name }} · {{ link.span_name }}
                </ElLink>
              </div>
            </ElTabPane>
          </ElTabs>
        </template>
        <ElEmpty
          v-else
          :description="t('ops.traces.detail.selectSpan')"
          :image-size="60"
        />
      </div>
    </div>

    <OpsCard
      v-if="detail && detail.linked_from.length"
      :title="t('ops.traces.detail.asyncTitle')"
    >
      <div
        v-for="link in detail.linked_from"
        :key="`${link.trace_id}-${link.span_id}`"
        class="ops-trace__link"
      >
        <ElTag
          size="small"
          :color="serviceColor(link.service_name)"
          effect="dark"
          >{{ link.service_name }}</ElTag
        >
        <ElLink type="primary" @click="openLinked(link.trace_id, link.span_id)">
          {{ link.span_name }} · {{ link.trace_id }}
        </ElLink>
        <span class="ops-trace__muted">← {{ link.linked_span_id }}</span>
      </div>
    </OpsCard>

    <OpsCard>
      <ElTabs v-model="bottomTab">
        <ElTabPane
          :label="`${t('ops.traces.detail.logs')} (${calls.length})`"
          name="logs"
        >
          <ElTable v-loading="callsLoading" :data="calls" stripe size="small">
            <ElTableColumn
              :label="t('ops.calls.stages.columns.time')"
              min-width="180"
            >
              <template #default="{ row }">{{
                formatTime(row.timestamp)
              }}</template>
            </ElTableColumn>
            <ElTableColumn
              prop="target"
              :label="t('ops.calls.stages.columns.source')"
              min-width="150"
            />
            <ElTableColumn prop="request_id" label="request_id" min-width="280">
              <template #default="{ row }">
                <ElLink type="primary" @click="openCalls(row.request_id)">{{
                  row.request_id
                }}</ElLink>
              </template>
            </ElTableColumn>
            <ElTableColumn
              prop="supplier_code"
              :label="t('ops.calls.stages.columns.supplier')"
              min-width="110"
            />
            <ElTableColumn
              prop="channel_code"
              :label="t('ops.calls.stages.columns.channel')"
              min-width="130"
            />
            <ElTableColumn
              prop="status_code"
              :label="t('ops.calls.stages.columns.status')"
              width="80"
            />
            <ElTableColumn
              prop="duration_ms"
              :label="t('ops.calls.stages.columns.duration')"
              width="100"
            />
            <ElTableColumn
              prop="upstream_service_time_ms"
              label="upstream ms"
              width="110"
            />
            <ElTableColumn
              prop="error_code"
              :label="t('ops.calls.stages.columns.errorCode')"
              min-width="130"
            />
          </ElTable>
        </ElTabPane>
        <ElTabPane :label="t('ops.traces.detail.stages')" name="stages">
          <StageBreakdown :stages="stages" />
        </ElTabPane>
        <ElTabPane :label="t('ops.traces.detail.billing')" name="billing">
          <div v-if="billingError" class="ops-trace__muted">
            {{ billingError }}
          </div>
          <ElTable
            v-else
            v-loading="billingLoading"
            :data="billing"
            stripe
            size="small"
          >
            <ElTableColumn
              prop="request_id"
              label="request_id"
              min-width="260"
            />
            <ElTableColumn prop="outbox_status" label="outbox" width="110" />
            <ElTableColumn
              prop="retry_count"
              :label="t('ops.traces.billing.retries')"
              width="80"
            />
            <ElTableColumn
              prop="product_id"
              :label="t('ops.traces.form.product')"
              min-width="120"
            />
            <ElTableColumn
              prop="billable_units"
              :label="t('ops.traces.billing.units')"
              width="90"
            />
            <ElTableColumn
              prop="billable_amount"
              :label="t('ops.traces.billing.amount')"
              width="110"
            />
            <ElTableColumn
              prop="charge_status"
              :label="t('ops.traces.billing.charge')"
              width="110"
            />
            <ElTableColumn
              :label="t('ops.traces.billing.processedAt')"
              min-width="170"
            >
              <template #default="{ row }">{{
                row.processed_at ? formatTime(row.processed_at) : "—"
              }}</template>
            </ElTableColumn>
            <ElTableColumn
              prop="error_message"
              :label="t('ops.traces.billing.error')"
              min-width="160"
            />
          </ElTable>
        </ElTabPane>
      </ElTabs>
    </OpsCard>
  </OpsPage>
</template>

<script setup lang="ts">
import { formatDateTime } from "@/utils/format";
import { computed, defineComponent, h, ref, watch, type PropType } from "vue";
import { useRoute, useRouter } from "vue-router";
import { useI18n } from "vue-i18n";
import { ElMessage } from "element-plus";
import OpsPage from "@/components/ops/OpsPage.vue";
import OpsCard from "@/components/ops/OpsCard.vue";
import { fetchTrace, fetchTraceBilling } from "@/api/traces";
import { fetchCallStages } from "@/api/call-monitor";
import type {
  LinkedSpan,
  TraceBillingRow,
  TraceDetail,
  TraceSpan,
  TraceSpanEvent,
  TraceSpanLink,
} from "@/types/api/traces";
import type {
  CallStageAccessRow,
  CallStageEntry,
} from "@/types/api/call-monitor";
import { HttpError } from "@/utils/http/error";
import { buildCallsRoute } from "@/utils/ops/calls-link";
import { buildTraceRoute, TRACES_PATH } from "@/utils/ops/trace-link";
import { logger } from "@/utils/sys";
import StageBreakdown from "../calls/modules/StageBreakdown.vue";

defineOptions({ name: "OpsTraceDetail" });

const KeyValueList = defineComponent({
  props: {
    value: {
      type: Object as PropType<Record<string, unknown>>,
      required: true,
    },
  },
  setup(props) {
    return () => {
      const entries = Object.entries(props.value ?? {}).sort(([a], [b]) =>
        a.localeCompare(b),
      );
      if (!entries.length) return h("div", { class: "ops-trace__muted" }, "—");
      return h(
        "dl",
        { class: "ops-trace__kv" },
        entries.flatMap(([k, v]) => [
          h("dt", k),
          h("dd", typeof v === "string" ? v : JSON.stringify(v)),
        ]),
      );
    };
  },
});

const { t } = useI18n();
const route = useRoute();
const router = useRouter();

const PALETTE = [
  "#409eff",
  "#67c23a",
  "#e6a23c",
  "#9b59b6",
  "#16a085",
  "#d35400",
  "#2c3e50",
  "#c0392b",
  "#7f8c8d",
  "#1abc9c",
  "#8e44ad",
];

const traceId = computed(() =>
  String(route.params.traceId ?? "").toLowerCase(),
);
const loading = ref(false);
const missing = ref(false);
const detail = ref<TraceDetail | null>(null);
const selectedId = ref("");
const collapsed = ref(new Set<string>());
const panelTab = ref("attributes");
const bottomTab = ref("logs");

const calls = ref<CallStageAccessRow[]>([]);
const stages = ref<CallStageEntry[]>([]);
const callsLoading = ref(false);
const billing = ref<TraceBillingRow[]>([]);
const billingLoading = ref(false);
const billingError = ref("");

const serviceIndex = computed(() => {
  const map = new Map<string, number>();
  (detail.value?.services ?? []).forEach((s, i) => map.set(s, i));
  return map;
});

function serviceColor(service: string): string {
  const idx = serviceIndex.value.get(service);
  if (idx !== undefined) return PALETTE[idx % PALETTE.length];
  let hash = 0;
  for (const ch of service) hash = (hash * 31 + ch.charCodeAt(0)) >>> 0;
  return PALETTE[hash % PALETTE.length];
}

const errorCount = computed(
  () => detail.value?.spans.filter((s) => s.error).length ?? 0,
);

interface WaterfallRow {
  span: TraceSpan;
  left: number;
  width: number;
}

const visibleRows = computed<WaterfallRow[]>(() => {
  const d = detail.value;
  if (!d) return [];
  const total = Math.max(1, d.end_ns - d.start_ns);
  const rows: WaterfallRow[] = [];
  let hideBelow: number | null = null;
  for (const span of d.spans) {
    if (hideBelow !== null) {
      if (span.depth > hideBelow) continue;
      hideBelow = null;
    }
    const left = ((span.start_ns - d.start_ns) / total) * 100;
    const width = Math.max(0.3, (span.duration_ns / total) * 100);
    rows.push({
      span,
      left: Math.max(0, left),
      width: Math.min(width, 100 - Math.max(0, left)),
    });
    if (collapsed.value.has(span.span_id)) hideBelow = span.depth;
  }
  return rows;
});

const ticks = computed(() => {
  const d = detail.value;
  if (!d) return [];
  const totalMs = (d.end_ns - d.start_ns) / 1e6;
  return [0, 25, 50, 75, 100].map((percent) => ({
    percent,
    label: `${((totalMs * percent) / 100).toFixed(totalMs < 10 ? 2 : 0)} ms`,
  }));
});

const selected = computed(
  () => detail.value?.spans.find((s) => s.span_id === selectedId.value) ?? null,
);

const spanEvents = computed<TraceSpanEvent[]>(() => {
  const ev = selected.value?.events;
  return Array.isArray(ev) ? (ev as TraceSpanEvent[]) : [];
});

const spanLinks = computed<TraceSpanLink[]>(() => {
  const links = selected.value?.links;
  if (!Array.isArray(links)) return [];
  return (links as TraceSpanLink[]).filter((l) => l && l.trace_id && l.span_id);
});

const spanLinkedFrom = computed<LinkedSpan[]>(() =>
  (detail.value?.linked_from ?? []).filter(
    (l) => l.linked_span_id === selectedId.value,
  ),
);

function isErrorEvent(ev: TraceSpanEvent): boolean {
  return (
    (ev.name ?? "").toLowerCase().includes("exception") ||
    (ev.name ?? "").toLowerCase().includes("error")
  );
}

function formatNs(ns: number): string {
  return formatDateTime(Math.floor(ns / 1e6));
}

function formatTime(value: string): string {
  const d = new Date(value);
  return Number.isNaN(d.getTime()) ? value : formatDateTime(d);
}

function formatEventTime(value: number | string | undefined): string {
  if (value === undefined) return "";
  if (typeof value === "number") {
    const offset = value - (detail.value?.start_ns ?? value);
    return `+${formatDurationNs(offset)}`;
  }
  return value;
}

function formatDurationNs(ns: number): string {
  const ms = ns / 1e6;
  if (ms >= 1000) return `${(ms / 1000).toFixed(2)} s`;
  if (ms >= 1) return `${ms.toFixed(2)} ms`;
  return `${(ns / 1000).toFixed(0)} µs`;
}

function toggle(id: string) {
  const next = new Set(collapsed.value);
  if (next.has(id)) next.delete(id);
  else next.add(id);
  collapsed.value = next;
}

function expandAll() {
  collapsed.value = new Set();
}

function collapseAll() {
  const d = detail.value;
  if (!d) return;
  collapsed.value = new Set(
    d.spans
      .filter((s) => s.depth >= 1 && s.child_count > 0)
      .map((s) => s.span_id),
  );
}

function selectSpan(id: string) {
  selectedId.value = id;
  void router.replace({ query: { ...route.query, span: id } });
}

function openLinked(target: string, spanId: string) {
  if (target === traceId.value) {
    selectSpan(spanId);
    return;
  }
  void router.push(buildTraceRoute(target, { span: spanId }));
}

function openCalls(requestId: string) {
  void router.push(
    buildCallsRoute({ tab: "customer", filters: { request_id: requestId } }),
  );
}

function goBack() {
  if (window.history.state?.back) router.back();
  else void router.push(TRACES_PATH);
}

function startHint(): { start?: number; end?: number } {
  const raw = Number(route.query.start);
  if (!Number.isFinite(raw) || raw <= 0) return {};
  return { start: Math.floor(raw) - 300, end: Math.floor(raw) + 3 * 3600 };
}

/**
 * Initial selection: the `span` query, else the first error span, else the
 * longest CLIENT span on the critical path (usually `upstream.attempt`).
 */
function defaultSelection(d: TraceDetail): string {
  const wanted = typeof route.query.span === "string" ? route.query.span : "";
  if (wanted && d.spans.some((s) => s.span_id === wanted)) return wanted;
  const firstError = d.spans.find((s) => s.error);
  if (firstError) return firstError.span_id;
  const longest = (list: TraceSpan[]) =>
    list.reduce<TraceSpan | null>(
      (best, s) => (!best || s.duration_ns > best.duration_ns ? s : best),
      null,
    );
  const critical = d.spans.filter((s) => s.critical && s.depth > 0);
  const client = longest(
    critical.filter((s) => s.span_kind === "SPAN_KIND_CLIENT"),
  );
  return (
    client?.span_id ?? longest(critical)?.span_id ?? d.spans[0]?.span_id ?? ""
  );
}

async function loadBilling(requestIds: string[]) {
  billingError.value = "";
  billing.value = [];
  if (!requestIds.length) return;
  billingLoading.value = true;
  try {
    billing.value = await fetchTraceBilling(requestIds.slice(0, 50));
  } catch (error) {
    billingError.value =
      error instanceof HttpError && error.code === 403
        ? t("ops.traces.billing.forbidden")
        : t("ops.traces.billing.loadFailed");
  } finally {
    billingLoading.value = false;
  }
}

async function loadCalls(id: string) {
  callsLoading.value = true;
  try {
    const fallbackRequest =
      typeof route.query.request_id === "string" ? route.query.request_id : "";
    let res = await fetchCallStages({ trace_id: id });
    if (!res.calls.length && fallbackRequest) {
      res = await fetchCallStages({ request_id: fallbackRequest });
    }
    calls.value = res.calls;
    stages.value = res.stages;
    const ids = [
      ...new Set(res.calls.map((c) => c.request_id).filter(Boolean)),
    ];
    void loadBilling(ids);
  } catch (error) {
    logger.error("Failed to load trace calls:", error);
    calls.value = [];
    stages.value = [];
  } finally {
    callsLoading.value = false;
  }
}

let seq = 0;
async function load() {
  const id = traceId.value;
  const mine = ++seq;
  if (!/^[0-9a-f]{32}$/.test(id)) {
    ElMessage.error(t("ops.traces.invalidId"));
    return;
  }
  loading.value = true;
  missing.value = false;
  detail.value = null;
  collapsed.value = new Set();
  void loadCalls(id);
  try {
    const res = await fetchTrace(id, startHint());
    if (mine !== seq) return;
    detail.value = res;
    selectedId.value = defaultSelection(res);
  } catch (error) {
    if (mine !== seq) return;
    if (error instanceof HttpError && error.code === 404) {
      missing.value = true;
      bottomTab.value = "stages";
    } else {
      logger.error("Failed to load trace:", error);
      ElMessage.error(t("ops.traces.loadFailed"));
    }
  } finally {
    if (mine === seq) loading.value = false;
  }
}

watch(traceId, () => void load(), { immediate: true });
</script>

<style scoped>
.ops-trace__toolbar {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  align-items: center;
  justify-content: space-between;
  width: 100%;
}
.ops-trace__title {
  display: flex;
  gap: 10px;
  align-items: center;
}
.ops-trace__id {
  font-size: 13px;
  color: var(--el-text-color-secondary);
}
.ops-trace__meta {
  display: flex;
  gap: 16px;
  font-size: 13px;
  color: var(--el-text-color-regular);
}
.ops-trace__body {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 380px;
  gap: 12px;
}
.ops-trace__waterfall,
.ops-trace__panel,
.ops-trace__card {
  padding: 12px 16px;
  background: var(--el-bg-color);
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 8px;
}
/* 右栏粘性：瀑布图超长时跟随滚动停在顶栏下方，自身限高滚动（不拉伸到瀑布图高度） */
.ops-trace__panel {
  position: sticky;
  top: var(--ops-sticky-top);
  align-self: start;
  max-height: calc(100vh - var(--ops-sticky-top) - var(--ops-space-md));
  overflow: auto;
}
.ops-trace__panel h3,
.ops-trace__card h3 {
  margin: 0 0 8px;
  font-size: 15px;
  word-break: break-all;
}
.ops-trace__panel h4 {
  margin: 12px 0 4px;
  font-size: 13px;
}
.ops-trace__panel-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-bottom: 8px;
}
.ops-trace__legend {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  align-items: center;
  margin-bottom: 8px;
  font-size: 12px;
}
.ops-trace__legend-item {
  display: inline-flex;
  gap: 4px;
  align-items: center;
}
.ops-trace__legend-item i {
  display: inline-block;
  width: 10px;
  height: 10px;
  border-radius: 2px;
}
.ops-trace__legend-item i.is-critical {
  background: transparent;
  border: 2px solid #e6a23c;
}
.ops-trace__legend-item i.is-error {
  background: #f56c6c;
}
.ops-trace__axis,
.ops-trace__row {
  display: grid;
  grid-template-columns: 340px minmax(0, 1fr);
  align-items: center;
}
.ops-trace__axis-track {
  position: relative;
  height: 18px;
  font-size: 11px;
  color: var(--el-text-color-secondary);
  border-bottom: 1px solid var(--el-border-color-lighter);
}
.ops-trace__axis-track span {
  position: absolute;
  transform: translateX(-50%);
}
.ops-trace__axis-track span:first-child {
  transform: none;
}
.ops-trace__axis-track span:last-child {
  transform: translateX(-100%);
}
.ops-trace__row {
  min-height: 24px;
  font-size: 12px;
  cursor: pointer;
  border-left: 3px solid transparent;
}
.ops-trace__row:hover {
  background: var(--el-fill-color-light);
}
.ops-trace__row.is-critical {
  border-left-color: #e6a23c;
}
.ops-trace__row.is-selected {
  background: var(--el-color-primary-light-9);
}
.ops-trace__row.is-error .ops-trace__name {
  color: #f56c6c;
}
.ops-trace__label {
  display: flex;
  gap: 4px;
  align-items: center;
  overflow: hidden;
  white-space: nowrap;
}
.ops-trace__toggle {
  width: 12px;
  color: var(--el-text-color-secondary);
  text-align: center;
}
.ops-trace__toggle.is-leaf {
  opacity: 0.4;
}
.ops-trace__dot {
  flex: none;
  width: 8px;
  height: 8px;
  border-radius: 50%;
}
.ops-trace__svc {
  color: var(--el-text-color-secondary);
}
.ops-trace__name {
  overflow: hidden;
  text-overflow: ellipsis;
}
.ops-trace__track {
  position: relative;
  height: 24px;
}
.ops-trace__bar {
  position: absolute;
  top: 6px;
  height: 12px;
  border-radius: 2px;
  opacity: 0.85;
}
.ops-trace__row.is-critical .ops-trace__bar {
  outline: 2px solid #e6a23c;
  opacity: 1;
}
.ops-trace__dur {
  position: absolute;
  top: 4px;
  padding-left: 4px;
  font-size: 11px;
  color: var(--el-text-color-secondary);
  white-space: nowrap;
}
.ops-trace__event {
  padding: 6px 8px;
  margin-bottom: 6px;
  background: var(--el-fill-color-lighter);
  border-radius: 4px;
}
.ops-trace__event.is-error {
  border-left: 3px solid #f56c6c;
}
.ops-trace__link {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: center;
  margin-bottom: 6px;
}
.ops-trace__muted {
  font-size: 12px;
  color: var(--el-text-color-secondary);
}
.ops-trace__kv {
  display: grid;
  grid-template-columns: max-content minmax(0, 1fr);
  gap: 2px 10px;
  margin: 0;
  font-size: 12px;
}
.ops-trace__kv :deep(dt) {
  color: var(--el-text-color-secondary);
}
.ops-trace__kv :deep(dd) {
  margin: 0;
  word-break: break-all;
}
@media (width <= 1200px) {
  .ops-trace__body {
    grid-template-columns: 1fr;
  }
  .ops-trace__panel {
    position: static;
  }
}
</style>
