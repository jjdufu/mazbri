<template>
  <div class="ops-overview art-full-height">
    <div class="ops-overview__toolbar">
      <h2 class="ops-overview__title">{{ t("menus.mazbri.opsOverview") }}</h2>
      <div class="ops-overview__actions">
        <TimeRangePicker v-model="timeRange" @change="loadStats" />
        <AutoRefresh v-model="refreshInterval" @tick="loadStats" />
        <ElButton type="primary" @click="openScreen">
          <ArtSvgIcon icon="ri:tv-2-line" class="mr-1" />
          {{ t("ops.overview.openScreen") }}
        </ElButton>
      </div>
    </div>

    <ElRow :gutter="16" class="ops-overview__cards">
      <ElCol :xs="24" :sm="12" :lg="6">
        <StatCard
          :label="t('ops.overview.stats.totalRequests')"
          :value="stats?.usage_stats?.total_requests"
          :loading="loading"
          :hide-amount="isViewerSlim"
          icon="ri:bar-chart-box-line"
        />
      </ElCol>
      <ElCol :xs="24" :sm="12" :lg="6">
        <StatCard
          :label="t('ops.overview.stats.totalBytes')"
          :value="stats?.usage_stats?.total_bytes"
          :loading="loading"
          :hide-amount="isViewerSlim"
          icon="ri:database-2-line"
          icon-color="#67c23a"
          icon-background="rgba(103, 194, 58, 0.12)"
        />
      </ElCol>
      <ElCol :xs="24" :sm="12" :lg="6">
        <StatCard
          :label="t('ops.overview.stats.activeTenants')"
          :value="stats?.tenant_stats?.active_tenants"
          :loading="loading"
          icon="ri:building-line"
        />
      </ElCol>
      <ElCol :xs="24" :sm="12" :lg="6">
        <StatCard
          :label="t('ops.overview.stats.avgResponseMs')"
          :value="stats?.usage_stats?.avg_customer_total_ms"
          unit="ms"
          :loading="loading"
          icon="ri:timer-line"
          icon-color="#e6a23c"
          icon-background="rgba(230, 162, 60, 0.12)"
        />
      </ElCol>
    </ElRow>

    <ElAlert
      v-if="isViewerSlim"
      type="info"
      :closable="false"
      class="ops-overview__slim"
      :title="t('ops.overview.viewerSlim')"
    />
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { ElMessage } from "element-plus";
import TimeRangePicker, {
  type TimeRangeValue,
} from "@/components/ops/TimeRangePicker.vue";
import AutoRefresh from "@/components/ops/AutoRefresh.vue";
import StatCard from "@/components/ops/StatCard.vue";
import { fetchPlatformStats } from "@/api/analytics";
import type { PlatformStats } from "@/types/api/analytics";
import { useUserStore } from "@/store/modules/user";

defineOptions({ name: "OpsOverview" });

const { t } = useI18n();
const userStore = useUserStore();

const loading = ref(false);
const stats = ref<PlatformStats | null>(null);
const refreshInterval = ref(0);
const timeRange = ref<TimeRangeValue>({
  preset: "1h",
  start: Math.floor(Date.now() / 1000) - 3600,
  end: Math.floor(Date.now() / 1000),
});

const roles = computed(() => (userStore.info?.roles as string[]) || []);
const isViewerSlim = computed(
  () => roles.value.includes("R_VIEWER") && !roles.value.includes("R_ADMIN"),
);

async function loadStats() {
  loading.value = true;
  try {
    stats.value = await fetchPlatformStats({
      start_time: timeRange.value.start,
      end_time: timeRange.value.end,
    });
  } catch (error) {
    ElMessage.error(
      error instanceof Error ? error.message : t("ops.overview.loadFailed"),
    );
  } finally {
    loading.value = false;
  }
}

function openScreen() {
  const url = `${window.location.origin}${window.location.pathname}#/screen`;
  window.open(url, "_blank", "noopener,noreferrer");
}

onMounted(loadStats);
</script>

<style scoped>
.ops-overview {
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding: 16px;
}
.ops-overview__toolbar {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  align-items: center;
  justify-content: space-between;
}
.ops-overview__title {
  margin: 0;
  font-size: 20px;
  font-weight: 600;
}
.ops-overview__actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
}
.ops-overview__cards {
  margin-top: 4px;
}
.ops-overview__slim {
  margin-top: 8px;
}
</style>
