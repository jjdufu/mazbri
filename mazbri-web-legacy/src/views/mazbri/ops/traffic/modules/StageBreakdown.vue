<template>
  <div v-loading="loading" class="stage-breakdown">
    <div v-if="errorText" class="stage-breakdown__empty">{{ errorText }}</div>
    <div
      v-else-if="!loading && !model.summaryBar && model.subBars.length === 0"
      class="stage-breakdown__empty"
    >
      {{ t("ops.calls.stages.empty") }}
    </div>
    <template v-else>
      <div
        v-if="model.summaryBar"
        class="stage-breakdown__row stage-breakdown__row--summary"
      >
        <div class="stage-breakdown__meta">
          <span
            class="stage-breakdown__label stage-breakdown__label--summary"
            :title="t('ops.calls.stages.gatewaySummary')"
          >
            {{ t("ops.calls.stages.gatewaySummary") }}
          </span>
          <ElTag
            v-if="model.summaryBar.statusCode > 0"
            size="small"
            :type="model.summaryBar.error ? 'danger' : 'success'"
            class="stage-breakdown__tag"
          >
            {{ model.summaryBar.statusCode }}
          </ElTag>
          <ElTag
            v-if="model.summaryBar.errorCode"
            size="small"
            type="danger"
            effect="plain"
            class="stage-breakdown__tag"
          >
            {{ model.summaryBar.errorCode }}
          </ElTag>
        </div>
        <div class="stage-breakdown__track">
          <div
            class="stage-breakdown__bar is-summary"
            :class="{ 'is-error': model.summaryBar.error }"
            style="width: 100%"
          ></div>
        </div>
        <span class="stage-breakdown__value stage-breakdown__value--summary">
          {{ model.summaryBar.ms }} ms
        </span>
      </div>

      <div
        v-if="model.unsampledSubStages"
        class="stage-breakdown__unsampled"
      >
        {{ t("ops.calls.stages.unsampledHint") }}
      </div>

      <div
        v-for="bar in model.subBars"
        :key="bar.key"
        class="stage-breakdown__row"
      >
        <div class="stage-breakdown__meta">
          <span class="stage-breakdown__label" :title="bar.label">{{
            bar.label
          }}</span>
          <ElTag
            v-if="bar.statusCode > 0"
            size="small"
            :type="bar.error ? 'danger' : 'success'"
            class="stage-breakdown__tag"
          >
            {{ bar.statusCode }}
          </ElTag>
          <ElTag
            v-if="bar.errorCode"
            size="small"
            type="danger"
            effect="plain"
            class="stage-breakdown__tag"
          >
            {{ bar.errorCode }}
          </ElTag>
        </div>
        <div class="stage-breakdown__track">
          <div
            class="stage-breakdown__bar"
            :class="{ 'is-error': bar.error }"
            :style="{ width: `${bar.percent}%` }"
          ></div>
        </div>
        <span class="stage-breakdown__value">
          {{ bar.ms }} ms ({{ formatStageShare(bar.share) }})
        </span>
      </div>
    </template>

    <div v-if="traceId" class="stage-breakdown__footer">
      <span v-if="canDrill" class="stage-breakdown__trace-row">
        <ElLink type="primary" @click="emit('openTrace', traceId)">
          {{ t("ops.calls.stages.openTrace") }}
        </ElLink>
        <CopyableText
          :value="traceId"
          :label="t('ops.traces.columns.traceId')"
        />
      </span>
      <span
        v-else-if="isAdmin && expired"
        class="stage-breakdown__trace-row is-expired"
      >
        <ElTooltip :content="t('ops.traces.expiredTooltip')" placement="top">
          <span class="stage-breakdown__expired-wrap">
            <CopyableText
              :value="traceId"
              :label="t('ops.traces.columns.traceId')"
            />
          </span>
        </ElTooltip>
      </span>
      <span v-else class="stage-breakdown__trace-row">
        <CopyableText
          :value="traceId"
          :label="t('ops.traces.columns.traceId')"
        />
      </span>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import CopyableText from "@/components/common/copyable-text/index.vue";
