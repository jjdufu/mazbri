<template>
  <TenantTabShell :loading="loading">
    <OpsKpiRow>
      <ElCol :xs="12" :sm="12" :md="8" :lg="4">
        <StatCard
          :label="t('tenant.overview360.calls')"
          :value="overview?.calls ?? null"
          :loading="loading"
          :digits="0"
        />
      </ElCol>
      <ElCol :xs="12" :sm="12" :md="8" :lg="4">
        <StatCard
          :label="t('tenant.overview360.successRate')"
          :value="overview?.success_rate ?? null"
          unit="%"
          :loading="loading"
          :digits="1"
        />
      </ElCol>
      <ElCol :xs="12" :sm="12" :md="8" :lg="4">
        <StatCard
          :label="t('tenant.overview360.p95')"
          :value="overview?.p95_ms ?? null"
          unit="ms"
          :loading="loading"
          :digits="0"
        />
      </ElCol>
      <ElCol :xs="12" :sm="12" :md="8" :lg="4">
        <StatCard
          :label="t('tenant.overview360.tenantSpend')"
          :value="spendNum"
          :hint="spend?.window?.label"
          :loading="loading"
          :digits="4"
        />
      </ElCol>
      <ElCol :xs="12" :sm="12" :md="8" :lg="4">
        <StatCard
          :label="t('tenant.overview360.health')"
          :value="healthScore"
          :loading="loading"
          :digits="0"
        />
      </ElCol>
    </OpsKpiRow>

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
import OpsKpiRow from "@/components/ops/OpsKpiRow.vue";
import TenantTabShell from "@/components/common/tenant-tab-shell/index.vue";
import EmptyState from "@/components/common/empty-state/index.vue";
import { postOlapQuery } from "@/api/olap";
import { presetWindow } from "@/composables/ops/useOlapRange";
import { fetchTenantSpend, type TenantSpend } from "@/api/tenant";
import type { TenantErrorRow, TenantOverviewRow } from "@/types/api/olap";
import type { ColumnOption } from "@/types/component";

const props = defineProps<{ tenantCode: string }>();
defineOptions({ name: "TenantOverview360" });

const { t } = useI18n();
const loading = ref(false);
const overview = ref<TenantOverviewRow | null>(null);
const spend = ref<TenantSpend | null>(null);
const errors = ref<TenantErrorRow[]>([]);

const spendNum = computed(() => {
  const n = Number(spend.value?.tenant_spend ?? NaN);
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

async function reload() {
  if (!props.tenantCode) return;
  loading.value = true;
  try {
    const range = last30DayRange();
    const [ov, er, sp] = await Promise.all([
      postOlapQuery<TenantOverviewRow>({
        template: "tenant_overview",
        params: { tenant_code: props.tenantCode, ...presetWindow("24h") },
      }),
      postOlapQuery<TenantErrorRow>({
        template: "tenant_errors",
        params: {
          tenant_code: props.tenantCode,
          ...presetWindow("24h"),
          limit: 20,
        },
      }),
      fetchTenantSpend(props.tenantCode, range),
    ]);
    overview.value = ov.rows?.[0] ?? null;
    errors.value = er.rows ?? [];
    spend.value = sp;
  } catch {
    ElMessage.error(t("tenant.overview360.loadFailed"));
  } finally {
    loading.value = false;
  }
}

watch(() => props.tenantCode, reload);
onMounted(reload);
</script>
