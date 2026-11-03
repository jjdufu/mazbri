<template>
  <OpsCard class="infra-health-strip">
    <template #title>
      {{ t("ops.overview.infraStrip.title") }}
    </template>
    <template #extra>
      <ElButton link type="primary" @click="openInfra()">
        {{ t("ops.overview.infraStrip.open") }}
      </ElButton>
    </template>

    <div v-loading="loading" class="infra-health-strip__row">
      <button
        v-for="item in readings"
        :key="item.id"
        type="button"
        class="infra-health-strip__item"
        @click="openInfra(item.tab)"
      >
        <span
          class="infra-health-strip__dot"
          :data-status="item.status"
        ></span>
        <span class="infra-health-strip__label">{{ t(item.labelKey) }}</span>
        <span class="infra-health-strip__value">{{
          formatInfraStripValue(item.id, item.value)
        }}</span>
      </button>
    </div>
  </OpsCard>
</template>

<script setup lang="ts">
import { ref } from "vue";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";
import { ElMessage } from "element-plus";
import OpsCard from "@/components/ops/OpsCard.vue";
import { fetchMetricTimeseries } from "@/api/monitoring";
import {
  INFRA_STRIP_ITEMS,
  INFRA_STRIP_WINDOW_SECONDS,
  buildInfraStripReading,
  formatInfraStripValue,
  type InfraStripReading,
} from "@/utils/ops/infra-health";
import { buildInfraRoute, type InfraTabId } from "@/utils/ops/opsRoutes";

defineOptions({ name: "OpsOverviewInfraHealthStrip" });

const { t } = useI18n();
const router = useRouter();
const loading = ref(false);
const readings = ref<InfraStripReading[]>(
  INFRA_STRIP_ITEMS.map((def) => buildInfraStripReading(def, null)),
);

function openInfra(tab?: InfraTabId) {
  void router.push(buildInfraRoute(tab ? { tab } : {}));
}

async function reload() {
  loading.value = true;
  const end = Math.floor(Date.now() / 1000);
  const start = end - INFRA_STRIP_WINDOW_SECONDS;
  try {
    const results = await Promise.all(
      INFRA_STRIP_ITEMS.map((def) =>
        fetchMetricTimeseries({
          metric: def.metric,
          start,
          end,
        }).catch(() => null),
      ),
    );
    readings.value = INFRA_STRIP_ITEMS.map((def, index) =>
      buildInfraStripReading(def, results[index]),
    );
  } catch (error) {
    ElMessage.error(
      error instanceof Error
        ? error.message
        : t("ops.overview.infraStrip.loadFailed"),
    );
  } finally {
    loading.value = false;
  }
}

defineExpose({ reload });
</script>

<style scoped>
.infra-health-strip__row {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  min-height: 40px;
}
.infra-health-strip__item {
  display: inline-flex;
  gap: 6px;
  align-items: center;
  padding: 6px 10px;
  border: 1px solid var(--el-border-color);
  border-radius: 999px;
  background: transparent;
  cursor: pointer;
}
.infra-health-strip__dot {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  background: #909399;
}
.infra-health-strip__dot[data-status="green"] {
  background: #67c23a;
}
.infra-health-strip__dot[data-status="yellow"] {
  background: #e6a23c;
}
.infra-health-strip__dot[data-status="red"] {
  background: #f56c6c;
}
.infra-health-strip__label {
  font-size: 12px;
}
.infra-health-strip__value {
  color: var(--el-text-color-secondary);
  font-size: 12px;
}
</style>