import { fetchCallStages } from "@/api/call-monitor";
import { useOpsRole } from "@/composables/ops/useOpsRole";
import { canDrillToTrace, isTraceExpired } from "@/utils/ops/trace-link";
import type {
  CallStageAccessRow,
  CallStageEntry,
} from "@/types/api/call-monitor";
import {
  computeStageBreakdown,
  formatStageShare,
  type StageBar,
} from "./stage-breakdown";

export type { StageBar };

const props = withDefaults(
  defineProps<{
    requestId?: string;
    traceId?: string;
    timestamp?: string | number | null;
    totalDurationMs?: number;
    /** Pre-loaded stage rows; when set no request is made. */
    stages?: CallStageEntry[] | null;
    /** Optional pre-loaded call access rows for gateway duration fallback. */
    calls?: CallStageAccessRow[] | null;
  }>(),
  {
    requestId: "",
    traceId: "",
    timestamp: null,
    totalDurationMs: undefined,
    stages: null,
    calls: null,
  },
);

const emit = defineEmits<{ openTrace: [traceId: string] }>();

const { t } = useI18n();
const { isAdmin } = useOpsRole();
const loading = ref(false);
const errorText = ref("");
const loadedStages = ref<CallStageEntry[]>([]);
const loadedCalls = ref<CallStageAccessRow[]>([]);

const rows = computed(() => props.stages ?? loadedStages.value);
const callRows = computed(() => props.calls ?? loadedCalls.value);

const model = computed(() =>
  computeStageBreakdown(rows.value, {
    calls: callRows.value,
    totalDurationMs: props.totalDurationMs,
  }),
);

const expired = computed(() => isTraceExpired(props.timestamp));
const canDrill = computed(() =>
  canDrillToTrace(props.timestamp, isAdmin.value),
);

async function load() {
  if (props.stages || !props.requestId) return;
  loading.value = true;
  errorText.value = "";
  try {
    const res = await fetchCallStages({ request_id: props.requestId });
    loadedStages.value = res.stages;
    loadedCalls.value = res.calls;
  } catch {
    errorText.value = t("ops.calls.stages.loadFailed");
  } finally {
    loading.value = false;
  }
}

onMounted(load);
watch(() => props.requestId, load);
</script>

<style scoped>
.stage-breakdown {
  min-height: 48px;
  padding: 8px 16px;
}
.stage-breakdown__row {
  display: grid;
  grid-template-columns: minmax(220px, 280px) 1fr 120px;
  gap: 10px;
  align-items: center;
  margin-bottom: 6px;
  font-size: 12px;
}
.stage-breakdown__row--summary {
  padding-bottom: 6px;
  margin-bottom: 8px;
  border-bottom: 1px dashed var(--el-border-color-lighter);
}
.stage-breakdown__meta {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}
.stage-breakdown__label {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: var(--el-font-family-mono, monospace);
}
.stage-breakdown__label--summary {
  font-family: inherit;
  font-weight: 600;
  color: var(--el-text-color-primary);
}
.stage-breakdown__tag {
  flex-shrink: 0;
}
.stage-breakdown__track {
  height: 10px;
  background: var(--el-fill-color-light);
  border-radius: var(--art-form-control-radius-sm);
}
.stage-breakdown__bar {
  height: 100%;
  background: var(--el-color-primary);
  border-radius: var(--art-form-control-radius-sm);
}
.stage-breakdown__bar.is-summary {
  background: var(--el-color-info-light-3);
}
.stage-breakdown__bar.is-error {
  background: var(--el-color-danger);
}
.stage-breakdown__value {
  text-align: right;
  font-variant-numeric: tabular-nums;
}
.stage-breakdown__value--summary {
  font-weight: 600;
  color: var(--el-text-color-primary);
}
.stage-breakdown__unsampled {
  margin-bottom: 6px;
  color: var(--el-text-color-secondary);
  font-size: 12px;
}
.stage-breakdown__empty {
  color: var(--el-text-color-secondary);
  font-size: 12px;
}
.stage-breakdown__footer {
  margin-top: 6px;
}
.stage-breakdown__trace-row {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  max-width: 100%;
}
.stage-breakdown__trace-row.is-expired {
  color: var(--el-text-color-placeholder);
}
.stage-breakdown__expired-wrap {
  display: inline-flex;
  align-items: center;
  max-width: 100%;
}
</style>
