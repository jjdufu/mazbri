<template>
  <div class="ops-screen">
    <header class="ops-screen__header">
      <h1>{{ t("ops.screen.title") }}</h1>
      <div class="ops-screen__meta">
        <span>{{ clock }}</span>
        <AutoRefresh v-model="refreshInterval" @tick="loadStats" />
      </div>
    </header>
    <ElRow :gutter="20" class="ops-screen__grid">
      <ElCol v-for="card in cards" :key="card.label" :span="6">
        <div class="ops-screen__card">
          <div class="ops-screen__label">{{ card.label }}</div>
          <div class="ops-screen__value">{{ card.value }}</div>
        </div>
      </ElCol>
    </ElRow>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import AutoRefresh from "@/components/ops/AutoRefresh.vue";
import { fetchPlatformStats } from "@/api/analytics";
import type { PlatformStats } from "@/types/api/analytics";

defineOptions({ name: "OpsScreen" });

const { t } = useI18n();
const stats = ref<PlatformStats | null>(null);
const refreshInterval = ref(30);
const clock = ref("");
let clockTimer: ReturnType<typeof setInterval> | null = null;

const cards = computed(() => [
  {
    label: t("ops.overview.stats.totalRequests"),
    value: formatNum(stats.value?.usage_stats?.total_requests ?? 0),
  },
  {
    label: t("ops.overview.stats.totalBytes"),
    value: formatNum(stats.value?.usage_stats?.total_bytes ?? 0),
  },
  {
    label: t("ops.overview.stats.activeTenants"),
    value: formatNum(stats.value?.tenant_stats?.active_tenants ?? 0),
  },
  {
    label: t("ops.overview.stats.avgResponseMs"),
    value: `${formatNum(stats.value?.usage_stats?.avg_customer_total_ms ?? 0)} ms`,
  },
]);

function formatNum(n: number) {
  return new Intl.NumberFormat().format(n);
}

async function loadStats() {
  const end = Math.floor(Date.now() / 1000);
  try {
    stats.value = await fetchPlatformStats({
      start_time: end - 3600,
      end_time: end,
    });
  } catch {
    // Keep last known values on transient failures.
  }
}

function tickClock() {
  clock.value = new Date().toLocaleString();
}

onMounted(() => {
  tickClock();
  clockTimer = setInterval(tickClock, 1000);
  void loadStats();
});

onUnmounted(() => {
  if (clockTimer) clearInterval(clockTimer);
});
</script>

<style scoped>
.ops-screen {
  min-height: 100vh;
  padding: 24px 32px;
  color: #e8eef7;
  background: radial-gradient(
    circle at top,
    #1a2a44 0%,
    #0b1220 55%,
    #070b14 100%
  );
}
.ops-screen__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 28px;
}
.ops-screen__header h1 {
  margin: 0;
  font-size: 28px;
  font-weight: 600;
  letter-spacing: 0.04em;
}
.ops-screen__meta {
  display: flex;
  gap: 16px;
  align-items: center;
  color: #9fb3cd;
}
.ops-screen__grid {
  margin-top: 8px;
}
.ops-screen__card {
  padding: 28px 24px;
  border: 1px solid rgba(120, 160, 220, 0.25);
  border-radius: 12px;
  background: rgba(16, 28, 48, 0.85);
  box-shadow: inset 0 0 30px rgba(64, 128, 255, 0.08);
}
.ops-screen__label {
  margin-bottom: 12px;
  color: #9fb3cd;
  font-size: 14px;
}
.ops-screen__value {
  font-size: 36px;
  font-weight: 700;
  line-height: 1.1;
}
</style>
