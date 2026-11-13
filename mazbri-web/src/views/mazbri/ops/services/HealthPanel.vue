<template>
  <div v-loading="loading" class="ops-services-health-panel">
    <OpsCard :title="cardTitle">
      <template v-if="filteredRows.length > 0" #extra>
        <span
          class="ops-services-health-panel__summary"
          :data-abnormal="abnormalCount"
        >
          {{
            abnormalCount > 0
              ? t("ops.components.abnormalOf", {
                  n: abnormalCount,
                  total: filteredRows.length,
                })
              : t("ops.components.allHealthy", { total: filteredRows.length })
          }}
        </span>
      </template>
      <ServiceHexagonGrid
        v-model:selected-id="selectedService"
        :nodes="hexagonNodes"
      >
        <template #empty>
          {{ t("ops.services.empty") }}
        </template>
      </ServiceHexagonGrid>
    </OpsCard>

    <OpsCard
      v-if="selectedRow"
      :title="
        t('ops.services.detailTitle', {
          name: selectedRow.name || selectedRow.service,
        })
      "
      class="ops-services-health-panel__detail"
    >
      <template #extra>
        <div class="ops-services-health-panel__extra-actions">
          <template v-if="isSelectedApp && isAdmin">
            <ElButton link type="primary" @click="openDetail(selectedRow)">
              {{ t("ops.services.openDetail") }}
            </ElButton>
            <ElButton link type="primary" @click="openTraces(selectedRow)">
              {{ t("ops.services.openTraces") }}
            </ElButton>
          </template>
          <ElButton
            v-else-if="
              selectedRow.group === 'observability' && isAdmin
            "
            link
            type="primary"
            @click="
              router.push(
                buildRuntimeRoute({
                  tab: selectedRow.service === 'vector' ? 'pipeline' : 'hosts',
                }),
              )
            "
          >
            {{
              selectedRow.service === "vector"
                ? t("ops.services.openPipeline")
                : t("ops.services.openHosts")
            }}
          </ElButton>
          <ElButton link @click="selectedService = null">
            {{ t("ops.services.clearSelection") }}
          </ElButton>
        </div>
      </template>

      <ElDescriptions
        :column="3"
        border
        size="small"
        class="ops-services-health-panel__descriptions"
      >
        <ElDescriptionsItem :label="t('ops.services.columns.service')">
          {{ selectedRow.name || selectedRow.service }}
        </ElDescriptionsItem>
        <ElDescriptionsItem :label="t('ops.services.columns.key')">
          <span class="ops-services-health-panel__mono">{{
            selectedRow.service
          }}</span>
        </ElDescriptionsItem>
        <ElDescriptionsItem :label="t('ops.services.columns.group')">
          {{ groupText(selectedRow) }}
        </ElDescriptionsItem>

        <ElDescriptionsItem :label="t('ops.services.columns.status')">
          <ElTag :type="statusTagType(selectedRow)" size="small">
            {{ statusText(selectedRow) }}
          </ElTag>
        </ElDescriptionsItem>
        <ElDescriptionsItem :label="t('ops.services.columns.health')">
          <ElTag
            :type="selectedRow.health_ok ? 'success' : 'danger'"
            size="small"
            effect="plain"
          >
            {{ boolText(selectedRow.health_ok) }}
          </ElTag>
        </ElDescriptionsItem>
        <ElDescriptionsItem :label="t('ops.services.columns.ready')">
          {{ boolText(selectedRow.ready) }}
        </ElDescriptionsItem>

        <ElDescriptionsItem :label="t('ops.services.columns.configCenter')">
          {{ boolText(selectedRow.config_center) }}
        </ElDescriptionsItem>
        <ElDescriptionsItem :label="t('ops.services.columns.container')">
          {{ containerText(selectedRow) }}
        </ElDescriptionsItem>
        <ElDescriptionsItem :label="t('ops.services.columns.dbPool')">
          <span
            v-if="!hasPostgresPool(selectedRow)"
            class="ops-services-health-panel__muted"
          >
            {{ t("ops.services.notApplicable") }}
          </span>
          <ElTag
            v-else-if="
              selectedRow.db_pool_usage === null ||
              selectedRow.db_pool_usage === undefined
            "
            type="warning"
            size="small"
            effect="plain"
          >
            {{ t("ops.services.unreported") }} (—)
          </ElTag>
          <span v-else>{{ percent(selectedRow.db_pool_usage) }}</span>
        </ElDescriptionsItem>

        <ElDescriptionsItem :label="t('ops.services.columns.cpu')">
          {{
            formatCpuCoresDisplay(
              selectedRow.cpu_cores,
              t("ops.components.cores"),
            )
          }}
        </ElDescriptionsItem>
        <ElDescriptionsItem :label="t('ops.services.columns.memory')">
          {{
            selectedRow.memory_bytes !== null &&
            selectedRow.memory_bytes !== undefined
              ? formatBytes(selectedRow.memory_bytes)
              : "—"
          }}
        </ElDescriptionsItem>
        <ElDescriptionsItem :label="t('ops.services.columns.fds')">
          {{ fixed(selectedRow.open_fds, 0) }}
        </ElDescriptionsItem>

        <ElDescriptionsItem :label="t('ops.services.columns.tokio')">
          {{ percent(selectedRow.tokio_busy) }}
        </ElDescriptionsItem>
        <ElDescriptionsItem :label="t('ops.services.columns.panic')">
          {{ fixed(selectedRow.panic_count, 0) }}
        </ElDescriptionsItem>
        <ElDescriptionsItem :label="t('ops.services.columns.restarts1h')">
          {{ fixed(selectedRow.restarts_1h, 0) }}
        </ElDescriptionsItem>

        <ElDescriptionsItem :label="t('ops.services.columns.restart')">
          {{
            selectedRow.last_restart_ts
              ? formatDateTime(selectedRow.last_restart_ts)
              : "—"
          }}
        </ElDescriptionsItem>
        <ElDescriptionsItem :label="t('ops.services.columns.version')">
          {{ dash(selectedRow.version) }}
        </ElDescriptionsItem>
        <ElDescriptionsItem :label="t('ops.services.columns.reasons')">
          {{ reasonsText(selectedRow) }}
        </ElDescriptionsItem>
      </ElDescriptions>
    </OpsCard>

    <OpsCard
      v-if="mode === 'apps' && filteredRows.length > 0"
      :title="t('ops.services.overviewTableTitle')"
    >
      <OpsTableWrap>
        <ArtTable
          :data="filteredRows"
          :columns="overviewColumns"
          row-key="service"
          class="surface-panel-table"
          stripe
          @row-click="onOverviewRowClick"
        >
          <template #status="{ row }">
            <ElTag :type="statusTagType(row)" size="small">
              {{ statusText(row) }}
            </ElTag>
          </template>
        </ArtTable>
      </OpsTableWrap>
    </OpsCard>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useRoute, useRouter } from "vue-router";
