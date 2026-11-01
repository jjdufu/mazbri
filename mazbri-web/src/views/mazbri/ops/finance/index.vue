<template>
  <div class="ops-traffic art-full-height">
    <div class="ops-traffic__toolbar">
      <h2>{{ t("menus.mazbri.financeOverview") }}</h2>
      <div class="ops-traffic__actions">
        <ElSelect v-model="days" style="width: 120px" @change="reload">
          <ElOption :value="7" label="7d" />
          <ElOption :value="30" label="30d" />
          <ElOption :value="90" label="90d" />
        </ElSelect>
        <AutoRefresh v-model="refreshInterval" @tick="reload" />
      </div>
    </div>

    <ElRow :gutter="12">
      <ElCol :xs="12" :lg="6">
        <StatCard
          :label="t('ops.finance.revenue')"
          :value="num(overview?.revenue_total)"
          :loading="loading"
          :digits="2"
        />
      </ElCol>
      <ElCol :xs="12" :lg="6">
        <StatCard
          :label="t('ops.finance.cost')"
          :value="num(overview?.cost_total)"
          :loading="loading"
          :digits="2"
        />
      </ElCol>
      <ElCol :xs="12" :lg="6">
        <StatCard
          :label="t('ops.finance.margin')"
          :value="num(overview?.margin_total)"
          :loading="loading"
          :digits="2"
        />
      </ElCol>
      <ElCol :xs="12" :lg="6">
        <StatCard
          :label="t('ops.finance.marginRate')"
          :value="overview?.margin_rate ?? null"
          unit="%"
          :loading="loading"
          :digits="1"
        />
      </ElCol>
    </ElRow>

    <div v-if="(overview?.inverted_days?.length ?? 0) > 0" class="ops-traffic__card">
      <ElAlert
        type="warning"
        :title="t('ops.finance.invertedWarn')"
        :description="overview?.inverted_days?.join(', ')"
        show-icon
        :closable="false"
      />
    </div>

    <div class="ops-traffic__card">
      <h3>{{ t("ops.finance.trend") }}</h3>
      <CategoryBarChart
        :categories="dayLabels"
        :series="trendSeries"
        :loading="loading"
        height="280px"
      />
    </div>

    <ElRow :gutter="12">
      <ElCol :xs="24" :lg="8">
        <div class="ops-traffic__card">
          <h3>{{ t("ops.finance.rankTenant") }}</h3>
          <ArtTable :data="ranks?.by_tenant ?? []" :loading="loading" :columns="rankColumns" stripe />
        </div>
      </ElCol>
      <ElCol :xs="24" :lg="8">
        <div class="ops-traffic__card">
          <h3>{{ t("ops.finance.rankProduct") }}</h3>
          <ArtTable :data="ranks?.by_product ?? []" :loading="loading" :columns="rankColumns" stripe />
        </div>
      </ElCol>
      <ElCol :xs="24" :lg="8">
        <div class="ops-traffic__card">
          <h3>{{ t("ops.finance.rankSupplier") }}</h3>
          <ArtTable :data="ranks?.by_supplier ?? []" :loading="loading" :columns="rankColumns" stripe />
        </div>
      </ElCol>
    </ElRow>

    <div class="ops-traffic__card">
      <h3>{{ t("ops.finance.balances") }}</h3>
      <ArtTable :data="balances" :loading="loading" :columns="balanceColumns" stripe />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { ElMessage } from "element-plus";
import AutoRefresh from "@/components/ops/AutoRefresh.vue";
import CategoryBarChart from "@/components/ops/CategoryBarChart.vue";
import StatCard from "@/components/ops/StatCard.vue";
import {
  fetchFinanceBalances,
  fetchFinanceOverview,
  fetchFinanceRanks,
  type BalanceRow,
  type FinanceOverview,
  type FinanceRanks,
} from "@/api/billing-ops";
import type { ColumnOption } from "@/types/component";

defineOptions({ name: "FinanceOverview" });

const { t } = useI18n();
const days = ref(30);
const refreshInterval = ref(120);
const loading = ref(false);
const overview = ref<FinanceOverview | null>(null);
const ranks = ref<FinanceRanks | null>(null);
const balances = ref<BalanceRow[]>([]);

function num(v?: string | null) {
  if (v == null || v === "") return null;
  const n = Number(v);
  return Number.isFinite(n) ? n : null;
}

const dayLabels = computed(() => overview.value?.days.map((d) => d.day) ?? []);
const trendSeries = computed(() => [
  {
    name: t("ops.finance.revenue"),
    data: overview.value?.days.map((d) => Number(d.revenue) || 0) ?? [],
  },
  {
    name: t("ops.finance.cost"),
    data: overview.value?.days.map((d) => Number(d.cost) || 0) ?? [],
  },
  {
    name: t("ops.finance.margin"),
    data: overview.value?.days.map((d) => Number(d.margin) || 0) ?? [],
  },
]);

const rankColumns = computed<ColumnOption[]>(() => [
  { prop: "key", label: t("ops.finance.key") },
  { prop: "revenue", label: t("ops.finance.revenue") },
  { prop: "cost", label: t("ops.finance.cost") },
  { prop: "margin", label: t("ops.finance.margin") },
]);

const balanceColumns = computed<ColumnOption<BalanceRow>[]>(() => [
  { prop: "tenant_id", label: t("ops.finance.tenantId") },
  { prop: "available_balance", label: t("ops.finance.available") },
  { prop: "recent_recharge", label: t("ops.finance.recharge") },
  { prop: "avg_daily_spend", label: t("ops.finance.avgSpend") },
  { prop: "days_remaining", label: t("ops.finance.daysLeft") },
  { prop: "status", label: t("ops.finance.status") },
]);

async function reload() {
  loading.value = true;
  try {
    const [ov, rk, bal] = await Promise.all([
      fetchFinanceOverview(days.value),
      fetchFinanceRanks(days.value, 10),
      fetchFinanceBalances(50),
    ]);
    overview.value = ov;
    ranks.value = rk;
    balances.value = bal ?? [];
  } catch {
    ElMessage.error(t("ops.finance.loadFailed"));
  } finally {
    loading.value = false;
  }
}

onMounted(reload);
</script>

<style scoped>
.ops-traffic__toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 12px;
}
.ops-traffic__actions {
  display: flex;
  gap: 8px;
  align-items: center;
}
.ops-traffic__card {
  margin-top: 12px;
  padding: 12px;
  border-radius: 8px;
  background: var(--el-bg-color);
}
</style>
