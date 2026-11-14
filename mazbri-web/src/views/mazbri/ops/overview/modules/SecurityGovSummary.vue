<template>
  <ElRow :gutter="16" class="ops-equal-row">
    <ElCol :xs="24" :lg="12">
      <OpsCard :title="t('ops.overview.securityGov.topIp')" class="ops-main-panel">
        <template #extra>
          <ElButton link type="primary" @click="openCalls">
            {{ t("ops.overview.securityGov.openCalls") }}
          </ElButton>
        </template>
        <div v-loading="loading" class="sec-gov__list">
          <button
            v-for="(row, idx) in topIp"
            :key="row.dim_value"
            type="button"
            class="sec-gov__row"
            @click="openCalls"
          >
            <span class="sec-gov__left">
              <span class="sec-gov__rank" :data-top="idx < 3">{{ idx + 1 }}</span>
              <span class="sec-gov__dim">{{ row.dim_value || EMPTY_PLACEHOLDER }}</span>
            </span>
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
    <ElCol :xs="24" :lg="12">
      <OpsCard :title="t('ops.overview.securityGov.topKey')" class="ops-main-panel">
        <template #extra>
          <ElButton link type="primary" @click="openCalls">
            {{ t("ops.overview.securityGov.openCalls") }}
          </ElButton>
        </template>
        <div v-loading="loading" class="sec-gov__list">
          <button
            v-for="(row, idx) in topKey"
            :key="row.dim_value"
            type="button"
            class="sec-gov__row"
            @click="openCalls"
          >
            <span class="sec-gov__left">
              <span class="sec-gov__rank" :data-top="idx < 3">{{ idx + 1 }}</span>
              <span class="sec-gov__dim">{{ row.dim_value || EMPTY_PLACEHOLDER }}</span>
            </span>
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
  </ElRow>
</template>

<script setup lang="ts">
import { ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";
import OpsCard from "@/components/ops/OpsCard.vue";
import EmptyState from "@/components/common/empty-state/index.vue";
import { postOlapQuery } from "@/api/olap";
import { EMPTY_PLACEHOLDER } from "@/utils/format";
import { buildCallsRoute } from "@/utils/ops/opsRoutes";
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
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-height: 120px;
}
.sec-gov__row {
  display: flex;
  gap: 12px;
  align-items: center;
  justify-content: space-between;
  width: 100%;
  padding: 8px 10px;
  text-align: left;
  cursor: pointer;
  background: transparent;
  border: 0;
  border-radius: var(--art-form-control-radius);
  border-bottom: 1px solid var(--el-border-color-lighter);
  color: inherit;
  transition: background-color 0.18s ease;
}
.sec-gov__row:last-child {
  border-bottom: 0;
}
.sec-gov__row:hover {
  background: var(--el-fill-color-light);
}
.sec-gov__left {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}
.sec-gov__rank {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  border-radius: var(--art-form-control-radius-sm);
  font-size: 11px;
  font-weight: 600;
  color: var(--el-text-color-secondary);
  background: var(--el-fill-color);
  flex-shrink: 0;
}
.sec-gov__rank[data-top="true"] {
  color: var(--el-color-danger);
  background: var(--el-color-danger-light-9);
}
.sec-gov__dim {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: var(--el-font-family-monospace, monospace);
  font-size: 13px;
}
.sec-gov__fails {
  flex-shrink: 0;
  padding: 2px 8px;
  border-radius: var(--art-form-control-radius-sm);
  font-size: 12px;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
  color: var(--el-color-danger);
  background: var(--el-color-danger-light-9);
}
</style>
