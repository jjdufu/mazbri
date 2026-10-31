<template>
  <div class="ops-service-detail art-full-height">
    <div class="ops-service-detail__toolbar">
      <div>
        <ElButton link type="primary" @click="router.push('/ops/services')">
          ← {{ t("ops.services.backToMatrix") }}
        </ElButton>
        <h2>{{ title }}</h2>
      </div>
      <div class="ops-service-detail__actions">
        <TimeRangePicker v-model="timeRange" @change="reload" />
        <AutoRefresh v-model="refreshInterval" @tick="reload" />
      </div>
    </div>

    <ElRow :gutter="16">
      <ElCol :xs="24" :sm="8">
        <StatCard
          :label="t('ops.services.detail.health')"
          :value="healthLabel"
          :loading="loading"
          icon="ri:heart-pulse-line"
        />
      </ElCol>
      <ElCol :xs="24" :sm="8">
        <StatCard
          :label="t('ops.services.detail.dbPool')"
          :value="poolLabel"
          :loading="loading"
          icon="ri:database-2-line"
        />
      </ElCol>
      <ElCol :xs="24" :sm="8">
        <StatCard
          :label="t('ops.services.detail.version')"
          :value="row?.version || '—'"
          :loading="loading"
          icon="ri:git-commit-line"
        />
      </ElCol>
    </ElRow>

    <ElCard shadow="never" class="ops-service-detail__section">
      <template #header>RED</template>
      <ElRow :gutter="16">
        <ElCol :xs="24" :lg="8">
          <MultiSeriesChart :series="qpsSeries" :loading="loading" height="220px" />
        </ElCol>
        <ElCol :xs="24" :lg="8">
          <MultiSeriesChart
            :series="errorSeries"
            :loading="loading"
            height="220px"
          />
        </ElCol>
        <ElCol :xs="24" :lg="8">
          <MultiSeriesChart
            :series="latencySeries"
            :loading="loading"
            height="220px"
          />
        </ElCol>
      </ElRow>
    </ElCard>

    <ElCard shadow="never" class="ops-service-detail__section">
      <template #header>USE</template>
      <ElAlert
        type="info"
        :closable="false"
        :title="t('ops.services.detail.usePending')"
      />
    </ElCard>

    <ElCard shadow="never" class="ops-service-detail__section">
      <template #header>{{ t("ops.services.detail.specific") }}</template>
      <ElEmpty
        v-if="!specificMetrics.length"
        :description="t('ops.services.detail.noSpecific')"
      />
      <ElRow v-else :gutter="16">
        <ElCol
          v-for="item in specificMetrics"
          :key="item.key"
          :xs="24"
          :lg="12"
        >
          <div class="ops-service-detail__metric-title">{{ item.label }}</div>
          <MultiSeriesChart
            :series="specificSeries[item.key]"
            :loading="loading"
            height="200px"
          />
        </ElCol>
      </ElRow>
    </ElCard>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useRoute, useRouter } from "vue-router";
import { ElMessage } from "element-plus";
import TimeRangePicker, {
  type TimeRangeValue,
} from "@/components/ops/TimeRangePicker.vue";
import AutoRefresh from "@/components/ops/AutoRefresh.vue";
import StatCard from "@/components/ops/StatCard.vue";
import MultiSeriesChart from "@/components/ops/MultiSeriesChart.vue";
import {
  fetchMetricTimeseries,
  fetchMetricsCatalog,
  fetchServicesHealth,
} from "@/api/monitoring";
import type {
  MetricCatalogItem,
  MetricSeries,
  ServiceHealthRow,
} from "@/types/api/monitoring";

defineOptions({ name: "OpsServiceDetail" });

const { t } = useI18n();
const route = useRoute();
const router = useRouter();

