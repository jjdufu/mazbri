<template>
  <div
    class="wb-res-meta"
    :class="[statusGroupClass(status), { 'is-engine-err': isEngineErr }]"
  >
    <span class="wb-res-meta__item">
      <span class="wb-res-meta__label">{{ $t("upstreamConnector.response.status") }}:</span>
      <span class="wb-res-meta__value">{{ statusText }}</span>
      <span v-if="errorCode" class="wb-res-meta__err">{{ errorCode }}</span>
    </span>
    <span class="wb-res-meta__item">
      <span class="wb-res-meta__label">{{ timeLabel }}:</span>
      <span class="wb-res-meta__value">{{ timeText }}</span>
    </span>
    <span class="wb-res-meta__item">
      <span class="wb-res-meta__label">{{ $t("upstreamConnector.response.size") }}:</span>
      <span class="wb-res-meta__value">{{ sizeText }}</span>
    </span>
    <span
      v-if="retryInfo"
      class="wb-res-meta__item"
      :title="`${$t('upstreamConnector.probeRetryReasons')}: ${retryInfo.reasons.join(', ')}`"
    >
      <span class="wb-res-meta__label">{{ $t("upstreamConnector.probeInternalRetries") }}:</span>
      <span class="wb-res-meta__value">{{ retryInfo.count }}</span>
      <span class="wb-res-meta__err">{{ retryInfo.reasons.join(", ") }}</span>
    </span>
    <span v-if="mode" class="wb-res-meta__mode">{{ mode }}</span>
    <span class="wb-res-meta__tools">
      <button
        type="button"
        class="wb-hopp-icon"
        :title="$t('common.actions.copy')"
        :disabled="!body"
        @click="copy"
      >
        <ArtSvgIcon icon="ri:file-copy-line" />
      </button>
      <button
        type="button"
        class="wb-hopp-icon"
        :title="$t('upstreamConnector.downloadBody')"
        :disabled="!body"
        @click="download"
      >
        <ArtSvgIcon icon="ri:download-2-line" />
      </button>
    </span>
  </div>
</template>

<script setup lang="ts">
import { copyWithFeedback } from "@/utils/ui/copy-feedback";
import { $t } from "@/locales";
import { BYPASS_MGMT_PROBE_PATH, formatBytes } from "../workbenchHelpers";
import { statusGroupClass, statusPhrase } from "./statusPhrases";
import type { RetryInfo } from "../authInvalidation";

const props = withDefaults(
  defineProps<{
    status?: number | null;
    latencyMs?: number | null;
    sizeBytes?: number;
    mode?: string;
    /** 试运行拓扑；bypass 管理探针时耗时为纯上游耗时。 */
    probePath?: string | null;
    errorCode?: string | null;
    body?: string;
    downloadName?: string;
    /** 引擎内部重试（认证失效重试 / WS 陈旧连接重试），无重试为 null。 */
    retryInfo?: RetryInfo | null;
  }>(),
  {
    status: null,
    latencyMs: null,
    sizeBytes: 0,
    mode: "",
    probePath: null,
    errorCode: "",
    body: "",
    downloadName: "response.txt",
    retryInfo: null,
  },
);

const isEngineErr = computed(
  () =>
    Boolean(props.errorCode) &&
    props.errorCode !== "ASSERTION_FAILED" &&
    props.status == null,
);

const statusText = computed(() => {
  if (props.status == null) return "—";
  const phrase = statusPhrase(props.status);
  return phrase ? `${props.status} • ${phrase}` : String(props.status);
});

const timeLabel = computed(() =>
  props.probePath === BYPASS_MGMT_PROBE_PATH
    ? $t("upstreamConnector.response.upstreamTime")
    : $t("upstreamConnector.response.time"),
);

const timeText = computed(() =>
  props.latencyMs == null ? "—" : `${props.latencyMs} ms`,
);

const sizeText = computed(() => formatBytes(props.sizeBytes ?? 0));

async function copy() {
  await copyWithFeedback(props.body || "");
}

function download() {
  const blob = new Blob([props.body || ""], { type: "application/octet-stream" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = props.downloadName || "response.txt";
  a.click();
  URL.revokeObjectURL(url);
}
</script>

<style scoped lang="scss">
.wb-res-meta {
  display: flex;
  align-items: center;
  gap: 16px;
  font-size: 10px;
  font-weight: 600;
  padding: 16px;
  white-space: nowrap;
  overflow-x: auto;
  color: var(--wb-status-other, #64748b);
  flex-shrink: 0;
  &.is-1xx {
    color: var(--wb-status-1xx);
  }
  &.is-2xx {
    color: var(--wb-status-2xx);
  }
  &.is-3xx {
    color: var(--wb-status-3xx);
  }
  &.is-4xx {
    color: var(--wb-status-4xx);
  }
  &.is-5xx {
    color: var(--wb-status-5xx);
  }
  &.is-other {
    color: var(--wb-status-other);
  }
  &.is-engine-err {
    color: var(--wb-danger, var(--el-color-danger));
  }
}
.wb-res-meta__item {
  display: inline-flex;
  align-items: baseline;
  gap: 6px;
}
.wb-res-meta__label {
  color: var(--wb-text-muted, var(--el-text-color-secondary));
  font-weight: 400;
}
.wb-res-meta__err {
  color: var(--wb-danger, var(--el-color-danger));
  font-weight: 600;
}
.wb-res-meta__mode {
  margin-left: auto;
  font-size: 10px;
  font-weight: 600;
  padding: 2px 8px;
  border: 1px solid var(--wb-divider, var(--el-border-color));
  border-radius: var(--wb-radius, var(--art-form-control-radius));
  color: var(--wb-text-muted, var(--el-text-color-secondary));
}
.wb-res-meta__tools {
  display: inline-flex;
  align-items: center;
  gap: 2px;
  flex-shrink: 0;
}
.wb-res-meta__tools .wb-hopp-icon:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
.wb-res-meta:not(:has(.wb-res-meta__mode)) .wb-res-meta__tools {
  margin-left: auto;
}
</style>
