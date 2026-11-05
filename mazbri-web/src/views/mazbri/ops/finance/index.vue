<template>
  <OpsPage
    :title="t('menus.mazbri.financeOverview')"
    :subtitle="windowHint"
  >
    <template #actions>
      <ElSelect v-model="days" class="finance-days-select" @change="reload">
        <ElOption :value="7" label="7d" />
        <ElOption :value="30" label="30d" />
        <ElOption :value="90" label="90d" />
      </ElSelect>
      <AutoRefresh v-model="refreshInterval" @tick="reload" />
    </template>

    <ElAlert
      v-if="costUnavailable"
      class="finance-cost-alert"
      type="warning"
      :title="t('ops.finance.costUnavailableTitle')"
      :description="t('ops.finance.costUnavailableDesc')"
      show-icon
      :closable="false"
    />

    <OpsKpiRow>
      <ElCol :xs="12" :sm="12" :md="8" :lg="6">
        <StatCard
          :label="t('ops.finance.revenue')"
          :value="num(overview?.revenue_total)"
          :hint="windowHint"
          :loading="loading"
          :digits="2"
        />
      </ElCol>
      <ElCol :xs="12" :sm="12" :md="8" :lg="6">
        <StatCard
          :label="t('ops.finance.cost')"
          :value="costUnavailable ? null : num(overview?.cost_total)"
          :hint="windowHint"
          :loading="loading"
          :digits="2"
        />
      </ElCol>
      <ElCol :xs="12" :sm="12" :md="8" :lg="6">
        <StatCard
          :label="t('ops.finance.margin')"
          :value="num(overview?.margin_total)"
          :hint="windowHint"
          :loading="loading"
          :digits="2"
        />
      </ElCol>
      <ElCol :xs="12" :sm="12" :md="8" :lg="6">
        <StatCard
          :label="t('ops.finance.marginRate')"
          :value="overview?.margin_rate ?? null"
          unit="%"
          :hint="windowHint"
          :loading="loading"
          :digits="1"
        />
      </ElCol>
    </OpsKpiRow>

    <OpsCard v-if="(overview?.inverted_days?.length ?? 0) > 0">
      <ElAlert
        type="warning"
        :title="t('ops.finance.invertedWarn')"
        :description="overview?.inverted_days?.join(', ')"
        show-icon
        :closable="false"
      />
    </OpsCard>

    <OpsCard :title="t('ops.finance.trend')">
      <CategoryBarChart
        :categories="dayLabels"
        :series="trendSeries"
        :loading="loading"
        height="280px"
      />
    </OpsCard>

    <ElRow :gutter="16" class="ops-equal-row">
      <ElCol :xs="24" :lg="8">
        <OpsCard :title="t('ops.finance.rankTenant')">
          <ArtTable
            :data="ranks?.by_tenant ?? []"
            :loading="loading"
            :columns="tenantRankColumns"
            stripe
          />
        </OpsCard>
      </ElCol>
      <ElCol :xs="24" :lg="8">
        <OpsCard :title="t('ops.finance.rankProduct')">
          <ArtTable
            :data="ranks?.by_product ?? []"
            :loading="loading"
            :columns="namedRankColumns(t('ops.finance.productName'))"
            stripe
          />
        </OpsCard>
      </ElCol>
      <ElCol :xs="24" :lg="8">
        <OpsCard :title="t('ops.finance.rankSupplier')">
          <ArtTable
            :data="ranks?.by_supplier ?? []"
            :loading="loading"
            :columns="namedRankColumns(t('ops.finance.supplierName'))"
            stripe
          />
        </OpsCard>
      </ElCol>
    </ElRow>

    <OpsCard :title="t('ops.finance.balances')">
      <ArtTable
        :data="balances"
        :loading="loading"
        :columns="balanceColumns"
        stripe
      />
    </OpsCard>
  </OpsPage>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { ElMessage } from "element-plus";
import AutoRefresh from "@/components/ops/AutoRefresh.vue";
import CategoryBarChart from "@/components/ops/CategoryBarChart.vue";
import StatCard from "@/components/ops/StatCard.vue";
import OpsPage from "@/components/ops/OpsPage.vue";
import OpsKpiRow from "@/components/ops/OpsKpiRow.vue";
import OpsCard from "@/components/ops/OpsCard.vue";
import {
  fetchFinanceBalances,
  fetchFinanceOverview,
  fetchFinanceRanks,
  type BalanceRow,
  type FinanceOverview,
  type FinanceRankRow,
  type FinanceRanks,
} from "@/api/billing-ops";
import type { ColumnOption } from "@/types/component";
import { getStatusLabel } from "@/utils/display-labels";
import { formatDays, formatMoney } from "@/utils/format";

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