import { ElMessage } from "element-plus";
import OpsCard from "@/components/ops/OpsCard.vue";
import OpsTableWrap from "@/components/ops/OpsTableWrap.vue";
import ServiceHexagonGrid, {
  type HexagonNode,
  type HexagonStatus,
} from "@/components/mazbri/ServiceHexagonGrid.vue";
import { useOpsRole } from "@/composables/ops/useOpsRole";
import { useOpsSharedRange } from "@/composables/ops/useOpsSharedRange";
import { fetchServicesHealth } from "@/api/monitoring";
import type { ServiceHealthRow } from "@/types/api/monitoring";
import type { ColumnOption } from "@/types/component";
import { formatBytes, formatDateTime } from "@/utils/format";
import { buildRuntimeRoute } from "@/utils/ops/runtime-link";
import { buildServiceDetailRoute } from "@/utils/ops/services-link";
import { buildTraceSearchRoute } from "@/utils/ops/trace-link";
import { formatShortVersion } from "@/utils/ops/components-health";
import {
  filterRowsByGroup,
  formatCpuCoresDisplay,
  serviceHasPostgresPool,
  type HealthPanelMode,
} from "@/utils/ops/service-topology";

defineOptions({ name: "OpsServicesHealthPanel" });

const props = withDefaults(
  defineProps<{
    mode?: HealthPanelMode;
  }>(),
  {
    mode: "apps",
  },
);

