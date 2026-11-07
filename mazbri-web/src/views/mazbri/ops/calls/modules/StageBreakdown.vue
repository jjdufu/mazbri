<template>
  <div v-loading="loading" class="stage-breakdown">
    <div v-if="errorText" class="stage-breakdown__empty">{{ errorText }}</div>
    <div v-else-if="!loading && bars.length === 0" class="stage-breakdown__empty">
      {{ t("ops.calls.stages.empty") }}
    </div>
    <template v-else>
      <div v-for="bar in bars" :key="bar.key" class="stage-breakdown__row">
        <div class="stage-breakdown__meta">
          <span class="stage-breakdown__label" :title="bar.label">{{ bar.label }}</span>
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
        <span class="stage-breakdown__value">{{ bar.ms }} ms ({{ bar.share }}%)</span>
      </div>
    </template>
    <div v-if="traceId" class="stage-breakdown__footer">
      <ElLink type="primary" @click="emit('openTrace', traceId)">
        {{ t("ops.calls.stages.openTrace") }} · {{ traceId }}
      </ElLink>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { fetchCallStages } from "@/api/call-monitor";
import type { CallStageEntry } from "@/types/api/call-monitor";

export interface StageBar {
  key: string;
  label: string;
  ms: number;
  percent: number;
  share: number;
  statusCode: number;
  errorCode: string;
  error: boolean;
}

const props = withDefaults(
  defineProps<{
    requestId?: string;
    traceId?: string;
    /** Pre-loaded stage rows; when set no request is made. */
    stages?: CallStageEntry[] | null;
  }>(),
  { requestId: "", traceId: "", stages: null },
);

const emit = defineEmits<{ openTrace: [traceId: string] }>();

const { t } = useI18n();
const loading = ref(false);
const errorText = ref("");
const loaded = ref<CallStageEntry[]>([]);

const rows = computed(() => props.stages ?? loaded.value);

const bars = computed<StageBar[]>(() => {
  const max = rows.value.reduce((acc, row) => Math.max(acc, row.stage_ms), 0);
  const total = rows.value.reduce((acc, row) => acc + Math.max(0, row.stage_ms), 0);
  return rows.value.map((row, index) => ({
    key: `${row.request_id}-${row.attempt_order}-${row.stage_name}-${index}`,
    label:
      row.attempt_order > 0
        ? `${row.service}/${row.stage_name} #${row.attempt_order}`
        : `${row.service}/${row.stage_name}`,
    ms: row.stage_ms,
    percent: max > 0 ? Math.max(3, Math.round((row.stage_ms / max) * 100)) : 3,
    share: total > 0 ? Math.round((Math.max(0, row.stage_ms) / total) * 100) : 0,
    statusCode: row.status_code ?? 0,
    errorCode: row.error_code ?? "",
    error: Boolean(row.error_code) || row.status_code >= 400,
  }));
});

async function load() {
  if (props.stages || !props.requestId) return;
  loading.value = true;
  errorText.value = "";
  try {
    const res = await fetchCallStages({ request_id: props.requestId });
    loaded.value = res.stages;
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
  grid-template-columns: minmax(220px, 280px) 1fr 110px;
  gap: 10px;
  align-items: center;
  margin-bottom: 6px;
  font-size: 12px;
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
.stage-breakdown__bar.is-error {
  background: var(--el-color-danger);
}
.stage-breakdown__value {
  text-align: right;
}
.stage-breakdown__empty {
  color: var(--el-text-color-secondary);
  font-size: 12px;
}
.stage-breakdown__footer {
  margin-top: 6px;
}
</style>
