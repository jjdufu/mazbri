<template>
  <ElCard v-loading="loading" shadow="never" class="gate-summary-card">
    <template #header>
      <SurfaceCardHeader>
        <template #left>
          <div class="header-title">
            <ArtSvgIcon icon="ri:dashboard-2-line" class="header-icon" />
            <span>{{ $t("rateLimit.gateSummary.title") }}</span>
          </div>
        </template>
        <template #actions>
          <ElButton link type="primary" :loading="loading" @click="loadSummary">
            <ArtSvgIcon icon="ri:refresh-line" class="mr-1" />
            {{ $t("common.actions.refresh") }}
          </ElButton>
        </template>
      </SurfaceCardHeader>
    </template>

    <div v-if="summary" class="summary-body">
      <div class="summary-grid">
        <div class="summary-item">
          <span class="summary-label">{{
            $t("rateLimit.gateSummary.enabled")
          }}</span>
          <ElTag :type="summary.enabled ? 'success' : 'info'" size="small">
            {{
              summary.enabled
                ? $t("rateLimit.gateSummary.on")
                : $t("rateLimit.gateSummary.off")
            }}
          </ElTag>
        </div>
        <div class="summary-item">
          <span class="summary-label">{{
            $t("rateLimit.gateSummary.planQps")
          }}</span>
          <span class="summary-value">{{ summary.max_concurrent }}</span>
        </div>
        <div class="summary-item">
          <span class="summary-label">{{
            $t("rateLimit.gateSummary.burst")
          }}</span>
          <span class="summary-value">{{ summary.burst_size }}</span>
        </div>
        <div class="summary-item">
          <span class="summary-label">{{
            $t("rateLimit.gateSummary.overrides")
          }}</span>
          <span class="summary-value">{{ overrideCount }}</span>
        </div>
        <div class="summary-item">
          <span class="summary-label">{{
            $t("rateLimit.gateSummary.enforcement")
          }}</span>
          <span class="summary-value mono">{{
            summary.enforcement_model || "local_per_instance"
          }}</span>
        </div>
      </div>
      <p v-if="summary.cluster_note" class="cluster-note">
        {{ summary.cluster_note }}
      </p>
      <div class="summary-links">
        <router-link
          class="summary-link"
          :to="{
            path: `/ops/tenants/${tenantCode}`,
            query: { tab: 'access-strategy' },
          }"
        >
          {{ $t("rateLimit.gateSummary.toPlan") }}
        </router-link>
        <span class="link-sep">·</span>
        <router-link class="summary-link" :to="{ path: '/ops/upstream-supply' }">
          {{ $t("rateLimit.gateSummary.toUpstream") }}
        </router-link>
      </div>
    </div>
    <div v-else-if="!loading" class="summary-empty">
      {{ $t("rateLimit.gateSummary.loadFailed") }}
    </div>
  </ElCard>
</template>

<script setup lang="ts">
import type { TenantRateLimitConfig } from "@/types/api/tenant";
import { fetchTenantRateLimitConfig } from "@/api/tenant";
import SurfaceCardHeader from "@/components/common/surface-card-header/index.vue";
import { logger } from "@/utils/sys";

defineOptions({ name: "TenantGateSummaryCard" });

const props = defineProps<{
  tenantCode: string;
}>();

const loading = ref(false);
const summary = ref<TenantRateLimitConfig | null>(null);

const overrideCount = computed(() => {
  if (!summary.value?.api_limits) return 0;
  return Object.keys(summary.value.api_limits).length;
});

const loadSummary = async () => {
  loading.value = true;
  try {
    summary.value = await fetchTenantRateLimitConfig(props.tenantCode);
  } catch (error) {
    summary.value = null;
    logger.error("Failed to load gate summary:", error);
  } finally {
    loading.value = false;
  }
};

onMounted(loadSummary);
watch(
  () => props.tenantCode,
  () => {
    loadSummary();
  },
);
</script>

<style scoped lang="scss">
.gate-summary-card {
  margin-top: 16px;
}

.header-title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-weight: 600;
  font-size: 15px;
}

.header-icon {
  font-size: 18px;
  color: var(--el-color-primary);
}

.summary-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
  gap: 12px 16px;
}

.summary-item {
  display: flex;
  flex-direction: column;
  gap: 4px;

  .summary-label {
    font-size: 12px;
    color: var(--el-text-color-secondary);
  }

  .summary-value {
    font-size: 15px;
    font-weight: 600;
    color: var(--el-text-color-primary);

    &.mono {
      font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
      font-size: 13px;
      font-weight: 500;
    }
  }
}

.cluster-note {
  margin: 12px 0 0;
  font-size: 12px;
  line-height: 1.5;
  color: var(--el-text-color-secondary);
}

.summary-links {
  margin-top: 14px;
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
  font-size: 13px;

  .summary-link {
    color: var(--el-color-primary);
    font-weight: 500;
    text-decoration: none;

    &:hover {
      text-decoration: underline;
    }
  }

  .link-sep {
    color: var(--el-text-color-placeholder);
  }
}

.summary-empty {
  font-size: 13px;
  color: var(--el-text-color-secondary);
}
</style>