const { t } = useI18n();
const route = useRoute();
const router = useRouter();
const { isAdmin } = useOpsRole();
const { timeRange, reloadTick } = useOpsSharedRange();
const loading = ref(false);
const rows = ref<ServiceHealthRow[]>([]);
const selectedService = ref<string | null>(
  typeof route.query.service === "string" && route.query.service
    ? route.query.service
    : null,
);

const cardTitle = computed(() =>
  props.mode === "apps"
    ? t("ops.services.appHexagonTitle")
    : t("ops.services.infraHexagonTitle"),
);

const filteredRows = computed(() => filterRowsByGroup(rows.value, props.mode));

function healthStatus(row: ServiceHealthRow): HexagonStatus {
  if (row.status) {
    switch (row.status) {
      case "green":
        return "healthy";
      case "yellow":
        return "degraded";
      case "red":
        return "down";
      default:
        return "unknown";
    }
  }
  if (!row.health_ok) return "down";
  if (row.ready === false) return "degraded";
  return "healthy";
}

function statusTagType(
  row: ServiceHealthRow,
): "success" | "warning" | "danger" | "info" {
  const s = healthStatus(row);
  if (s === "healthy") return "success";
  if (s === "degraded") return "warning";
  if (s === "down") return "danger";
  return "info";
}

function resolveBadge(row: ServiceHealthRow): string | undefined {
  if (row.reasons && row.reasons.length > 0) {
    return t(`ops.components.reason.${row.reasons[0]}`);
  }
  const shortVer = formatShortVersion(row.version);
  if (shortVer) return shortVer;
  const group = row.group ?? "app";
  return t(`ops.components.group.${group}`);
}

function buildTooltipLines(row: ServiceHealthRow): string[] {
  const statusKey = row.status ?? (row.health_ok ? "green" : "red");
  const lines: string[] = [
    `${row.name || row.service} (${row.service})`,
    `${t("ops.components.status")}：${t(`ops.components.statusName.${statusKey}`)}`,
  ];
  if (row.reasons && row.reasons.length > 0) {
    lines.push(
      row.reasons.map((r) => t(`ops.components.reason.${r}`)).join("、"),
    );
  }
  if (row.version) {
    lines.push(`${t("ops.components.version")}：v${row.version}`);
  }
  if (row.restarts_1h !== null && row.restarts_1h !== undefined) {
    lines.push(
      `${t("ops.components.restarts1h")}：${Math.round(row.restarts_1h)}`,
    );
  }
  if (row.cpu_cores !== null && row.cpu_cores !== undefined) {
    lines.push(
      `CPU：${formatCpuCoresDisplay(row.cpu_cores, t("ops.components.cores"))}`,
    );
  }
  if (row.memory_bytes !== null && row.memory_bytes !== undefined) {
    lines.push(`${t("ops.components.memory")}：${formatBytes(row.memory_bytes)}`);
  }
  return lines;
}

const abnormalCount = computed(
  () =>
    filteredRows.value.filter((r) =>
      r.status ? r.status !== "green" : !r.health_ok || r.ready === false,
    ).length,
);

const hexagonNodes = computed<HexagonNode[]>(() =>
  filteredRows.value.map((row) => ({
    id: row.service,
    label: row.name || row.service,
    status: healthStatus(row),
    badge: resolveBadge(row),
    tooltipLines: buildTooltipLines(row),
  })),
);

const selectedRow = computed(
  () =>
    filteredRows.value.find((r) => r.service === selectedService.value) ?? null,
);

const isSelectedApp = computed(
  () => (selectedRow.value?.group ?? "app") === "app",
);

function hasPostgresPool(row: ServiceHealthRow): boolean {
  if ((row.group ?? "app") !== "app") return false;
  return serviceHasPostgresPool(row.service);
}

function boolText(value: boolean | null | undefined) {
  if (value === null || value === undefined) return "—";
  return value ? t("ops.services.ok") : t("ops.services.down");
}

function containerText(row: ServiceHealthRow) {
  if (row.container_running === true) return t("ops.services.containerRunning");
  if (row.container_running === false) return t("ops.services.containerStopped");
  if (row.group === "middleware" && row.health_ok) {
    return t("ops.services.containerExternal");
  }
  return "—";
}

function groupText(row: ServiceHealthRow) {
  return t(`ops.components.group.${row.group ?? "app"}`);
}

