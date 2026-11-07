<template>
  <OpsCard class="product-topn" :info="t('ops.products.basisNote')">
    <template #title>
      <span class="product-topn__title">
        {{ t("ops.overview.productTop") }}
        <ElTag
          v-if="zeroRows.length > 0"
          size="small"
          type="info"
          effect="plain"
          class="product-topn__badge"
        >
          {{ t("ops.overview.productZeroBadge", { n: zeroRows.length }) }}
        </ElTag>
      </span>
    </template>
    <template #extra>
      <ElSelect
        v-model="metric"
        style="width: 140px"
        size="small"
        @change="onMetricChange"
      >
        <ElOption value="req_count" :label="t('ops.products.metricReq')" />
        <ElOption value="error_rate" :label="t('ops.products.metricErr')" />
        <ElOption value="p95" :label="t('ops.products.metricP95')" />
        <ElOption value="cost" :label="t('ops.products.metricCost')" />
      </ElSelect>
    </template>

    <EmptyState
      v-if="loaded && !loading && rows.length === 0"
      compact
      :image-size="56"
      :description="t('ops.products.rankEmpty')"
    />
    <OpsTableWrap v-else>
      <ArtTable :data="rows" :loading="loading" :columns="columns" stripe />
    </OpsTableWrap>

    <div v-if="zeroRows.length > 0" class="product-topn__zero">
      <ElButton link type="primary" @click="zeroOpen = !zeroOpen">
        {{
          zeroOpen
            ? t("ops.overview.productZeroCollapse")
            : t("ops.overview.productZeroExpand")
        }}
      </ElButton>
      <OpsTableWrap v-if="zeroOpen" class="product-topn__zero-table">
        <ArtTable :data="zeroRows" :columns="zeroColumns" stripe />
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
import EmptyState from "@/components/common/empty-state/index.vue";
import { postOlapQuery } from "@/api/olap";
import { listProductNames } from "@/api/opsReliability";
import type { ColumnOption } from "@/types/component";
import { formatMoney } from "@/utils/format";
import {
  normalizeProductRankRows,
  productNameMap,
  productRankDelta,
  withProductNames,
  zeroCallProducts,
  type NamedProductRankRow,
  type ProductRankMetric,
  type ProductRankRow,
  type ZeroCallProductRow,
} from "@/utils/ops/product-rank";

defineOptions({ name: "OpsOverviewProductTopN" });

const TOP = 10;
/** 零调用差集需要窗口内全部活跃产品（OLAP 单次上限）。 */
const ALL_ACTIVE = 1000;

const { t } = useI18n();
const metric = ref<ProductRankMetric>("req_count");
const loading = ref(false);
/** 首次成功/失败返回前不显示空态，避免切换时闪「暂无数据」。 */
const loaded = ref(false);
const rows = ref<NamedProductRankRow[]>([]);
const zeroRows = ref<ZeroCallProductRow[]>([]);
const zeroOpen = ref(false);
let lastWindow: { start: number; end: number } | null = null;
let seq = 0;

const nf = new Intl.NumberFormat("zh-CN");

function pct(v: number): string {
  return `${(v * 100).toFixed(2)}%`;
}

function deltaText(row: ProductRankRow): string {
  const d = productRankDelta(row);
  if (d === null) return t("ops.products.deltaNew");
  const sign = d > 0 ? "+" : "";
  return `${sign}${(d * 100).toFixed(1)}%`;
}

const columns = computed<ColumnOption<NamedProductRankRow>[]>(() => [
  { prop: "name", label: t("ops.products.colProduct"), minWidth: 160 },
  {
    prop: "req_count",
    label: t("ops.products.colReq"),
    width: 120,
    formatter: (r) => nf.format(r.req_count),
  },
  {
    prop: "prev_req_count",
    label: t("ops.products.colPrev"),
    width: 120,
    formatter: (r) => nf.format(r.prev_req_count),
  },
  {
    prop: "delta",
    label: t("ops.products.colDelta"),
    width: 90,
    formatter: (r) => deltaText(r),
  },
  {
    prop: "error_rate",
    label: t("ops.products.colErr"),
    width: 110,
    formatter: (r) => pct(r.error_rate),
  },
  {
    prop: "p95",
    label: t("ops.products.colP95"),
    width: 120,
    formatter: (r) => r.p95.toFixed(1),
  },
  {
    prop: "cost",
    label: t("ops.products.colCost"),
    width: 120,
    formatter: (r) => formatMoney(r.cost),
  },
]);

const zeroColumns = computed<ColumnOption<ZeroCallProductRow>[]>(() => [
  { prop: "name", label: t("ops.products.colName") },
  { prop: "product_id", label: t("ops.products.colId"), width: 160 },
]);

async function load(window: { start: number; end: number }) {
  const mine = ++seq;
  lastWindow = window;
  loading.value = true;
  try {
    const needAll = metric.value === "req_count";
    const [rankRes, allRes, catalog] = await Promise.all([
      postOlapQuery<ProductRankRow>({
        template: "product_rank",
        params: { ...window, metric: metric.value, limit: needAll ? ALL_ACTIVE : TOP },
      }),
      needAll
        ? Promise.resolve(null)
        : postOlapQuery<ProductRankRow>({
            template: "product_rank",
            params: { ...window, metric: "req_count", limit: ALL_ACTIVE },
          }),
      listProductNames(),
    ]);
    if (mine !== seq) return;
    const ranked = normalizeProductRankRows(rankRes.rows);
    const active = allRes ? normalizeProductRankRows(allRes.rows) : ranked;
    const names = productNameMap(catalog);
    rows.value = withProductNames(ranked.slice(0, TOP), names);
    zeroRows.value = zeroCallProducts(catalog, active);
    if (zeroRows.value.length === 0) zeroOpen.value = false;
  } catch (error) {
    if (mine !== seq) return;
    rows.value = [];
    zeroRows.value = [];
    zeroOpen.value = false;
    ElMessage.error(
      error instanceof Error ? error.message : t("ops.overview.productTopLoadFailed"),
    );
  } finally {
    if (mine === seq) {
      loading.value = false;
      loaded.value = true;
    }
  }
}

function onMetricChange() {
  if (lastWindow) void load(lastWindow);
}

/** 由总览按全局时间窗驱动；窗口由父组件在每次刷新时解析好传入。 */
function reload(window: { start: number; end: number }) {
  return load(window);
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
.product-topn__note {
  margin: 6px 0 0;
  color: var(--el-text-color-secondary);
  font-size: 12px;
}
.product-topn__zero {
  margin-top: 8px;
}
.product-topn__zero-table {
  margin-top: 8px;
}
</style>
