<template>
  <OpsCard class="product-topn" :info="t('ops.products.basisNote')">
    <template #title>
      <span class="product-topn__title">
        {{ t("ops.overview.productTop") }}
      </span>
    </template>
    <template #extra>
      <div class="product-topn__toolbar">
        <ElSegmented
          v-model="viewMode"
          :options="segmentOptions"
          @change="onModeChange"
        />
        <ElSelect
          v-if="viewMode === 'active'"
          v-model="metric"
          class="product-topn__metric-select"
          @change="onMetricChange"
        >
          <ElOption value="req_count" :label="t('ops.products.metricReq')" />
          <ElOption value="error_rate" :label="t('ops.products.metricErr')" />
          <ElOption value="p95" :label="t('ops.products.metricP95')" />
          <ElOption value="cost" :label="t('ops.products.metricCost')" />
        </ElSelect>
        <ElInput
          v-else
          v-model="zeroQuery"
          clearable
          class="product-topn__zero-search"
          :placeholder="t('ops.products.zeroSearchPlaceholder')"
          @clear="onZeroSearch"
          @keyup.enter="onZeroSearch"
        >
          <template #prefix>
            <ArtSvgIcon icon="ri:search-line" />
          </template>
        </ElInput>
      </div>
    </template>

    <template v-if="viewMode === 'active'">
      <EmptyState
        v-if="loaded && !loading && rows.length === 0"
        compact
        :image-size="56"
        :description="t('ops.products.rankEmpty')"
      />
      <OpsTableWrap v-else>
        <ArtTable :data="rows" :loading="loading" :columns="columns" stripe />
      </OpsTableWrap>
    </template>

    <template v-else>
      <EmptyState
        v-if="zeroLoaded && !zeroLoading && zeroRows.length === 0"
        compact
        :image-size="56"
        :description="t('ops.products.zeroEmpty')"
      />
      <OpsTableWrap v-else>
        <ArtTable
          :data="zeroRows"
          :loading="zeroLoading"
          :columns="zeroColumns"
          :pagination="zeroPagination"
          stripe
          @pagination:current-change="onZeroPageChange"
          @pagination:size-change="onZeroSizeChange"
        />
      </OpsTableWrap>
    </template>
  </OpsCard>
</template>

<script setup lang="ts">
import { computed, reactive, ref } from "vue";
import { useI18n } from "vue-i18n";
import { ElMessage } from "element-plus";
import OpsCard from "@/components/ops/OpsCard.vue";
import OpsTableWrap from "@/components/ops/OpsTableWrap.vue";
import EmptyState from "@/components/common/empty-state/index.vue";
import { postOlapQuery } from "@/api/olap";
import { listProductNames, listZeroCallProducts } from "@/api/opsReliability";
import type { ColumnOption, PaginationConfig } from "@/types/component";
import { formatMoney } from "@/utils/format";
import {
  normalizeProductRankRows,
  productNameMap,
  productRankDelta,
  withProductNames,
  type NamedProductRankRow,
  type ProductRankMetric,
  type ProductRankRow,
  type ZeroCallProductRow,
} from "@/utils/ops/product-rank";

defineOptions({ name: "OpsOverviewProductTopN" });

type ViewMode = "active" | "zero";

const TOP = 10;

const { t } = useI18n();
const viewMode = ref<ViewMode>("active");
const metric = ref<ProductRankMetric>("req_count");
const loading = ref(false);
const loaded = ref(false);
const rows = ref<NamedProductRankRow[]>([]);

const zeroLoading = ref(false);
const zeroLoaded = ref(false);
const zeroRows = ref<ZeroCallProductRow[]>([]);
const zeroTotal = ref(0);
const zeroBadgeCount = ref(0);
const zeroQuery = ref("");
const zeroPager = reactive({
  page: 1,
  pageSize: 10,
});

let lastWindow: { start: number; end: number } | null = null;
let seq = 0;
let zeroSeq = 0;

const nf = new Intl.NumberFormat("zh-CN");