function statusText(row: ServiceHealthRow) {
  const statusKey = row.status ?? (row.health_ok ? "green" : "red");
  return t(`ops.components.statusName.${statusKey}`);
}

function reasonsText(row: ServiceHealthRow) {
  if (!row.reasons || row.reasons.length === 0) return "—";
  return row.reasons.map((r) => t(`ops.components.reason.${r}`)).join("、");
}

function percent(value: number | null | undefined) {
  if (value === null || value === undefined) return "—";
  return `${(value * 100).toFixed(1)}%`;
}

function fixed(value: number | null | undefined, digits: number) {
  if (value === null || value === undefined) return "—";
  return value.toFixed(digits);
}

function dash(value: string | number | null | undefined) {
  if (value === null || value === undefined || value === "") return "—";
  return String(value);
}

const overviewColumns = computed<ColumnOption<ServiceHealthRow>[]>(() => [
  {
    prop: "name",
    label: t("ops.services.columns.service"),
    minWidth: 140,
    showOverflowTooltip: true,
    formatter: (row) => `${row.name || row.service} (${row.service})`,
  },
  {
    prop: "status",
    label: t("ops.services.columns.status"),
    width: 100,
    useSlot: true,
  },
  {
    prop: "version",
    label: t("ops.services.columns.version"),
    minWidth: 130,
    showOverflowTooltip: true,
    formatter: (row) => dash(row.version),
  },
  {
    prop: "cpu_cores",
    label: t("ops.services.columns.cpu"),
    minWidth: 105,
    formatter: (row) =>
      formatCpuCoresDisplay(row.cpu_cores, t("ops.components.cores")),
  },
  {
    prop: "memory_bytes",
    label: t("ops.services.columns.memory"),
    minWidth: 105,
    formatter: (row) =>
      row.memory_bytes !== null && row.memory_bytes !== undefined
        ? formatBytes(row.memory_bytes)
        : "—",
  },
  {
    prop: "restarts_1h",
    label: t("ops.services.columns.restarts1h"),
    width: 105,
    formatter: (row) => fixed(row.restarts_1h, 0),
  },
  {
    prop: "reasons",
    label: t("ops.services.columns.reasons"),
    minWidth: 150,
    showOverflowTooltip: true,
    formatter: (row) => reasonsText(row),
  },
]);

function onOverviewRowClick(row: ServiceHealthRow) {
  selectedService.value =
    selectedService.value === row.service ? null : row.service;
}

function openDetail(row: ServiceHealthRow) {
  if (!isAdmin.value) return;
  void router.push(buildServiceDetailRoute(row.service));
}

function openTraces(row: ServiceHealthRow) {
  if (!isAdmin.value) return;
  void router.push(
    buildTraceSearchRoute({
      start: timeRange.value.start,
      end: timeRange.value.end,
      filters: { service: row.service },
    }),
  );
}

async function load() {
  loading.value = true;
  try {
    rows.value = await fetchServicesHealth();
    if (
      selectedService.value &&
      !filteredRows.value.some((r) => r.service === selectedService.value)
    ) {
      selectedService.value = null;
    }
  } catch (error) {
    ElMessage.error(
      error instanceof Error ? error.message : t("ops.services.loadFailed"),
    );
  } finally {
    loading.value = false;
  }
}

watch(
  () => route.query.service,
  (service) => {
    if (typeof service === "string" && service) {
      selectedService.value = service;
    }
  },
);

onMounted(() => {
  void load();
});

watch(reloadTick, () => {
  void load();
});
</script>

<style scoped>
.ops-services-health-panel {
  display: flex;
  flex-direction: column;
  gap: var(--ops-space-sm, 12px);
}

.ops-services-health-panel__summary {
  font-size: 13px;
  font-weight: 600;
  color: var(--el-color-success);
}

.ops-services-health-panel__summary[data-abnormal]:not([data-abnormal="0"]) {
  color: var(--el-color-danger);
}

.ops-services-health-panel__extra-actions {
  display: inline-flex;
  align-items: center;
  gap: 8px;
}

.ops-services-health-panel__mono {
  font-family: var(--el-font-family-monospace, monospace);
  font-size: 12px;
}

.ops-services-health-panel__muted {
  color: var(--el-text-color-secondary);
}
</style>
