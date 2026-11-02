<template>
  <OpsPage :title="t('menus.mazbri.alertEvents')">
    <template #actions>
        <ElSelect
          v-model="filters.status"
          clearable
          :placeholder="t('ops.alerts.filters.status')"
          style="width: 140px"
          @change="search"
        >
          <ElOption value="active" :label="t('ops.alerts.status.active')" />
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
    </template>

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
          :timestamp="formatTime(item.started_at)"
          :type="timelineType(item)"
          placement="top"
        >
          <span class="ops-alerts__timeline-title">{{ item.rule_name }}</span>
          <ElTag size="small" :type="statusTag(item.status)">
            {{ t(`ops.alerts.status.${item.status}`) }}
          </ElTag>
          <span class="ops-alerts__muted">
            {{ labelText(item.labels) }} · {{ formatValue(item.value) }}
            {{ item.op }} {{ formatValue(item.threshold) }}
          </span>
          <span v-if="item.resolved_at" class="ops-alerts__muted">
            · {{ t("ops.alerts.resolvedAt") }}
            {{ formatTime(item.resolved_at) }}
          </span>
        </ElTimelineItem>
      </ElTimeline>
    </OpsCard>

    <OpsCard>
      <OpsTableWrap>
      <ElTable v-loading="loading" :data="rows" row-key="id" size="small" @row-click="selectRow">
        <ElTableColumn :label="t('ops.alerts.columns.severity')" width="100">
          <template #default="{ row }">
            <ElTag :type="severityTag(row.severity)" size="small">
              {{ t(`ops.alerts.severity.${row.severity}`) }}
            </ElTag>
          </template>
        </ElTableColumn>
        <ElTableColumn :label="t('ops.alerts.columns.status')" width="180">
          <template #default="{ row }">
            <ElTag :type="statusTag(row.status)" size="small">
              {{ t(`ops.alerts.status.${row.status}`) }}
            </ElTag>
            <ElTag v-if="row.silenced" type="info" size="small" class="ops-alerts__flag">
              {{ t("ops.alerts.silenced") }}
            </ElTag>
            <ElTag v-if="row.inhibited" type="warning" size="small" class="ops-alerts__flag">
              {{ t("ops.alerts.inhibited") }}
            </ElTag>
          </template>
        </ElTableColumn>
        <ElTableColumn
          prop="rule_name"
          :label="t('ops.alerts.columns.rule')"
          min-width="140"
        />
        <ElTableColumn :label="t('ops.alerts.columns.labels')" min-width="200">
          <template #default="{ row }">{{ labelText(row.labels) }}</template>
        </ElTableColumn>
        <ElTableColumn :label="t('ops.traces.traceIds')" min-width="150">
          <template #default="{ row }">
            <div v-if="row.trace_ids?.length" class="ops-alerts__traces">
              <ElLink
                v-for="tid in row.trace_ids"
                :key="tid"
                type="primary"
                @click="openTrace(row, tid)"
              >
                {{ tid.slice(0, 12) }}…
              </ElLink>
            </div>
            <span v-else>—</span>
          </template>
        </ElTableColumn>
        <ElTableColumn :label="t('ops.alerts.columns.value')" width="150">
          <template #default="{ row }">
            {{ formatValue(row.value) }} {{ row.op }}
            {{ formatValue(row.threshold) }}
          </template>
        </ElTableColumn>
        <ElTableColumn :label="t('ops.alerts.columns.startedAt')" width="170">
          <template #default="{ row }">{{
            formatTime(row.started_at)
          }}</template>
        </ElTableColumn>
        <ElTableColumn :label="t('ops.alerts.columns.resolvedAt')" width="170">
          <template #default="{ row }">
            {{ row.resolved_at ? formatTime(row.resolved_at) : "—" }}
          </template>
        </ElTableColumn>
        <ElTableColumn :label="t('ops.alerts.columns.ackedBy')" width="110">
          <template #default="{ row }">{{ row.acked_by || "—" }}</template>
        </ElTableColumn>
        <ElTableColumn :label="t('ops.alerts.columns.assignee')" width="110">
          <template #default="{ row }">{{ row.assignee || "—" }}</template>
        </ElTableColumn>
        <ElTableColumn
          v-if="canOperate"
          :label="t('ops.alerts.columns.actions')"
          width="170"
          fixed="right"
        >
          <template #default="{ row }">
            <ElButton
              v-if="row.status === 'firing'"
              link
              type="primary"
              @click="ack(row)"
            >
              {{ t("ops.alerts.ack") }}
            </ElButton>
            <ElButton link type="primary" @click="assign(row)">
              {{ t("ops.alerts.assign") }}
            </ElButton>
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
            :timestamp="c.ts"
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
  </OpsPage>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useRoute, useRouter } from "vue-router";
