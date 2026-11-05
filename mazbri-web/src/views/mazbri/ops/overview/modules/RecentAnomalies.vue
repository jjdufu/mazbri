<template>
  <OpsCard class="recent-anomalies">
    <template #title>
      {{ t("ops.overview.recentAnomalies") }}
    </template>

    <EmptyState
      v-if="loaded && !loading && rows.length === 0"
      compact
      :image-size="56"
      :description="t('ops.overview.anomalyEmpty')"
    />
    <OpsTableWrap v-else>
      <ArtTable
        :data="rows"
        :loading="loading"
        :columns="columns"
        stripe
        class="recent-anomalies__table"
        @row-click="onRowClick"
      />
    </OpsTableWrap>

    <AdaptiveDrawer
      v-model="drawerOpen"
      :title="t('ops.overview.anomalyDetail')"
      destroy-on-close
      size="420px"
    >
      <ElDescriptions
        v-if="selected"
        :column="1"
        border
        size="small"
        class="recent-anomalies__desc"
      >
        <ElDescriptionsItem
          v-for="field in drawerFields"
          :key="field.prop"
          :label="field.label"
        >
          <pre
            v-if="field.prop === 'detail'"
            class="recent-anomalies__detail"
          >{{ anomalyFieldDisplay(selected, field.prop) }}</pre>
          <span v-else>{{ anomalyFieldDisplay(selected, field.prop) }}</span>
        </ElDescriptionsItem>
      </ElDescriptions>
    </AdaptiveDrawer>
  </OpsCard>
</template>

<script setup lang="ts">
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import { ElMessage } from "element-plus";
import AdaptiveDrawer from "@/components/common/AdaptiveDrawer.vue";
import OpsCard from "@/components/ops/OpsCard.vue";
import EmptyState from "@/components/common/empty-state/index.vue";
import OpsTableWrap from "@/components/ops/OpsTableWrap.vue";
import { listAnomalies, type AnomalyEvent } from "@/api/opsReliability";
import {
  anomalyFieldDisplay,
  buildAnomalyDrawerFields,
  buildAnomalyTableColumns,
  normalizeAnomalyEvents,
  takeAnomalyTop,
} from "@/utils/ops/anomaly";

defineOptions({ name: "OpsOverviewRecentAnomalies" });

const LIMIT = 10;

const { t } = useI18n();
const loading = ref(false);
const rows = ref<AnomalyEvent[]>([]);
const selected = ref<AnomalyEvent | null>(null);
const drawerOpen = ref(false);

const columns = computed(() => buildAnomalyTableColumns(t));
const drawerFields = computed(() => buildAnomalyDrawerFields(t));

function onRowClick(row: AnomalyEvent) {
  selected.value = row;
  drawerOpen.value = true;
}

const loaded = ref(false);
let seq = 0;

/** 按全局时间窗 [start, end) 取最近异常（容量预测不属于异常，后端已排除）。 */
async function reload(window: { start: number; end: number }) {
  const mine = ++seq;
  loading.value = true;
  try {
    const raw = await listAnomalies({ ...window, limit: LIMIT });
    if (mine !== seq) return;
    rows.value = takeAnomalyTop(normalizeAnomalyEvents(raw), LIMIT);
  } catch (error) {
    if (mine !== seq) return;
    rows.value = [];
    ElMessage.error(
      error instanceof Error
        ? error.message
        : t("ops.overview.anomalyLoadFailed"),
    );
  } finally {
    if (mine === seq) {
      loading.value = false;
      loaded.value = true;
    }
  }
}

defineExpose({ reload });
</script>

<style scoped>
.recent-anomalies__table {
  cursor: pointer;
}
.recent-anomalies__desc {
  width: 100%;
}
.recent-anomalies__detail {
  margin: 0;
  white-space: pre-wrap;
  word-break: break-word;
  font-family: var(--el-font-family);
  font-size: 12px;
  line-height: 1.45;
}
</style>