const serviceKey = computed(() => String(route.params.service || ""));
const loading = ref(false);
const row = ref<ServiceHealthRow | null>(null);
const qpsSeries = ref<MetricSeries | null>(null);
const errorSeries = ref<MetricSeries | null>(null);
const latencySeries = ref<MetricSeries | null>(null);
const specificSeries = ref<Record<string, MetricSeries | null>>({});
const catalogMetrics = ref<MetricCatalogItem[]>([]);
const refreshInterval = ref(30);
const timeRange = ref<TimeRangeValue>({
  preset: "1h",
  start: Math.floor(Date.now() / 1000) - 3600,
  end: Math.floor(Date.now() / 1000),
});

const title = computed(
  () => row.value?.name || serviceKey.value || t("menus.mazbri.serviceDetail"),
);
const healthLabel = computed(() => {
  if (!row.value) return "—";
  return row.value.health_ok ? t("ops.services.ok") : t("ops.services.down");
});
const poolLabel = computed(() => {
  if (row.value?.db_pool_usage === null || row.value?.db_pool_usage === undefined) {
    return "—";
  }
  return `${(row.value.db_pool_usage * 100).toFixed(1)}%`;
});

const specificPrefix = computed(() => {
  const key = serviceKey.value.replace(/^mazbri-/, "").replace(/-/g, "_");
  if (key === "gateway") return "gateway_";
  if (key === "auth") return "auth_";
  if (key === "cert") return "cert_";
  if (key === "billing") return "billing_";
  if (key === "cache") return "cache_";
  if (key === "bypass") return "bypass_";
  if (key.startsWith("cbpro")) return "cbpro_";
  return `${key}_`;
});

const specificMetrics = computed(() =>
  catalogMetrics.value
    .filter((m) => m.key.startsWith(specificPrefix.value))
    .slice(0, 4),
);

async function reload() {
  if (!serviceKey.value) return;
  loading.value = true;
  try {
    const [healthRows, catalog, qps, errors, latency] = await Promise.all([
      fetchServicesHealth(),
      fetchMetricsCatalog(),
      fetchMetricTimeseries({
        metric: "service_qps",
        start: timeRange.value.start,
        end: timeRange.value.end,
        service: serviceKey.value,
      }).catch(() => null),
      fetchMetricTimeseries({
        metric: "error_rate",
        start: timeRange.value.start,
        end: timeRange.value.end,
        service: serviceKey.value,
      }).catch(() => null),
      fetchMetricTimeseries({
        metric: "p95_latency_ms",
        start: timeRange.value.start,
        end: timeRange.value.end,
        service: serviceKey.value,
      }).catch(() => null),
    ]);
    row.value =
      healthRows.find((item) => item.service === serviceKey.value) || null;
    catalogMetrics.value = catalog.metrics || [];
    qpsSeries.value = qps;
    errorSeries.value = errors;
    latencySeries.value = latency;

    const next: Record<string, MetricSeries | null> = {};
    await Promise.all(
      specificMetrics.value.map(async (item) => {
        next[item.key] = await fetchMetricTimeseries({
          metric: item.key,
          start: timeRange.value.start,
          end: timeRange.value.end,
          service: serviceKey.value,
        }).catch(() => null);
      }),
    );
    specificSeries.value = next;
  } catch (error) {
    ElMessage.error(
      error instanceof Error ? error.message : t("ops.services.loadFailed"),
    );
  } finally {
    loading.value = false;
  }
}

watch(serviceKey, () => {
  void reload();
});

onMounted(reload);
</script>

<style scoped>
.ops-service-detail {
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding: 16px;
}
.ops-service-detail__toolbar {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  align-items: flex-start;
  justify-content: space-between;
}
.ops-service-detail__toolbar h2 {
  margin: 4px 0 0;
  font-size: 20px;
  font-weight: 600;
}
.ops-service-detail__actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
}
.ops-service-detail__section {
  margin-top: 4px;
}
.ops-service-detail__metric-title {
  margin-bottom: 8px;
  font-weight: 600;
}
</style>
