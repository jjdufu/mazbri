<template>
  <ControlPanelPageShell
    :title="t('menus.mazbri.dashboard')"
    icon="ri:dashboard-line"
  >
    <div class="analytics-overview-page" style="display: flex; flex-direction: column; flex: 1; min-height: 0;">
    <ElRow :gutter="20" class="mb-5">
      <ElCol :sm="12" :md="6" :lg="6">
        <MetricSummaryCard
          :label="t('analytics.metrics.todayRequests')"
          icon="ri:bar-chart-line"
          icon-color="#409eff"
          icon-background="rgba(64, 158, 255, 0.1)"
          :loading="loading"
        >
          <ArtCountTo
            class="metric-summary-card__value"
            :target="metrics?.total_requests_today || 0"
            :duration="1000"
          />
        </MetricSummaryCard>
      </ElCol>
      <ElCol :sm="12" :md="6" :lg="6">
        <MetricSummaryCard
          :label="t('analytics.metrics.weekRequests')"
          icon="ri:line-chart-line"
          icon-color="#67c23a"
          icon-background="rgba(103, 194, 58, 0.1)"
          :loading="loading"
        >
          <ArtCountTo
            class="metric-summary-card__value"
            :target="metrics?.total_requests_week || 0"
            :duration="1000"
          />
        </MetricSummaryCard>
      </ElCol>
      <ElCol :sm="12" :md="6" :lg="6">
        <MetricSummaryCard
          :label="t('analytics.metrics.totalRequests')"
          icon="ri:exchange-line"
          icon-color="#e6a23c"
          icon-background="rgba(230, 162, 60, 0.1)"
          :loading="loading"
        >
          <ArtCountTo
            class="metric-summary-card__value"
            :target="metrics?.total_requests || 0"
            :duration="1000"
          />
        </MetricSummaryCard>
      </ElCol>
      <ElCol :sm="12" :md="6" :lg="6">
        <MetricSummaryCard
          :label="t('analytics.metrics.activeTenants')"
          icon="ri:building-line"
          icon-color="#f56c6c"
          icon-background="rgba(245, 108, 108, 0.1)"
          :loading="loading"
        >
          <ArtCountTo
            class="metric-summary-card__value"
            :target="metrics?.active_tenants || 0"
            :duration="1000"
          />
        </MetricSummaryCard>
      </ElCol>
    </ElRow>

    <ElRow :gutter="20">
      <ElCol :sm="24" :md="16" :lg="16">
        <ElCard shadow="never">
          <template #header>
            <span>{{ t("analytics.requestTrendTitle") }}</span>
          </template>
          <div v-if="loading" class="loading-container">
            <ElSkeleton :rows="5" animated />
          </div>
          <div v-else ref="chartRef" class="chart-container"></div>
        </ElCard>
      </ElCol>

      <ElCol :sm="24" :md="8" :lg="8">
        <ElCard shadow="never">
          <template #header>
            <span>{{ t("analytics.topEndpointsTitle") }}</span>
          </template>
          <div v-if="loading" class="loading-container">
            <ElSkeleton :rows="5" animated />
          </div>
          <div v-else-if="metrics?.top_endpoints?.length" class="endpoint-list">
            <div
              v-for="(endpoint, idx) in metrics.top_endpoints.slice(0, 5)"
              :key="idx"
              class="endpoint-item"
            >
              <div class="endpoint-info">
                <span
                  class="endpoint-method"
                  :class="`method-${endpoint.method.toLowerCase()}`"
                >
                  {{ endpoint.method }}
                </span>
                <span class="endpoint-path">{{ endpoint.endpoint }}</span>
              </div>
              <span class="endpoint-count">{{
                formatNumber(endpoint.request_count)
              }}</span>
            </div>
          </div>
          <ElEmpty v-else :description="t('analytics.noData')" />
        </ElCard>
      </ElCol>
    </ElRow>
    </div>
  </ControlPanelPageShell>
</template>

