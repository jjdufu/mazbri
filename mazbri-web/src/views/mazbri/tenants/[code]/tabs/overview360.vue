<template>
  <TenantTabShell :loading="loading">
    <div class="tenant-kpi-grid" :class="{ 'tenant-kpi-grid--ops-only': !isBiz }">
      <StatCard
        :label="t('tenant.overview360.calls')"
        :value="overview?.calls ?? null"
        icon="ri:exchange-box-line"
        :hint="t('ops.timeRange.presets.24h')"
        :loading="loading"
        :digits="0"
      />
      <StatCard
        :label="t('tenant.overview360.successRate')"
        :value="overview?.success_rate ?? null"
        unit="%"
        icon="ri:checkbox-circle-line"
        icon-color="#67c23a"
        icon-background="rgba(103, 194, 58, 0.12)"
        :hint="t('ops.timeRange.presets.24h')"
        :loading="loading"
        :digits="1"
      />
      <StatCard
        :label="t('tenant.overview360.p95')"
        :value="overview?.p95_ms ?? null"
        unit="ms"
        icon="ri:timer-flash-line"
        icon-color="#e6a23c"
        icon-background="rgba(230, 162, 60, 0.12)"
        :hint="t('ops.timeRange.presets.24h')"
        :loading="loading"
        :digits="0"
      />
      <StatCard
        v-if="isBiz"
        :label="t('tenant.overview360.tenantSpend')"
        :value="spendDisplay"
        :unit="t('common.currencies.cny')"
        icon="ri:money-cny-circle-line"
        :hint="spend?.window?.label || t('ops.timeRange.presets.30d')"
        :loading="loading"
      />
      <StatCard
        :label="t('tenant.overview360.health')"
        :value="healthScore"
        icon="ri:heart-pulse-line"
        :hint="t('ops.timeRange.presets.24h')"
        :loading="loading"
        :digits="0"
      />
    </div>

    <OpsCard :title="t('tenant.overview360.errors')">
      <div v-if="errors.length || loading" class="tenant-errors-grid">
        <CategoryBarChart
          :categories="errorChartCategories"
          :series="errorChartSeries"
          :loading="loading"
          horizontal
          unit-type="count"
          height="240px"
        />
        <ArtTable
          :data="errorTableRows"
          :loading="loading"
          :columns="errorColumns"
          stripe
        />
      </div>
      <EmptyState
        v-else
        compact
        :description="t('tenant.overview360.noErrors24h')"
      />
    </OpsCard>
  </TenantTabShell>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { ElMessage } from "element-plus";
import StatCard from "@/components/ops/StatCard.vue";
import OpsCard from "@/components/ops/OpsCard.vue";
import CategoryBarChart from "@/components/ops/CategoryBarChart.vue";
import TenantTabShell from "@/components/common/tenant-tab-shell/index.vue";
import EmptyState from "@/components/common/empty-state/index.vue";
import { formatMoney } from "@/utils/format";
import { postOlapQuery } from "@/api/olap";
import { presetWindow } from "@/composables/ops/useOlapRange";
import { useOpsRole } from "@/composables/ops/useOpsRole";
import { useLatestRequest } from "@/composables/useLatestRequest";
import { fetchTenantSpend, type TenantSpend } from "@/api/tenant";
import type { TenantErrorRow, TenantOverviewRow } from "@/types/api/olap";
import type { ColumnOption } from "@/types/component";

const props = defineProps<{ tenantCode: string }>();
defineOptions({ name: "TenantOverview360" });

const { t } = useI18n();
const { isBiz } = useOpsRole();
const overview = ref<TenantOverviewRow | null>(null);
const spend = ref<TenantSpend | null>(null);
const errors = ref<TenantErrorRow[]>([]);

const spendDisplay = computed(() => {
  const raw = spend.value?.tenant_spend;
  if (raw == null || raw === "") return null;
  const formatted = formatMoney(raw);
  return formatted === "—" ? null : formatted;
});

