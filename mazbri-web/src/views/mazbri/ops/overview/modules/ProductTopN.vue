<template>
  <OpsCard class="product-topn">
    <template #title>
      <span class="product-topn__title">
        {{ t("ops.overview.productTop") }}
        <ElTag
          v-if="zeroCount > 0"
          size="small"
          type="info"
          effect="plain"
          class="product-topn__badge"
        >
          {{ t("ops.overview.productZeroBadge", { n: zeroCount }) }}
        </ElTag>
      </span>
    </template>
    <template #extra>
      <ElSelect
        v-model="metric"
        style="width: 140px"
        size="small"
        @change="reload"
      >
        <ElOption value="req_count" :label="t('ops.products.metricReq')" />
        <ElOption value="error_rate" :label="t('ops.products.metricErr')" />
        <ElOption value="p95" label="P95" />
        <ElOption value="cost" :label="t('ops.products.metricCost')" />
      </ElSelect>
    </template>

    <OpsTableWrap>
      <ArtTable
        :data="rows"
        :loading="loading"
        :columns="columns"
        stripe
      />
    </OpsTableWrap>

    <div v-if="zeroCount > 0" class="product-topn__zero">
      <ElButton link type="primary" @click="zeroOpen = !zeroOpen">
        {{
          zeroOpen
            ? t("ops.overview.productZeroCollapse")
            : t("ops.overview.productZeroExpand")
        }}
      </ElButton>
      <OpsTableWrap v-if="zeroOpen" class="product-topn__zero-table">
        <ArtTable
          :data="zeroRows"
          :loading="loading"
          :columns="zeroColumns"
          stripe
        />
      </OpsTableWrap>
    </div>
  </OpsCard>
</template>

<script setup lang="ts">
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import { ElMessage } from "element-plus";
import OpsCard from "@/components/ops/OpsCard.vue";
import OpsTableWrap from "@/components/ops/OpsTableWrap.vue";
import type { TimeRangeValue } from "@/components/ops/TimeRangePicker.vue";
import { toOlapWindow } from "@/composables/ops/useOlapRange";
import { postOlapQuery } from "@/api/olap";
import { zeroCallProducts } from "@/api/opsReliability";
import type { ColumnOption } from "@/types/component";
import {
  activeProductIdsFromRank,
  normalizeProductRankRows,
  normalizeZeroCallProducts,
  type ProductRankMetric,
  type ProductRankRow,
  type ZeroCallProductRow,
} from "@/utils/ops/product-rank";

defineOptions({ name: "OpsOverviewProductTopN" });

const props = defineProps<{
  timeRange: TimeRangeValue;
}>();

const { t } = useI18n();
const metric = ref<ProductRankMetric>("req_count");
const loading = ref(false);
const rows = ref<ProductRankRow[]>([]);
const zeroRows = ref<ZeroCallProductRow[]>([]);
const zeroOpen = ref(false);

const zeroCount = computed(() => zeroRows.value.length);

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
    const window = toOlapWindow(props.timeRange);
    const activeWindow = {
      start: window.end - 30 * 86400,
      end: window.end,
    };
    const [rankRes, activeRes] = await Promise.all([
      postOlapQuery<ProductRankRow>({
        template: "product_rank",
        params: { ...window, metric: metric.value, limit: 10 },
      }),
      // 零调用差集按近 30 天有调用的产品（与文案一致），与 TopN 排序窗解耦。
      postOlapQuery<ProductRankRow>({
        template: "product_rank",
        params: { ...activeWindow, metric: "req_count", limit: 1000 },
      }),
    ]);
    rows.value = normalizeProductRankRows(rankRes.rows);
    const active = activeProductIdsFromRank(
      normalizeProductRankRows(activeRes.rows),
    );
    const zero = await zeroCallProducts(active);
    zeroRows.value = normalizeZeroCallProducts(zero);
    if (zeroRows.value.length === 0) zeroOpen.value = false;
  } catch (error) {
    rows.value = [];
    zeroRows.value = [];
    zeroOpen.value = false;
    ElMessage.error(
      error instanceof Error
        ? error.message
        : t("ops.overview.productTopLoadFailed"),
    );
  } finally {
    loading.value = false;
  }
}

defineExpose({ reload });
</script>

<style scoped>
.product-topn__title {
  display: inline-flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
}
.product-topn__badge {
  font-weight: 500;
}
.product-topn__zero {
  margin-top: 8px;
}
.product-topn__zero-table {
  margin-top: 8px;
}
</style>
