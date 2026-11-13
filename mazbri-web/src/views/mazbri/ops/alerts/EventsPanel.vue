<template>
  <div class="ops-alerts-panel">
    <OpsPanelToolbar>
      <ElSelect
        v-model="filters.status"
        clearable
        :placeholder="t('ops.alerts.filters.status')"
        style="width: 140px"
        @change="search"
      >
        <ElOption
          v-for="s in statuses"
          :key="s"
          :value="s"
          :label="t(`ops.alerts.status.${s}`)"
        />
      </ElSelect>
      <ElSelect
        v-model="filters.severity"
        clearable
        :placeholder="t('ops.alerts.filters.severity')"
        style="width: 140px"
        @change="search"
      >
        <ElOption
          v-for="s in severities"
          :key="s"
          :value="s"
          :label="t(`ops.alerts.severity.${s}`)"
        />
      </ElSelect>
      <ElTag v-if="filters.event_id" closable @close="clearEventFilter">
        {{ t("ops.alerts.filters.event") }} {{ filters.event_id.slice(0, 8) }}
      </ElTag>
      <AutoRefresh v-model="refreshInterval" @tick="reload" />
    </OpsPanelToolbar>

    <OpsCard :title="t('ops.alerts.timeline')">
      <ElEmpty
        v-if="!rows.length"
        :description="t('ops.alerts.empty')"
        :image-size="60"
      />
      <ElTimeline v-else>
        <ElTimelineItem
          v-for="item in rows.slice(0, 10)"
          :key="item.id"
          :timestamp="formatAlertTime(item.started_at)"
          :type="alertTimelineType(item.status, item.severity)"
          placement="top"
        >
          <span class="ops-alerts__timeline-title">{{ item.rule_name }}</span>
          <ElTag size="small" :type="alertStatusTag(item.status)">
            {{ alertStatusText(item.status) }}
          </ElTag>
          <span class="ops-alerts__muted">
            {{ formatAlertLabels(item.labels) }} ·
            {{ formatAlertValue(item.value) }} {{ item.op }}
            {{ formatAlertValue(item.threshold) }}
          </span>
          <span v-if="item.resolved_at" class="ops-alerts__muted">
            · {{ t("ops.alerts.resolvedAt") }}
            {{ formatAlertTime(item.resolved_at) }}
          </span>
        </ElTimelineItem>
      </ElTimeline>
    </OpsCard>

    <OpsCard>
      <OpsTableWrap>
        <ElTable
          v-loading="loading"
          :data="rows"
          row-key="id"
          size="small"
          stripe
          class="surface-panel-table"
          @row-click="selectRow"
        >
          <ElTableColumn :label="t('ops.alerts.columns.severity')" width="76">
            <template #default="{ row }">
              <ElTag :type="alertSeverityTag(row.severity)" size="small">
                {{ alertSeverityText(row.severity) }}
              </ElTag>
            </template>
          </ElTableColumn>
          <ElTableColumn :label="t('ops.alerts.columns.status')" min-width="110">
            <template #default="{ row }">
              <ElTag :type="alertStatusTag(row.status)" size="small">
                {{ alertStatusText(row.status) }}
              </ElTag>
              <ElTag
                v-if="row.silenced"
                type="info"
                size="small"
                class="ops-alerts__flag"
              >
                {{ t("ops.alerts.silenced") }}
              </ElTag>
              <ElTag
                v-if="row.inhibited"
                type="warning"
                size="small"
                class="ops-alerts__flag"
              >
                {{ t("ops.alerts.inhibited") }}
              </ElTag>
            </template>
          </ElTableColumn>
          <ElTableColumn
            prop="rule_name"
            :label="t('ops.alerts.columns.rule')"
            min-width="130"
            show-overflow-tooltip
          />
          <ElTableColumn
            :label="t('ops.alerts.columns.labels')"
            min-width="160"
            show-overflow-tooltip
          >
            <template #default="{ row }">{{
              formatAlertLabels(row.labels)
            }}</template>
          </ElTableColumn>
          <ElTableColumn :label="t('ops.traces.traceIds')" min-width="120">
            <template #default="{ row }">
              <div v-if="row.trace_ids?.length" class="ops-alerts__traces">
                <CopyableText
                  v-for="tid in row.trace_ids"
                  :key="tid"
                  :value="tid"
                  compact
                  :link="traceRouteOf(row, tid)"
                  :label="t('ops.traces.columns.traceId')"
                />
              </div>
              <span v-else>—</span>
            </template>
          </ElTableColumn>
          <ElTableColumn
            :label="t('ops.alerts.columns.value')"
            min-width="110"
            show-overflow-tooltip
          >
            <template #default="{ row }">
              {{ formatAlertValue(row.value) }} {{ row.op }}
              {{ formatAlertValue(row.threshold) }}
            </template>
          </ElTableColumn>
          <ElTableColumn :label="t('ops.alerts.columns.startedAt')" width="148">
            <template #default="{ row }">{{
              formatAlertTime(row.started_at)
            }}</template>
          </ElTableColumn>
          <ElTableColumn :label="t('ops.alerts.columns.resolvedAt')" width="148">
            <template #default="{ row }">
              {{ row.resolved_at ? formatAlertTime(row.resolved_at) : "—" }}
            </template>
          </ElTableColumn>
          <ElTableColumn :label="t('ops.alerts.columns.actions')" width="92">
            <template #default="{ row }">
              <ElButton
                v-if="drillRouteOf(row)"
                link
                type="primary"
                @click.stop="goDrill(row)"
              >
                {{ t("ops.alerts.drillDown") }}
              </ElButton>
              <span v-else>—</span>
            </template>
          </ElTableColumn>
        </ElTable>
      </OpsTableWrap>
      <div v-if="selected" class="ops-alerts__related">
        <h3>{{ t("ops.alerts.relatedChanges") }} · {{ selected.rule_name }}</h3>
        <ElEmpty
          v-if="!relatedChanges.length"
          :description="t('ops.alerts.noRelatedChanges')"
          :image-size="48"
        />
        <ElTimeline v-else>
          <ElTimelineItem
            v-for="(c, idx) in relatedChanges"
            :key="idx"
            :timestamp="formatAlertTime(c.ts)"
            placement="top"
          >
            {{ c.kind }} · {{ c.title }}
            <span class="ops-alerts__muted">{{ c.actor || "" }}</span>
          </ElTimelineItem>
        </ElTimeline>
      </div>
      <div class="ops-alerts__pager">
        <ElPagination
          v-model:current-page="page"
          v-model:page-size="pageSize"
          :total="total"
          :page-sizes="[20, 50, 100]"
          layout="total, sizes, prev, pager, next"
          @current-change="reload"
          @size-change="search"
        />
      </div>
    </OpsCard>
  </div>