<script setup lang="ts">
import ControlPanelPageShell from "@/components/common/control-panel-page-shell/index.vue";
import type { PlatformStats, DashboardMetrics } from "@/types/api/analytics";
import { useI18n } from "vue-i18n";
import { fetchPlatformStats } from "@/api/analytics";
import { graphic, type EChartsOption } from "@/plugins/echarts";
import MetricSummaryCard from "@/components/common/metric-summary-card/index.vue";
import { logger } from "@/utils/sys";
import { useECharts } from "@/composables/useECharts";
import { runWithLoading } from "@/composables/useAsyncState";
defineOptions({ name: "AnalyticsOverview" });
const { t } = useI18n();
const loading = ref(false);
const metrics = ref<DashboardMetrics | null>(null);
const { chartRef, initChart } = useECharts();
const formatNumber = (num: number) => {
  return new Intl.NumberFormat().format(num);
};
const transformToDashboardMetrics = (
  stats: PlatformStats,
): DashboardMetrics => {
  return {
    total_requests_today: stats.usage_stats?.total_requests || 0,
    total_requests_week: stats.usage_stats?.total_requests || 0,
    total_requests: stats.usage_stats?.total_requests || 0,
    active_tenants: stats.tenant_stats?.active_tenants || 0,
    top_endpoints:
      stats.top_products?.map((p) => ({
        endpoint: p.product_endpoint,
        method: "GET",
        request_count: p.request_count,
        avg_billing_service_duration_ms: p.avg_service_total_ms || 0,
        total_cost: p.total_cost,
      })) || [],
    requests_by_day: [],
  };
};
const initRequestChart = () => {
  if (!metrics.value?.requests_by_day?.length) return;
  const option: EChartsOption = {
    tooltip: {
      trigger: "axis",
    },
    grid: {
      left: "3%",
      right: "4%",
      bottom: "3%",
      containLabel: true,
    },
    xAxis: {
      type: "category",
      data: metrics.value.requests_by_day.map((d) => d.date),
      boundaryGap: false,
    },
    yAxis: {
      type: "value",
      name: t("analytics.requestCountAxis"),
    },
    series: [
      {
        name: t("analytics.requestCountAxis"),
        type: "line",
        data: metrics.value.requests_by_day.map((d) => d.count),
        smooth: true,
        areaStyle: {
          color: new graphic.LinearGradient(0, 0, 1, 1, [
            { offset: 0, color: "rgba(64, 158, 255, 0.3)" },
            { offset: 1, color: "rgba(64, 158, 255, 0.05)" },
          ]),
        },
        lineStyle: {
          color: "#409eff",
        },
        itemStyle: {
          color: "#409eff",
        },
      },
    ],
  };
  initChart(option);
};
const loadData = async () => {
  await runWithLoading(loading, async () => {
    const platformStats = await fetchPlatformStats();
    metrics.value = transformToDashboardMetrics(platformStats);
    nextTick(() => initRequestChart());
  }).catch((error) => {
    logger.error("Failed to load metrics:", error);
  });
};
onMounted(() => {
  loadData();
});
</script>

<style scoped lang="scss">
.chart-container {
  width: 100%;
  height: 300px;
}

.loading-container {
  height: 300px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.endpoint-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.endpoint-item {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 8px 0;
  border-bottom: 1px solid var(--border-color-lighter);

  &:last-child {
    border-bottom: none;
  }
}

.endpoint-info {
  display: flex;
  align-items: center;
  gap: 8px;
  overflow: hidden;
}

.endpoint-method {
  font-size: 12px;
  font-weight: 600;
  padding: 2px 6px;
  border-radius: 4px;

  &.method-get {
    background-color: rgba(103, 194, 58, 0.1);
    color: #67c23a;
  }

  &.method-post {
    background-color: rgba(64, 158, 255, 0.1);
    color: #409eff;
  }

  &.method-put {
    background-color: rgba(230, 162, 60, 0.1);
    color: #e6a23c;
  }

  &.method-delete {
    background-color: rgba(245, 108, 108, 0.1);
    color: #f56c6c;
  }
}

.endpoint-path {
  font-size: 13px;
  color: var(--text-color-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.endpoint-count {
  font-size: 14px;
  font-weight: 500;
  color: var(--text-color-secondary);
}
</style>
