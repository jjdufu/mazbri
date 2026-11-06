<template>
  <ElRow :gutter="16" class="ops-equal-row">
    <ElCol :xs="24" :lg="8">
      <OpsCard :title="t('ops.overview.securityGov.topIp')">
        <template #extra>
          <ElButton link type="primary" @click="openCalls">
            {{ t("ops.overview.securityGov.openCalls") }}
          </ElButton>
        </template>
        <div v-loading="loading" class="sec-gov__list">
          <button
            v-for="row in topIp"
            :key="row.dim_value"
            type="button"
            class="sec-gov__row"
            @click="openCalls"
          >
            <span class="sec-gov__dim">{{ row.dim_value || "—" }}</span>
            <span class="sec-gov__fails">{{ row.fail_count }}</span>
          </button>
          <EmptyState
            v-if="!loading && !topIp.length"
            compact
            :description="t('ops.overview.securityGov.empty')"
          />
        </div>
      </OpsCard>
    </ElCol>
    <ElCol :xs="24" :lg="8">
      <OpsCard :title="t('ops.overview.securityGov.topKey')">
        <div v-loading="loading" class="sec-gov__list">
          <button
            v-for="row in topKey"
            :key="row.dim_value"
            type="button"
            class="sec-gov__row"
            @click="openCalls"
          >
            <span class="sec-gov__dim">{{ row.dim_value || "—" }}</span>
            <span class="sec-gov__fails">{{ row.fail_count }}</span>
          </button>
          <EmptyState
            v-if="!loading && !topKey.length"
            compact
            :description="t('ops.overview.securityGov.empty')"
          />
        </div>
      </OpsCard>
    </ElCol>
    <ElCol :xs="24" :lg="8">
      <OpsCard :title="t('ops.overview.securityGov.governance')">
        <OpsKpiRow>
          <ElCol :span="24">
            <StatCard
              :label="t('ops.overview.stats.ratelimit')"
              :value="rateLimitPerSec"
              :unit="t('ops.units.perSec')"
              :loading="loadingKpi"
              icon="ri:stop-circle-line"
              :interactive="true"
              @click="openCalls"
            />
          </ElCol>
        </OpsKpiRow>
        <p class="sec-gov__hint">{{ t("ops.overview.securityGov.hint") }}</p>
        <ElButton link type="primary" @click="openRuntime">
          {{ t("ops.overview.securityGov.openRuntime") }}
        </ElButton>
      </OpsCard>
    </ElCol>
  </ElRow>
</template>

<script setup lang="ts">
import { ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";
import OpsCard from "@/components/ops/OpsCard.vue";
import OpsKpiRow from "@/components/ops/OpsKpiRow.vue";
import StatCard from "@/components/ops/StatCard.vue";
import EmptyState from "@/components/common/empty-state/index.vue";
import { postOlapQuery } from "@/api/olap";
import { buildCallsRoute, buildRuntimeRoute } from "@/utils/ops/opsRoutes";
import { logger } from "@/utils/sys";

defineOptions({ name: "OpsOverviewSecurityGovSummary" });

interface TopRow {
  dim_value: string;
  fail_count: number;
  req_count: number;
}

const props = defineProps<{
  start: number;
  end: number;
  rateLimitPerSec: number | null | undefined;
  loadingKpi?: boolean;
}>();

const { t } = useI18n();
const router = useRouter();
const loading = ref(false);
const topIp = ref<TopRow[]>([]);
const topKey = ref<TopRow[]>([]);
let seq = 0;

function openCalls() {
  void router.push(
    buildCallsRoute({
      tab: "customer",
      start: props.start,
      end: props.end,
    }),
  );
}

function openRuntime() {
  void router.push(buildRuntimeRoute({ tab: "apps" }));
}

async function reload() {
  if (!props.start || !props.end || props.end <= props.start) return;
  const mine = ++seq;
  loading.value = true;
  try {
    const window = { start: props.start, end: props.end };
    const [ip, key] = await Promise.all([
      postOlapQuery({
        template: "security_top",
        params: { ...window, dim: "ip", limit: 5 },
      }),
      postOlapQuery({
        template: "security_top",
        params: { ...window, dim: "key", limit: 5 },
      }),
    ]);
    if (mine !== seq) return;
    topIp.value = ((ip as unknown as { rows?: TopRow[] }).rows) || [];
    topKey.value = ((key as unknown as { rows?: TopRow[] }).rows) || [];
  } catch (error) {
    logger.error("Failed to load security gov summary:", error);
    if (mine === seq) {
      topIp.value = [];
      topKey.value = [];
    }
  } finally {
    if (mine === seq) loading.value = false;
  }
}

watch(
  () => [props.start, props.end] as const,
  () => {
    void reload();
  },
  { immediate: true },
);

defineExpose({ reload });
</script>

<style scoped>
.sec-gov__list {
  min-height: 120px;
}
.sec-gov__row {
  display: flex;
  gap: 12px;
  align-items: center;
  justify-content: space-between;
  width: 100%;
  padding: 6px 0;
  text-align: left;
  cursor: pointer;
  background: transparent;
  border: 0;
  border-bottom: 1px solid var(--el-border-color-lighter);
  color: inherit;
}
.sec-gov__dim {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.sec-gov__fails {
  flex-shrink: 0;
  font-variant-numeric: tabular-nums;
  color: var(--el-color-danger);
}
.sec-gov__hint {
  margin: 8px 0 4px;
  font-size: 12px;
  color: var(--el-text-color-secondary);
}
</style>