function moneyCell(value: string | number | null | undefined): string {
  return formatMoney(value);
}

function daysCell(value: number | null | undefined): string {
  const formatted = formatDays(value);
  if (formatted === "—") return formatted;
  return `${formatted} ${t("ops.finance.daysUnit")}`;
}

function rankName(row: FinanceRankRow): string {
  const name = row.display_name?.trim();
  return name || t("ops.finance.unnamed");
}

function moneyOrDash(
  value: string | null | undefined,
  costStatus: string,
): string {
  if (costStatus !== "ok") return "—";
  return moneyCell(value);
}

const costUnavailable = computed(
  () => overview.value != null && overview.value.cost_status !== "ok",
);

const windowHint = computed(
  () =>
    overview.value?.window?.label ??
    t("ops.finance.windowFallback", { days: days.value }),
);

const dayLabels = computed(() => overview.value?.days.map((d) => d.day) ?? []);
const trendSeries = computed(() => {
  const series = [
    {
      name: t("ops.finance.revenue"),
      data: overview.value?.days.map((d) => Number(d.revenue) || 0) ?? [],
    },
  ];
  if (!costUnavailable.value) {
    series.push({
      name: t("ops.finance.cost"),
      data: overview.value?.days.map((d) => Number(d.cost) || 0) ?? [],
    });
    series.push({
      name: t("ops.finance.margin"),
      data:
        overview.value?.days.map((d) =>
          d.margin == null || d.margin === "" ? 0 : Number(d.margin) || 0,
        ) ?? [],
    });
  }
  return series;
});

function namedRankColumns(nameLabel: string): ColumnOption<FinanceRankRow>[] {
  return [
    {
      prop: "display_name",
      label: nameLabel,
      minWidth: 120,
      formatter: (row) => rankName(row),
    },
    {
      prop: "revenue",
      label: t("ops.finance.revenue"),
      formatter: (row) => moneyCell(row.revenue),
    },
    {
      prop: "cost",
      label: t("ops.finance.cost"),
      formatter: (row) => moneyOrDash(row.cost, row.cost_status),
    },
    {
      prop: "margin",
      label: t("ops.finance.margin"),
      formatter: (row) => moneyOrDash(row.margin, row.cost_status),
    },
  ];
}

const tenantRankColumns = computed<ColumnOption<FinanceRankRow>[]>(() => [
  {
    prop: "display_name",
    label: t("ops.finance.tenantName"),
    minWidth: 120,
    formatter: (row) => rankName(row),
  },
  {
    prop: "tenant_code",
    label: t("ops.finance.tenantCode"),
    width: 110,
    formatter: (row) => row.tenant_code?.trim() || "—",
  },
  {
    prop: "revenue",
    label: t("ops.finance.revenue"),
    formatter: (row) => moneyCell(row.revenue),
  },
  {
    prop: "cost",
    label: t("ops.finance.cost"),
    formatter: (row) => moneyOrDash(row.cost, row.cost_status),
  },
  {
    prop: "margin",
    label: t("ops.finance.margin"),
    formatter: (row) => moneyOrDash(row.margin, row.cost_status),
  },
]);

const balanceColumns = computed<ColumnOption<BalanceRow>[]>(() => [
  {
    prop: "tenant_name",
    label: t("ops.finance.tenantName"),
    minWidth: 140,
    formatter: (row) => {
      const name = row.tenant_name?.trim();
      if (name) return name;
      const code = row.tenant_code?.trim();
      return code || t("ops.finance.unnamed");
    },
  },
  {
    prop: "tenant_code",
    label: t("ops.finance.tenantCode"),
    width: 110,
    formatter: (row) => row.tenant_code?.trim() || "—",
  },
  {
    prop: "available_balance",
    label: t("ops.finance.available"),
    formatter: (row) => moneyCell(row.available_balance),
  },
  {
    prop: "recent_recharge",
    label: t("ops.finance.recharge"),
    formatter: (row) => moneyCell(row.recent_recharge),
  },
  {
    prop: "avg_daily_spend",
    label: t("ops.finance.avgSpend"),
    formatter: (row) => moneyCell(row.avg_daily_spend),
  },
  {
    prop: "days_remaining",
    label: t("ops.finance.daysLeft"),
    formatter: (row) => daysCell(row.days_remaining),
  },
  {
    prop: "status",
    label: t("ops.finance.status"),
    width: 100,
    formatter: (row) => getStatusLabel(row.status),
  },
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
.finance-cost-alert {
  margin-bottom: 16px;
}
</style>
