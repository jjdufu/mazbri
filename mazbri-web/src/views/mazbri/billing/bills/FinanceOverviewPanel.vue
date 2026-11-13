<template>
  <div class="finance-overview-panel ops-page-stack">
    <Teleport
      v-if="props.active"
      :to="tabActionsEl || 'body'"
      :disabled="!tabActionsEl"
    >
      <div
        class="finance-overview__toolbar"
        :class="{ 'finance-overview__toolbar--inline': !tabActionsEl }"
      >
        <ElSelect v-model="days" class="finance-days-select" @change="reload">
          <ElOption :value="7" :label="t('ops.timeRange.presets.7d')" />
          <ElOption :value="30" :label="t('ops.timeRange.presets.30d')" />
          <ElOption :value="90" :label="`90 ${t('ops.units.days')}`" />
        </ElSelect>
        <AutoRefresh v-model="refreshInterval" @tick="onAutoRefreshTick" />
      </div>
    </Teleport>

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
          :hint="costHint"
          :loading="loading"
          :digits="2"
        />
      </ElCol>
      <ElCol :xs="12" :sm="12" :md="8" :lg="6">
        <StatCard
          :label="t('ops.finance.margin')"
          :value="costUnavailable ? null : num(overview?.margin_total)"
          :hint="costHint"
          :loading="loading"
          :digits="2"
        />
      </ElCol>
      <ElCol :xs="12" :sm="12" :md="8" :lg="6">
        <StatCard
          :label="t('ops.finance.marginRate')"
          :value="costUnavailable ? null : (overview?.margin_rate ?? null)"
          unit="%"
          :hint="costHint"
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
        unit-type="amount"
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
            empty-height="160px"
            stripe
          />
        </OpsCard>
      </ElCol>
      <ElCol :xs="24" :lg="8">
        <OpsCard :title="t('ops.finance.rankProduct')">
          <ArtTable
            :data="ranks?.by_product ?? []"
            :loading="loading"
            :columns="productRankColumns"
            empty-height="160px"
            stripe
          />
        </OpsCard>
      </ElCol>
      <ElCol :xs="24" :lg="8">
        <OpsCard :title="t('ops.finance.rankSupplier')">
          <ArtTable
            :data="ranks?.by_supplier ?? []"
            :loading="loading"
            :columns="supplierRankColumns"
            empty-height="160px"
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
        empty-height="160px"
        stripe
      >
        <template #available_balance-header>
          <span class="balance-header-with-tip">
            <span>{{ t("ops.finance.available") }}</span>
            <ElTooltip
              :content="t('ops.finance.availableTooltip')"
              placement="top"
            >
              <span class="balance-header-tip-icon" tabindex="0">
                <ArtSvgIcon icon="ri:question-line" />
              </span>
            </ElTooltip>
          </span>
        </template>
      </ArtTable>
    </OpsCard>
  </div>
</template>

<script setup lang="ts">
import { computed, inject, onMounted, ref, type Ref } from "vue";
import { useI18n } from "vue-i18n";
import { ElMessage } from "element-plus";
import AutoRefresh from "@/components/ops/AutoRefresh.vue";
import CategoryBarChart from "@/components/ops/CategoryBarChart.vue";
import StatCard from "@/components/ops/StatCard.vue";
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

defineOptions({ name: "FinanceOverviewPanel" });

const props = withDefaults(
  defineProps<{
    active?: boolean;
  }>(),
  {
    active: true,
  },
);

const tabActionsEl = inject<Ref<HTMLElement | null> | null>(
  "billsTabActionsEl",
  null,
);

const { t } = useI18n();
const days = ref(30);
const refreshInterval = ref(60);
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
  if (costStatus === "unavailable") return "—";
  return moneyCell(value);
}

const costUnavailable = computed(
  () => overview.value != null && overview.value.cost_status === "unavailable",
);

const costPartial = computed(
  () => overview.value?.cost_status === "partial",
);

const windowHint = computed(
  () =>
    overview.value?.window?.label ??
    t("ops.finance.windowFallback", { days: days.value }),
);

const costHint = computed(() =>
  costPartial.value
    ? `${windowHint.value} · ${t("ops.finance.partialBadge")}`
    : windowHint.value,
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

const productRankColumns = computed<ColumnOption<FinanceRankRow>[]>(() => [
  {
    prop: "display_name",
    label: t("ops.finance.productName"),
    minWidth: 160,
    formatter: (row) => rankName(row),
  },
  {
    prop: "revenue",
    label: t("ops.finance.revenue"),
    width: 108,
    formatter: (row) => moneyCell(row.revenue),
  },
]);

const supplierRankColumns = computed<ColumnOption<FinanceRankRow>[]>(() => [
  {
    prop: "display_name",
    label: t("ops.finance.supplierName"),
    minWidth: 160,
    formatter: (row) => rankName(row),
  },
  {
    prop: "cost",
    label: t("ops.finance.cost"),
    width: 108,
    formatter: (row) => moneyOrDash(row.cost, row.cost_status),
  },
]);

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
    width: 104,
    formatter: (row) => row.tenant_code?.trim() || "—",
  },
  {
    prop: "revenue",
    label: t("ops.finance.revenue"),
    width: 96,
    formatter: (row) => moneyCell(row.revenue),
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
    minWidth: 108,
    formatter: (row) => row.tenant_code?.trim() || "—",
  },
  {
    prop: "balance",
    label: t("ops.finance.cashBalance"),
    minWidth: 132,
    formatter: (row) => moneyCell(row.balance),
  },
  {
    prop: "credit_limit",
    label: t("ops.finance.creditLimit"),
    minWidth: 108,
    formatter: (row) => moneyCell(row.credit_limit),
  },
  {
    prop: "available_balance",
    label: t("ops.finance.available"),
    minWidth: 132,
    useHeaderSlot: true,
    formatter: (row) => moneyCell(row.available_balance),
  },
  {
    prop: "recent_recharge",
    label: t("ops.finance.recharge"),
    minWidth: 116,
    formatter: (row) => moneyCell(row.recent_recharge),
  },
  {
    prop: "avg_daily_spend",
    label: t("ops.finance.avgSpend"),
    minWidth: 116,
    formatter: (row) => moneyCell(row.avg_daily_spend),
  },
  {
    prop: "days_remaining",
    label: t("ops.finance.daysLeft"),
    minWidth: 96,
    formatter: (row) => daysCell(row.days_remaining),
  },
  {
    prop: "status",
    label: t("ops.finance.status"),
    width: 96,
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

function onAutoRefreshTick() {
  if (!props.active) return;
  void reload();
}

onMounted(() => {
  void reload();
});
</script>

<style scoped lang="scss">
.finance-overview-panel {
  display: flex;
  flex-direction: column;
  gap: 16px;
  min-width: 0;
}

.finance-overview__toolbar {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
}

.finance-overview__toolbar--inline {
  align-self: flex-end;
}

.finance-days-select {
  width: 120px;
  --el-select-width: 120px;
  flex-shrink: 0;
}

.balance-header-with-tip {
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.balance-header-tip-icon {
  display: inline-flex;
  align-items: center;
  color: var(--el-text-color-secondary);
  cursor: help;
}
</style>
