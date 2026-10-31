<template>
  <div class="ops-services art-full-height">
    <div class="ops-services__toolbar">
      <h2>{{ t("menus.mazbri.serviceHealth") }}</h2>
      <div class="ops-services__actions">
        <AutoRefresh v-model="refreshInterval" @tick="load" />
        <ElButton @click="load">{{ t("ops.services.refresh") }}</ElButton>
      </div>
    </div>
    <ArtTable
      :data="rows"
      :loading="loading"
      :columns="columns"
      class="surface-panel-table"
      stripe
      @row-click="onRowClick"
    />
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";
import { ElMessage } from "element-plus";
import AutoRefresh from "@/components/ops/AutoRefresh.vue";
import { fetchServicesHealth } from "@/api/monitoring";
import type { ServiceHealthRow } from "@/types/api/monitoring";
import type { ColumnOption } from "@/types/component";

defineOptions({ name: "OpsServiceHealthMatrix" });

const { t } = useI18n();
const router = useRouter();
const loading = ref(false);
const rows = ref<ServiceHealthRow[]>([]);
const refreshInterval = ref(30);

function boolText(value: boolean | null | undefined) {
  if (value === null || value === undefined) return "—";
  return value ? t("ops.services.ok") : t("ops.services.down");
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
    formatter: (row) => dash(row.tokio_busy),
  },
  {
    prop: "last_restart_ts",
    label: t("ops.services.columns.restart"),
    minWidth: 160,
    formatter: (row) =>
      row.last_restart_ts
        ? new Date(row.last_restart_ts * 1000).toLocaleString()
        : "—",
  },
  {
    prop: "version",
    label: t("ops.services.columns.version"),
    minWidth: 120,
    formatter: (row) => dash(row.version),
  },
]);

function onRowClick(row: ServiceHealthRow) {
  void router.push(`/ops/services/${row.service}`);
}

async function load() {
  loading.value = true;
  try {
    rows.value = await fetchServicesHealth();
  } catch (error) {
    ElMessage.error(
      error instanceof Error ? error.message : t("ops.services.loadFailed"),
    );
  } finally {
    loading.value = false;
  }
}

onMounted(load);
</script>

<style scoped>
.ops-services {
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding: 16px;
}
.ops-services__toolbar {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  align-items: center;
  justify-content: space-between;
}
.ops-services__toolbar h2 {
  margin: 0;
  font-size: 20px;
  font-weight: 600;
}
.ops-services__actions {
  display: flex;
  gap: 8px;
  align-items: center;
}
</style>