const segmentOptions = computed(() => [
  { label: t("ops.products.segmentActive"), value: "active" },
  {
    label: t("ops.products.segmentZero", { n: zeroBadgeCount.value }),
    value: "zero",
  },
]);

const zeroPagination = computed<PaginationConfig>(() => ({
  current: zeroPager.page,
  size: zeroPager.pageSize,
  total: zeroTotal.value,
}));

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
  {
    prop: "name",
    label: t("ops.products.colName"),
    minWidth: 200,
    formatter: (r) => r.name || "—",
  },
  {
    prop: "product_id",
    label: t("ops.products.colId"),
    minWidth: 160,
    formatter: (r) => r.product_id || "—",
  },
]);

async function loadZeroPage(
  window: { start: number; end: number },
  updateBadge = false,
) {
  const mine = ++zeroSeq;
  zeroLoading.value = true;
  try {
    const q = zeroQuery.value.trim();
    const res = await listZeroCallProducts({
      start: window.start,
      end: window.end,
      page: zeroPager.page,
      page_size: zeroPager.pageSize,
      ...(q ? { q } : {}),
    });
    if (mine !== zeroSeq) return;
    zeroRows.value = (res.items ?? []).map((item) => ({
      product_id: item.product_id,
      name: item.name?.trim() || item.product_id,
    }));
    zeroTotal.value = res.total ?? 0;
    if (updateBadge && !q) {
      zeroBadgeCount.value = res.total ?? 0;
    }
  } catch (error) {
    if (mine !== zeroSeq) return;
    zeroRows.value = [];
    zeroTotal.value = 0;
    ElMessage.error(
      error instanceof Error ? error.message : t("ops.overview.productTopLoadFailed"),
    );
  } finally {
    if (mine === zeroSeq) {
      zeroLoading.value = false;
      zeroLoaded.value = true;
    }
  }
}

async function loadActive(window: { start: number; end: number }) {
  const mine = ++seq;
  loading.value = true;
  try {
    const [rankRes, catalog] = await Promise.all([
      postOlapQuery<ProductRankRow>({
        template: "product_rank",
        params: { ...window, metric: metric.value, limit: TOP },
      }),
      listProductNames(),
    ]);
    if (mine !== seq) return;
    const ranked = normalizeProductRankRows(rankRes.rows);
    const names = productNameMap(catalog);
    rows.value = withProductNames(ranked.slice(0, TOP), names);
  } catch (error) {
    if (mine !== seq) return;
    rows.value = [];
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

async function load(window: { start: number; end: number }) {
  lastWindow = window;
  zeroPager.page = 1;
  await Promise.all([loadActive(window), loadZeroPage(window, true)]);
}

function onModeChange() {
  if (viewMode.value === "zero" && lastWindow) {
    void loadZeroPage(lastWindow, false);
  }
}

function onMetricChange() {
  if (lastWindow) void loadActive(lastWindow);
}

function onZeroSearch() {
  zeroPager.page = 1;
  if (lastWindow) void loadZeroPage(lastWindow, false);
}

function onZeroPageChange(page: number) {
  zeroPager.page = page;
  if (lastWindow) void loadZeroPage(lastWindow, false);
}

function onZeroSizeChange(size: number) {
  zeroPager.pageSize = size;
  zeroPager.page = 1;
  if (lastWindow) void loadZeroPage(lastWindow, false);
}

/** 由总览按全局时间窗驱动；窗口由父组件在每次刷新时解析好传入。 */
function reload(window: { start: number; end: number }) {
  return load(window);
}

defineExpose({ reload });
</script>

<style scoped lang="scss">
.product-topn__title {
  display: inline-flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
}

.product-topn__toolbar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
}

.product-topn__metric-select {
  width: 140px;
  flex-shrink: 0;
}

.product-topn__zero-search {
  width: 180px;
  flex-shrink: 0;
  transition: width 0.2s ease;

  &:focus-within {
    width: 260px;
  }
}
</style>
