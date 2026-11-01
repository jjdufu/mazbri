<template>
  <div class="ops-traffic art-full-height">
    <div class="ops-traffic__toolbar">
      <h2>{{ t("menus.mazbri.capacity") }}</h2>
      <AutoRefresh v-model="refreshInterval" @tick="reload" />
    </div>
    <ElRow :gutter="12">
      <ElCol :xs="24" :md="8">
        <div class="ops-traffic__card">
          <h3>{{ t("ops.capacity.forecast") }}</h3>
          <p>{{ t("ops.capacity.holt") }}: {{ snap?.holt_next?.toFixed?.(2) ?? "-" }}</p>
          <p>{{ t("ops.capacity.cap") }}: {{ snap?.qps_cap ?? "-" }}</p>
          <p>{{ t("ops.capacity.days") }}: {{ snap?.days_to_cap != null ? snap.days_to_cap.toFixed(1) : t("ops.capacity.never") }}</p>
          <p class="muted">{{ snap?.detected_at || "" }}</p>
        </div>
      </ElCol>
      <ElCol :xs="24" :md="16">
        <div class="ops-traffic__card">
          <h3>{{ t("ops.capacity.loadtests") }}</h3>
          <ArtTable :data="runs" :loading="loading" :columns="columns" stripe />
        </div>
      </ElCol>
    </ElRow>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { ElMessage } from "element-plus";
import AutoRefresh from "@/components/ops/AutoRefresh.vue";
import { getCapacity, listLoadtests, type CapacitySnapshot, type LoadtestRun } from "@/api/opsReliability";
import type { ColumnOption } from "@/types/component";

defineOptions({ name: "OpsReliabilityCapacity" });

const { t } = useI18n();
const refreshInterval = ref(120);
const loading = ref(false);
const snap = ref<CapacitySnapshot | null>(null);
const runs = ref<LoadtestRun[]>([]);
const columns = computed<ColumnOption[]>(() => [
  { prop: "scenario", label: t("ops.capacity.colScenario") },
  { prop: "products", label: t("ops.capacity.colProducts"), width: 90 },
  { prop: "requests", label: t("ops.capacity.colReqs"), width: 100 },
  { prop: "p50", label: "P50", width: 80 },
  { prop: "p95", label: "P95", width: 80 },
  { prop: "p99", label: "P99", width: 80 },
  { prop: "internal_p99", label: t("ops.capacity.colInternal"), width: 110 },
  { prop: "notes", label: t("ops.capacity.colNotes") },
]);

async function reload() {
  loading.value = true;
  try {
    const [c, l] = await Promise.all([getCapacity(), listLoadtests(10)]);
    snap.value = c as CapacitySnapshot;
    runs.value = (l as LoadtestRun[]) || [];
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
.muted { color: var(--el-text-color-secondary); font-size: 12px; }
</style>