import { ElMessage, ElMessageBox } from "element-plus";
import AutoRefresh from "@/components/ops/AutoRefresh.vue";
import OpsTableWrap from "@/components/ops/OpsTableWrap.vue";
import OpsCard from "@/components/ops/OpsCard.vue";
import OpsPage from "@/components/ops/OpsPage.vue";
import {
  ackAlertEvent,
  assignAlertEvent,
  fetchAlertEvents,
} from "@/api/alerts";
import { fetchOpsChanges, type ChangeEvent } from "@/api/opsChanges";
import type {
  AlertEvent,
  AlertEventQuery,
  AlertSeverity,
  AlertStatus,
} from "@/types/api/alerts";
import { useUserStore } from "@/store/modules/user";
import { buildTraceRoute } from "@/utils/ops/trace-link";

defineOptions({ name: "OpsAlertEvents" });

const { t } = useI18n();
const route = useRoute();
const router = useRouter();

function openTrace(row: AlertEvent, traceId: string) {
  const started = Date.parse(row.started_at);
  void router.push(
    buildTraceRoute(traceId, {
      start: Number.isFinite(started)
        ? Math.floor(started / 1000) - 300
        : undefined,
    }),
  );
}
const userStore = useUserStore();

const statuses: AlertStatus[] = ["firing", "acked", "resolved"];
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

const roles = computed(() => (userStore.info?.roles as string[]) || []);
const canOperate = computed(() =>
  roles.value.some((role) => role === "R_ADMIN" || role === "R_FINANCE"),
);

function formatTime(value: string) {
  return new Date(value).toLocaleString();
}

function formatValue(value: number) {
  return new Intl.NumberFormat(undefined, { maximumFractionDigits: 2 }).format(
    value,
  );
}

function labelText(labels: Record<string, string>) {
  const text = Object.entries(labels)
    .map(([k, v]) => `${k}=${v}`)
    .join(", ");
  return text || "—";
}

function severityTag(severity: AlertSeverity) {
  if (severity === "critical") return "danger";
  if (severity === "warning") return "warning";
  return "info";
}

function statusTag(status: AlertStatus) {
  if (status === "firing") return "danger";
  if (status === "acked") return "warning";
  return "success";
}

function timelineType(item: AlertEvent) {
  if (item.status === "resolved") return "success";
  return item.severity === "critical" ? "danger" : "warning";
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
  void router.replace({ query: {} });
  search();
}

async function ack(row: AlertEvent) {
  await ackAlertEvent(row.id);
  ElMessage.success(t("ops.alerts.ackDone"));
  await reload();
}

async function assign(row: AlertEvent) {
  let value: string;
  try {
    const result = await ElMessageBox.prompt(
      t("ops.alerts.assignPrompt"),
      t("ops.alerts.assign"),
      {
        inputValue: row.assignee || userStore.info?.userName || "",
        inputPattern: /^.{0,64}$/,
        inputErrorMessage: t("ops.alerts.assignInvalid"),
      },
    );
    value = result.value;
  } catch {
    return;
  }
  const assignee = value.trim();
  await assignAlertEvent(row.id, assignee || null);
  ElMessage.success(t("ops.alerts.assignDone"));
  await reload();
}

const selected = ref<AlertEvent | null>(null);
const relatedChanges = ref<ChangeEvent[]>([]);

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
