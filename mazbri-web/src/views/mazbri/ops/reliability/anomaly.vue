<template>
  <div class="ops-traffic art-full-height">
    <div class="ops-traffic__toolbar">
      <h2>{{ t("menus.mazbri.anomaly") }}</h2>
      <AutoRefresh v-model="refreshInterval" @tick="reload" />
    </div>
    <div class="ops-traffic__card">
      <ArtTable :data="rows" :loading="loading" :columns="columns" stripe />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { ElMessage } from "element-plus";
import AutoRefresh from "@/components/ops/AutoRefresh.vue";
import { listAnomalies, type AnomalyEvent } from "@/api/opsReliability";
import type { ColumnOption } from "@/types/component";

defineOptions({ name: "OpsReliabilityAnomaly" });

const { t } = useI18n();
const refreshInterval = ref(60);
const loading = ref(false);
const rows = ref<AnomalyEvent[]>([]);
const columns = computed<ColumnOption[]>(() => [
  { prop: "detected_at", label: t("ops.anomaly.colTime"), width: 180 },
  { prop: "kind", label: t("ops.anomaly.colKind"), width: 120 },
  { prop: "severity", label: t("ops.anomaly.colSev"), width: 90 },
  { prop: "entity_id", label: t("ops.anomaly.colEntity"), width: 140 },
  { prop: "metric", label: t("ops.anomaly.colMetric"), width: 90 },
  { prop: "value", label: t("ops.anomaly.colValue"), width: 100 },
  { prop: "baseline", label: t("ops.anomaly.colBase"), width: 100 },
  { prop: "score", label: t("ops.anomaly.colScore"), width: 90 },
  { prop: "status", label: t("ops.anomaly.colStatus"), width: 90 },
]);

async function reload() {
  loading.value = true;
  try {
    rows.value = ((await listAnomalies(100)) as AnomalyEvent[]) || [];
  } catch (e) {
    ElMessage.error(String(e));
  } finally {
    loading.value = false;
  }
}
onMounted(reload);
</script>

<style scoped>
.ops-traffic__toolbar { display:flex; justify-content:space-between; align-items:center; margin-bottom:12px; }
.ops-traffic__card { background:var(--el-bg-color); border-radius:8px; padding:12px; margin-bottom:12px; }
</style>
