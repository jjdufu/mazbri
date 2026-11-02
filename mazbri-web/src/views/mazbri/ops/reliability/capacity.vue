<template>
  <OpsPage :title="t('menus.mazbri.capacity')">
    <template #actions>
      <AutoRefresh v-model="refreshInterval" @tick="reload" />
    </template>
    <ElRow :gutter="16">
      <ElCol :xs="24" :md="8">
        <OpsCard :title="t('ops.capacity.forecast')">
          <p>{{ t("ops.capacity.holt") }}: {{ snap?.holt_next?.toFixed?.(2) ?? "-" }}</p>
          <p>{{ t("ops.capacity.cap") }}: {{ snap?.qps_cap ?? "-" }}</p>
          <p>{{ t("ops.capacity.days") }}: {{ snap?.days_to_cap != null ? snap.days_to_cap.toFixed(1) : t("ops.capacity.never") }}</p>
          <p class="muted">{{ snap?.detected_at || "" }}</p>
        </OpsCard>
      </ElCol>
      <ElCol :xs="24" :md="16">
        <OpsCard :title="t('ops.capacity.loadtests')">
          <ArtTable :data="runs" :loading="loading" :columns="columns" stripe />
        </OpsCard>
      </ElCol>
    </ElRow>
  </OpsPage>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { ElMessage } from "element-plus";
import AutoRefresh from "@/components/ops/AutoRefresh.vue";
import OpsPage from "@/components/ops/OpsPage.vue";
import OpsCard from "@/components/ops/OpsCard.vue";
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
.muted { color: var(--el-text-color-secondary); font-size: 12px; }
</style>