</template>

<script setup lang="ts">
import CopyableText from "@/components/common/copyable-text/index.vue";
import { onMounted, reactive, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useRoute, useRouter } from "vue-router";
import AutoRefresh from "@/components/ops/AutoRefresh.vue";
import OpsCard from "@/components/ops/OpsCard.vue";
import OpsPanelToolbar from "@/components/ops/OpsPanelToolbar.vue";
import OpsTableWrap from "@/components/ops/OpsTableWrap.vue";
import { fetchAlertEvents } from "@/api/alerts";
import { fetchOpsChanges, type ChangeEvent } from "@/api/opsChanges";
import type {
  AlertEvent,
  AlertEventQuery,
  AlertSeverity,
  AlertStatus,
} from "@/types/api/alerts";
import {
  alertSeverityTag,
  alertStatusTag,
  alertTimelineType,
  formatAlertLabels,
  formatAlertTime,
  formatAlertValue,
  resolveAlertI18nLabel,
} from "@/utils/ops/alerts-format";
import { getSeverityLabel, getStatusLabel } from "@/utils/display-labels";
import { useOpsRole } from "@/composables/ops/useOpsRole";
import {
  resolveAlertDrillRoute,
  stripAlertEventIdQuery,
} from "@/utils/ops/alerts-link";
import { buildTraceRoute } from "@/utils/ops/trace-link";

