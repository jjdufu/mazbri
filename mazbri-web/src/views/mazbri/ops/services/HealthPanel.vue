<template>
  <div v-loading="loading" class="ops-services-health-panel">
    <OpsCard :title="t('ops.services.hexagonTitle')">
      <template v-if="rows.length > 0" #extra>
        <span class="ops-services-health-panel__summary" :data-abnormal="abnormalCount">
          {{
            abnormalCount > 0
              ? t("ops.components.abnormalOf", { n: abnormalCount, total: rows.length })
              : t("ops.components.allHealthy", { total: rows.length })
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
      :title="t('ops.services.detailTitle', { name: selectedRow.name || selectedRow.service })"
      class="ops-services-health-panel__detail"
    >
      <template #extra>
        <ElButton
          v-if="isSelectedApp && isAdmin"
          link
          type="primary"
          @click="openDetail(selectedRow)"
        >
          {{ t("ops.services.openDetail") }}
        </ElButton>
        <ElButton
          v-else-if="selectedRow.group === 'middleware'"
          link
          type="primary"
          @click="router.push(buildRuntimeRoute({ tab: 'middleware' }))"
        >
          {{ t("ops.services.openMiddleware") }}
        </ElButton>
        <ElButton
          v-else-if="selectedRow.group === 'observability'"
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
      </template>
      <OpsTableWrap>
        <ArtTable
          :data="[selectedRow]"
          :columns="columns"
          class="surface-panel-table"
          stripe
        />
      </OpsTableWrap>
    </OpsCard>
  </div>
</template>

<script setup lang="ts">
import { formatBytes, formatDateTime } from "@/utils/format";
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
import { buildRuntimeRoute } from "@/utils/ops/runtime-link";
import { buildServiceDetailRoute } from "@/utils/ops/services-link";
import { formatShortVersion } from "@/utils/ops/components-health";

defineOptions({ name: "OpsServicesHealthPanel" });

const { t } = useI18n();
const route = useRoute();
const router = useRouter();
const { isAdmin } = useOpsRole();
const { reloadTick } = useOpsSharedRange();
const loading = ref(false);
const rows = ref<ServiceHealthRow[]>([]);
const selectedService = ref<string | null>(
  typeof route.query.service === "string" && route.query.service
    ? route.query.service
    : null,
);

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
      `CPU：${row.cpu_cores.toFixed(3)} ${t("ops.components.cores")}`,
    );
  }
  if (row.memory_bytes !== null && row.memory_bytes !== undefined) {
    lines.push(`${t("ops.components.memory")}：${formatBytes(row.memory_bytes)}`);
  }
  return lines;
}

const abnormalCount = computed(
  () =>
    rows.value.filter((r) =>
      r.status ? r.status !== "green" : !r.health_ok || r.ready === false,
    ).length,
);

const hexagonNodes = computed<HexagonNode[]>(() =>
  rows.value.map((row) => ({
    id: row.service,
    label: row.name || row.service,
    status: healthStatus(row),
    badge: resolveBadge(row),
    tooltipLines: buildTooltipLines(row),
  })),
);

const selectedRow = computed(
  () => rows.value.find((r) => r.service === selectedService.value) ?? null,
);

const isSelectedApp = computed(
  () => (selectedRow.value?.group ?? "app") === "app",
);

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

function mebibytes(value: number | null | undefined) {
  if (value === null || value === undefined) return "—";
  return `${(value / 1048576).toFixed(0)} MB`;
}

function fixed(value: number | null | undefined, digits: number) {
  if (value === null || value === undefined) return "—";
  return value.toFixed(digits);
}

function dash(value: string | number | null | undefined) {
  if (value === null || value === undefined || value === "") return "—";
  return String(value);
}