/** 成功率 50% + 时延 30% + 活跃度 20%；时延以 1000ms 为满分门槛。 */
const healthScore = computed(() => {
  if (!overview.value) return null;
  const success = Math.min(Math.max(overview.value.success_rate, 0), 100) / 100;
  const latency = Math.max(0, 1 - overview.value.p95_ms / 1000);
  const activity = overview.value.calls > 0 ? 1 : 0;
  return Math.round((success * 0.5 + latency * 0.3 + activity * 0.2) * 100);
});

interface TenantErrorTableRow extends TenantErrorRow {
  share: string;
}

const totalErrorCount = computed(() =>
  errors.value.reduce((acc, row) => acc + (Number(row.count) || 0), 0),
);

const errorTableRows = computed<TenantErrorTableRow[]>(() =>
  errors.value.map((row) => ({
    ...row,
    share:
      totalErrorCount.value > 0
        ? `${((Number(row.count) / totalErrorCount.value) * 100).toFixed(1)}%`
        : "0.0%",
  })),
);

const errorChartCategories = computed(() =>
  errors.value.slice(0, 10).map((row) => row.error_code),
);

const errorChartSeries = computed(() => [
  {
    name: t("tenant.overview360.count"),
    data: errors.value.slice(0, 10).map((row) => Number(row.count) || 0),
  },
]);

const errorColumns = computed<ColumnOption<TenantErrorTableRow>[]>(() => [
  { prop: "error_code", label: t("tenant.overview360.errorCode"), minWidth: 160 },
  {
    prop: "count",
    label: t("tenant.overview360.count"),
    width: 110,
    align: "right",
    numeric: true,
  },
  {
    prop: "share",
    label: t("ops.common.share"),
    width: 96,
    align: "right",
    numeric: true,
  },
]);

function last30DayRange() {
  const to = new Date();
  const from = new Date(to);
  from.setUTCDate(from.getUTCDate() - 29);
  const fmt = (d: Date) => d.toISOString().slice(0, 10);
  return { from: fmt(from), to: fmt(to) };
}

const { loading, run: runReload, cancel: cancelReload } = useLatestRequest(
  async (ctx, code: string, bizAllowed: boolean) => {
    const range = last30DayRange();
    const windowParams = presetWindow("24h");
    const [ov, er, sp] = await Promise.all([
      postOlapQuery<TenantOverviewRow>(
        {
          template: "tenant_overview",
          params: { tenant_code: code, ...windowParams },
        },
        ctx.signal,
      ),
      postOlapQuery<TenantErrorRow>(
        {
          template: "tenant_errors",
          params: {
            tenant_code: code,
            ...windowParams,
            limit: 20,
          },
        },
        ctx.signal,
      ),
      bizAllowed
        ? fetchTenantSpend(code, range, ctx.signal)
        : Promise.resolve(null),
    ]);
    if (!ctx.isLatest()) return;
    overview.value = ov.rows?.[0] ?? null;
    errors.value = er.rows ?? [];
    spend.value = sp;
  },
);

async function reload() {
  if (!props.tenantCode) {
    cancelReload();
    overview.value = null;
    errors.value = [];
    spend.value = null;
    return;
  }
  try {
    await runReload(props.tenantCode, isBiz.value);
  } catch {
    ElMessage.error(t("tenant.overview360.loadFailed"));
  }
}

watch([() => props.tenantCode, isBiz], () => void reload());
onMounted(() => void reload());
</script>

<style scoped lang="scss">
.tenant-kpi-grid {
  display: grid;
  grid-template-columns: repeat(5, minmax(0, 1fr));
  gap: 12px;

  &--ops-only {
    grid-template-columns: repeat(4, minmax(0, 1fr));
  }
}

@media (max-width: 1360px) {
  .tenant-kpi-grid {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }
}

@media (max-width: 768px) {
  .tenant-kpi-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}

.tenant-errors-grid {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  gap: 16px;
  align-items: start;
}

@media (max-width: 960px) {
  .tenant-errors-grid {
    grid-template-columns: 1fr;
  }
}
</style>
