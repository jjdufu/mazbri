<template>
  <div v-loading="loading" class="ops-services-health-panel">
    <OpsCard :title="t('ops.services.hexagonTitle')">
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
      :title="t('ops.services.detailTitle', { name: selectedRow.name })"
      class="ops-services-health-panel__detail"
    >
      <template #extra>
        <ElButton
          v-if="isAdmin"
          link
          type="primary"
          @click="openDetail(selectedRow)"
        >
          {{ t("ops.services.openDetail") }}
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
import { formatDateTime } from "@/utils/format";
import { computed, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";
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
import { buildServiceDetailRoute } from "@/utils/ops/services-link";

defineOptions({ name: "OpsServicesHealthPanel" });

const { t } = useI18n();
const router = useRouter();
const { isAdmin } = useOpsRole();
const { reloadTick } = useOpsSharedRange();
const loading = ref(false);
const rows = ref<ServiceHealthRow[]>([]);
const selectedService = ref<string | null>(null);

function healthStatus(row: ServiceHealthRow): HexagonStatus {
  if (!row.health_ok) return "down";
  if (row.ready === false) return "degraded";
  return "healthy";
}

const hexagonNodes = computed<HexagonNode[]>(() =>
  rows.value.map((row) => ({
    id: row.service,
    label: row.name || row.service,
    status: healthStatus(row),
    badge: row.version ? `v${row.version}` : undefined,
  })),
);

const selectedRow = computed(
  () => rows.value.find((r) => r.service === selectedService.value) ?? null,
);

function boolText(value: boolean | null | undefined) {
  if (value === null || value === undefined) return "—";
  return value ? t("ops.services.ok") : t("ops.services.down");
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

const columns = computed<ColumnOption<ServiceHealthRow>[]>(() => [
  {
    prop: "name",
    label: t("ops.services.columns.service"),
    minWidth: 140,
    showOverflowTooltip: true,
  },
  {
    prop: "health_ok",
    label: t("ops.services.columns.health"),
    width: 110,
    formatter: (row) => boolText(row.health_ok),
  },
  {
    prop: "ready",
    label: t("ops.services.columns.ready"),
    width: 110,
    formatter: (row) => boolText(row.ready),
  },
  {
    prop: "config_center",
    label: t("ops.services.columns.configCenter"),
    width: 120,
    formatter: (row) => boolText(row.config_center),
  },
  {
    prop: "db_pool_usage",
    label: t("ops.services.columns.dbPool"),
    width: 120,
    formatter: (row) =>
      row.db_pool_usage === null || row.db_pool_usage === undefined
        ? "—"
        : `${(row.db_pool_usage * 100).toFixed(1)}%`,
  },
  {
    prop: "panic_count",
    label: t("ops.services.columns.panic"),
    width: 100,
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
    minWidth: 120,
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
]);

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
</style>
