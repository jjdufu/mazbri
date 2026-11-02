<template>
  <OpsPage :title="t('menus.mazbri.productRank')">
    <template #actions>
      <ElSelect v-model="metric" style="width: 140px" @change="reload">
          <ElOption value="req_count" :label="t('ops.products.metricReq')" />
          <ElOption value="error_rate" :label="t('ops.products.metricErr')" />
          <ElOption value="p95" label="P95" />
          <ElOption value="cost" :label="t('ops.products.metricCost')" />
        </ElSelect>
        <TimeRangePicker v-model="timeRange" @change="reload" />
        <AutoRefresh v-model="refreshInterval" @tick="reload" />
    </template>
    <OpsCard :title="t('ops.products.rankTitle')">
      <ArtTable :data="rows" :loading="loading" :columns="columns" stripe />
    </OpsCard>
    <OpsCard :title="t('ops.products.zeroTitle')">
      <ArtTable :data="zeroRows" :loading="loading" :columns="zeroColumns" stripe />
    </OpsCard>
  </OpsPage>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { ElMessage } from "element-plus";
import TimeRangePicker, { type TimeRangeValue } from "@/components/ops/TimeRangePicker.vue";
import AutoRefresh from "@/components/ops/AutoRefresh.vue";
import OpsPage from "@/components/ops/OpsPage.vue";
import OpsCard from "@/components/ops/OpsCard.vue";
import { toOlapRange } from "@/composables/ops/useOlapRange";
import { postOlapQuery } from "@/api/olap";
import { zeroCallProducts } from "@/api/opsReliability";
import type { ColumnOption } from "@/types/component";

defineOptions({ name: "OpsTrafficProducts" });

interface RankRow {
  product_id: string;
  req_count: number;
  error_rate: number;
  p95: number;
  cost: number;
  prev_req_count: number;
}

const { t } = useI18n();
const metric = ref("req_count");
const timeRange = ref<TimeRangeValue>({
  preset: "7d",
  start: Math.floor(Date.now() / 1000) - 7 * 86400,
  end: Math.floor(Date.now() / 1000),
});
const refreshInterval = ref(60);
const loading = ref(false);
const rows = ref<RankRow[]>([]);
const zeroRows = ref<{ product_id: string; name?: string }[]>([]);

const columns = computed<ColumnOption[]>(() => [
  { prop: "product_id", label: t("ops.products.colProduct") },
  { prop: "req_count", label: t("ops.products.colReq"), width: 110 },
  { prop: "prev_req_count", label: t("ops.products.colPrev"), width: 110 },
  { prop: "error_rate", label: t("ops.products.colErr"), width: 100 },
  { prop: "p95", label: "P95", width: 90 },
  { prop: "cost", label: t("ops.products.colCost"), width: 100 },
]);
const zeroColumns = computed<ColumnOption[]>(() => [
  { prop: "product_id", label: t("ops.products.colProduct") },
  { prop: "name", label: t("ops.products.colName") },
]);

async function reload() {
  loading.value = true;
  try {
    const range = toOlapRange(timeRange.value);
    const rank = await postOlapQuery<RankRow>({
      template: "product_rank",
      params: { range, metric: metric.value, limit: 30 },
    });
    rows.value = ((rank as unknown as { rows?: RankRow[] }).rows) || [];
    const active = rows.value.map((r) => r.product_id);
    const zero = await zeroCallProducts(active);
    zeroRows.value = (zero as { product_id: string; name?: string }[]) || [];
  } catch (e) {
    ElMessage.error(String(e));
  } finally {
    loading.value = false;
  }
}

onMounted(reload);
</script>

<style scoped>
</style>