const columns = computed<ColumnOption<ServiceHealthRow>[]>(() => {
  const baseStart: ColumnOption<ServiceHealthRow>[] = [
    {
      prop: "name",
      label: t("ops.services.columns.service"),
      minWidth: 140,
      showOverflowTooltip: true,
    },
    {
      prop: "service",
      label: t("ops.services.columns.key"),
      minWidth: 130,
      showOverflowTooltip: true,
    },
    {
      prop: "group",
      label: t("ops.services.columns.group"),
      width: 110,
      formatter: (row) => groupText(row),
    },
    {
      prop: "status",
      label: t("ops.services.columns.status"),
      width: 100,
      formatter: (row) => statusText(row),
    },
  ];

  if (!isSelectedApp.value) {
    return [
      ...baseStart,
      {
        prop: "reasons",
        label: t("ops.services.columns.reasons"),
        minWidth: 130,
        showOverflowTooltip: true,
        formatter: (row) => reasonsText(row),
      },
      {
        prop: "health_ok",
        label: t("ops.services.columns.health"),
        width: 100,
        formatter: (row) => boolText(row.health_ok),
      },
      {
        prop: "ready",
        label: t("ops.services.columns.ready"),
        width: 100,
        formatter: (row) => boolText(row.ready),
      },
      {
        prop: "container_running",
        label: t("ops.services.columns.container"),
        width: 110,
        formatter: (row) => containerText(row),
      },
      {
        prop: "restarts_1h",
        label: t("ops.services.columns.restarts1h"),
        width: 110,
        formatter: (row) => fixed(row.restarts_1h, 0),
      },
      {
        prop: "cpu_cores",
        label: t("ops.services.columns.cpu"),
        width: 110,
        formatter: (row) => fixed(row.cpu_cores, 3),
      },
      {
        prop: "memory_bytes",
        label: t("ops.services.columns.memory"),
        width: 110,
        formatter: (row) => mebibytes(row.memory_bytes),
      },
      {
        prop: "last_restart_ts",
        label: t("ops.services.columns.restart"),
        minWidth: 160,
        formatter: (row) =>
          row.last_restart_ts ? formatDateTime(row.last_restart_ts) : "—",
      },
      {
        prop: "version",
        label: t("ops.services.columns.version"),
        minWidth: 120,
        formatter: (row) => dash(row.version),
      },
    ];
  }

  return [
    ...baseStart,
    {
      prop: "health_ok",
      label: t("ops.services.columns.health"),
      width: 100,
      formatter: (row) => boolText(row.health_ok),
    },
    {
      prop: "ready",
      label: t("ops.services.columns.ready"),
      width: 100,
      formatter: (row) => boolText(row.ready),
    },
    {
      prop: "config_center",
      label: t("ops.services.columns.configCenter"),
      width: 110,
      formatter: (row) => boolText(row.config_center),
    },
    {
      prop: "container_running",
      label: t("ops.services.columns.container"),
      width: 110,
      formatter: (row) => containerText(row),
    },
    {
      prop: "restarts_1h",
      label: t("ops.services.columns.restarts1h"),
      width: 110,
      formatter: (row) => fixed(row.restarts_1h, 0),
    },
    {
      prop: "db_pool_usage",
      label: t("ops.services.columns.dbPool"),
      width: 110,
      formatter: (row) =>
        row.db_pool_usage === null || row.db_pool_usage === undefined
          ? "—"
          : `${(row.db_pool_usage * 100).toFixed(1)}%`,
    },
    {
      prop: "panic_count",
      label: t("ops.services.columns.panic"),
      width: 90,
      formatter: (row) => dash(row.panic_count),
    },
    {
      prop: "tokio_busy",
      label: t("ops.services.columns.tokio"),
      width: 110,
      formatter: (row) => percent(row.tokio_busy),
    },
    {
      prop: "cpu_cores",
      label: t("ops.services.columns.cpu"),
      width: 110,
      formatter: (row) => fixed(row.cpu_cores, 3),
    },
    {
      prop: "memory_bytes",
      label: t("ops.services.columns.memory"),
      width: 110,
      formatter: (row) => mebibytes(row.memory_bytes),
    },
    {
      prop: "open_fds",
      label: t("ops.services.columns.fds"),
      minWidth: 110,
      formatter: (row) => fixed(row.open_fds, 0),
    },
    {
      prop: "last_restart_ts",
      label: t("ops.services.columns.restart"),
      minWidth: 160,
      formatter: (row) =>
        row.last_restart_ts ? formatDateTime(row.last_restart_ts) : "—",
    },
    {
      prop: "version",
      label: t("ops.services.columns.version"),
      minWidth: 120,
      formatter: (row) => dash(row.version),
    },
  ];
});

function openDetail(row: ServiceHealthRow) {
  if (!isAdmin.value) return;
  void router.push(buildServiceDetailRoute(row.service));
}

async function load() {
  loading.value = true;
  try {
    rows.value = await fetchServicesHealth();
    if (
      selectedService.value &&
      !rows.value.some((r) => r.service === selectedService.value)
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
</style>
