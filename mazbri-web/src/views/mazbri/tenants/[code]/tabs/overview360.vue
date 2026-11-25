<template>
  <TenantTabShell :loading="loading">
    <div class="tenant-kpi-grid" :class="{ 'tenant-kpi-grid--ops-only': !isBiz }">
      <StatCard
        :label="t('tenant.overview360.calls')"
        :value="overview?.calls ?? null"
        :loading="loading"
        :digits="0"
      />
      <StatCard
        :label="t('tenant.overview360.successRate')"
        :value="overview?.success_rate ?? null"
        unit="%"
        :loading="loading"
        :digits="1"
      />
      <StatCard
        :label="t('tenant.overview360.p95')"
        :value="overview?.p95_ms ?? null"
        unit="ms"
        :loading="loading"
        :digits="0"
      />
      <StatCard
        v-if="isBiz"
        :label="t('tenant.overview360.tenantSpend')"
        :value="spendNum"
        :hint="spend?.window?.label"
        :loading="loading"
        :digits="4"
      />
      <StatCard
        :label="t('tenant.overview360.health')"
        :value="healthScore"
        :loading="loading"
        :digits="0"
      />
    </div>

    <OpsCard :title="t('tenant.overview360.errors')">
      <ArtTable
        v-if="errors.length || loading"
        :data="errors"
        :loading="loading"
        :columns="errorColumns"
        stripe
      />
      <EmptyState
        v-else
        compact
        :description="t('common.feedback.noData')"
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
import TenantTabShell from "@/components/common/tenant-tab-shell/index.vue";
import EmptyState from "@/components/common/empty-state/index.vue";
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

const spendNum = computed(() => {
  const raw = spend.value?.tenant_spend;
  if (raw == null || raw === "") return null;
  const n = Number(raw);
  return Number.isFinite(n) ? n : null;
});

/** 成功率 50% + 时延 30% + 活跃度 20%；时延以 1000ms 为满分门槛。 */
const healthScore = computed(() => {
  if (!overview.value) return null;
  const success = Math.min(Math.max(overview.value.success_rate, 0), 100) / 100;
  const latency = Math.max(0, 1 - overview.value.p95_ms / 1000);
  const activity = overview.value.calls > 0 ? 1 : 0;
  return Math.round((success * 0.5 + latency * 0.3 + activity * 0.2) * 100);
});

const errorColumns = computed<ColumnOption<TenantErrorRow>[]>(() => [
  { prop: "error_code", label: t("tenant.overview360.errorCode") },
  { prop: "count", label: t("tenant.overview360.count") },
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

@media (max-width: 1100px) {
  .tenant-kpi-grid {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }
}

@media (max-width: 768px) {
  .tenant-kpi-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}
</style>