defineOptions({ name: "OpsAlertEventsPanel" });

const { t, te } = useI18n();
const { isAdmin, isBiz, isViewer } = useOpsRole();

function alertStatusText(status: string): string {
  const labeled = resolveAlertI18nLabel("ops.alerts.status", status, t, te);
  return labeled === status ? getStatusLabel(status) : labeled;
}

function alertSeverityText(severity: string): string {
  const labeled = resolveAlertI18nLabel("ops.alerts.severity", severity, t, te);
  return labeled === severity ? getSeverityLabel(severity) : labeled;
}

const route = useRoute();
const router = useRouter();

const statuses: AlertStatus[] = ["firing", "resolved"];
const severities: AlertSeverity[] = ["critical", "warning", "info"];

const loading = ref(false);
const rows = ref<AlertEvent[]>([]);
const total = ref(0);
const page = ref(1);
const pageSize = ref(20);
const refreshInterval = ref(30);
const filters = reactive<{
  status: AlertEventQuery["status"] | "";
  severity: AlertSeverity | "";
  event_id: string;
}>({
  status: "",
  severity: "",
  event_id:
    typeof route.query.event_id === "string" ? route.query.event_id : "",
});

const selected = ref<AlertEvent | null>(null);
const relatedChanges = ref<ChangeEvent[]>([]);

function traceRouteOf(row: AlertEvent, traceId: string) {
  const started = Date.parse(row.started_at);
  return buildTraceRoute(traceId, {
    start: Number.isFinite(started)
      ? Math.floor(started / 1000) - 300
      : undefined,
  });
}

function drillRouteOf(row: AlertEvent) {
  return resolveAlertDrillRoute(
    { labels: row.labels },
    { isAdmin: isAdmin.value, isBiz: isBiz.value, isViewer: isViewer.value },
  );
}

function goDrill(row: AlertEvent) {
  const target = drillRouteOf(row);
  if (target) {
    void router.push(target);
  }
}

async function reload() {
  loading.value = true;
  try {
    const res = await fetchAlertEvents({
      status: filters.status || undefined,
      severity: filters.severity || undefined,
      event_id: filters.event_id || undefined,
      page: page.value,
      page_size: pageSize.value,
    });
    rows.value = res.data;
    total.value = res.total;
  } finally {
    loading.value = false;
  }
}

function search() {
  page.value = 1;
  void reload();
}

function clearEventFilter() {
  filters.event_id = "";
  void router.replace({ query: stripAlertEventIdQuery(route.query) });
  search();
}

async function selectRow(row: AlertEvent) {
  selected.value = row;
  relatedChanges.value = [];
  const started = Date.parse(row.started_at);
  if (!Number.isFinite(started)) return;
  const start = Math.floor(started / 1000) - 300;
  const end = Math.floor(started / 1000) + 300;
  try {
    const res = await fetchOpsChanges({ start, end, limit: 20 });
    relatedChanges.value = res.events || [];
  } catch {
    relatedChanges.value = [];
  }
}

onMounted(reload);
</script>

<style scoped>
.ops-alerts__timeline-title {
  margin-right: 8px;
  font-weight: 600;
}
.ops-alerts__muted {
  margin-left: 8px;
  color: var(--el-text-color-secondary);
  font-size: 12px;
}
.ops-alerts__pager {
  display: flex;
  justify-content: flex-end;
  margin-top: 12px;
}
.ops-alerts__traces {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
}
.ops-alerts__flag {
  margin-left: 4px;
}
.ops-alerts__related {
  margin-top: 12px;
}
</style>
