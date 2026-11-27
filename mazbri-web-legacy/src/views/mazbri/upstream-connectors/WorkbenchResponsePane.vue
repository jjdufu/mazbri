<template>
  <div class="wb-res">
    <template v-if="run && !loading">
      <div class="wb-res__top">
        <WorkbenchResponseMeta
          :status="view.status"
          :latency-ms="run.latency_ms"
          :size-bytes="bodyBytes"
          :mode="run.mode"
          :probe-path="run.probe_path"
          :error-code="run.error_code"
          :retry-info="retryInfo"
          :body="displayBody"
          :download-name="downloadName"
        />

        <div v-if="hasTraceBar" class="wb-res__tracebar">
          <button
            v-if="requestId"
            type="button"
            class="wb-res__trace-chip"
            :title="$t('common.actions.copy')"
            @click="copyVal(requestId)"
          >
            <span class="wb-res__trace-lbl">{{
              $t("upstreamConnector.traceRequestId")
            }}</span>
            <code>{{ requestId }}</code>
            <ArtSvgIcon icon="ri:file-copy-line" />
          </button>

          <button
            v-if="upstreamRequestId"
            type="button"
            class="wb-res__trace-chip"
            :title="$t('common.actions.copy')"
            @click="copyVal(upstreamRequestId)"
          >
            <span class="wb-res__trace-lbl">{{
              $t("upstreamConnector.traceUpstreamId")
            }}</span>
            <code>{{ upstreamRequestId }}</code>
            <ArtSvgIcon icon="ri:file-copy-line" />
          </button>

          <button
            v-if="traceId"
            type="button"
            class="wb-res__trace-chip"
            :title="$t('common.actions.copy')"
            @click="copyVal(traceId)"
          >
            <span class="wb-res__trace-lbl">{{
              $t("upstreamConnector.traceIdLabel")
            }}</span>
            <code>{{ traceId }}</code>
            <ArtSvgIcon icon="ri:file-copy-line" />
          </button>
        </div>
      </div>
    </template>

    <div v-if="loading" class="wb-res__loading">
      <ArtSvgIcon icon="ri:refresh-line" class="wb-res__spin" />
      <p>{{ $t("upstreamConnector.requesting") }}</p>
    </div>

    <div v-else-if="!run" class="wb-res__idle">
      <ArtSvgIcon icon="ri:code-box-line" class="wb-res__idle-icon" />
      <p class="wb-res__idle-title">{{ idleTitle }}</p>
      <p class="wb-res__idle-hint">{{ $t("upstreamConnector.responseIdleHint") }}</p>
      <button
        type="button"
        class="wb-res__idle-link"
        @click="emit('open-shortcuts')"
      >
        {{ $t("upstreamConnector.shortcutsAll") }}
      </button>
    </div>

    <template v-else>
      <div class="wb-res__subnav" role="tablist">
        <button
          v-for="t in subTabs"
          :key="t.id"
          type="button"
          role="tab"
          class="wb-res__subtab"
          :class="{ 'is-active': activeTab === t.id }"
          @click="activeTab = t.id"
        >
          <span>{{ t.label }}</span>
          <span
            v-if="t.badge"
            class="wb-res__subbadge tabular-nums"
            :data-tone="t.tone || 'neutral'"
          >
            {{ t.badge }}
          </span>
        </button>
      </div>

      <div class="wb-res__content">
        <!-- 1. JSON / Tree View -->
        <div v-if="activeTab === 'json'" class="wb-res__fill">
          <p v-if="jsonTooLarge" class="wb-res__too-large">
            {{ $t("upstreamConnector.jsonTooLarge") }}
          </p>
          <WorkbenchJsonTree
            v-if="bodyKind === 'json' && parsedJson.ok && !jsonTooLarge"
            :value="parsedJson.value"
          />
          <WorkbenchCodeEditor
            v-else
            :model-value="displayBody"
            readonly
            fill
            :show-toolbar="true"
            :lang-label="rawLang"
            height="100%"
          />
        </div>

        <!-- 2. Raw Text (CodeMirror AST Highlighting) -->
        <div v-else-if="activeTab === 'raw'" class="wb-res__fill">
          <WorkbenchCodeEditor
            :model-value="displayBody"
            readonly
            fill
            :show-toolbar="true"
            :lang-label="rawLang"
            height="100%"
          />
        </div>

        <!-- 3. Response Headers -->
        <div v-else-if="activeTab === 'headers'" class="wb-res__scroll">
          <ElEmpty
            v-if="!headerEntries.length"
            :description="$t('common.feedback.noData')"
            :image-size="44"
          />
          <div v-else class="wb-res__htable">
            <div
              v-for="[k, v] in headerEntries"
              :key="k"
              class="wb-res__hrow"
              @click="copyVal(v)"
            >
              <span class="k">{{ k }}</span>
              <span class="v">{{ v }}</span>
            </div>
          </div>
        </div>

        <!-- 4. Assertions & Captures -->
        <div v-else-if="activeTab === 'assertions'" class="wb-res__scroll wb-res__stack">
          <section class="wb-res__sec">
            <header class="wb-res__sec-head">
              <span>{{ $t("upstreamConnector.assertions") }}</span>
              <span class="tabular-nums">
                {{ passedAssertCount }}/{{ asserts.length }}
              </span>
            </header>
            <div v-if="!asserts.length" class="wb-res__muted">
              {{ $t("upstreamConnector.noAssertions") }}
            </div>
            <div v-else class="wb-res__assert-list">
              <div
                v-for="(a, i) in asserts"
                :key="i"
                class="wb-res__assert-card"
                :class="a.ok ? 'ok' : 'fail'"
              >
                <div class="wb-res__assert-head">
                  <span class="wb-res__assert-badge">{{ a.ok ? "✓" : "✗" }}</span>
                  <span class="wb-res__assert-name">{{ a.name }}</span>
                  <span v-if="a.message" class="wb-res__assert-msg">
                    {{ a.message }}
                  </span>
                </div>
                <div
                  v-if="a.expected !== undefined || a.actual !== undefined"
                  class="wb-res__assert-diff"
                >
                  <div class="wb-res__diff-box">
                    <span class="wb-res__diff-lbl">{{
                      $t("upstreamConnector.assertExpected")
                    }}</span>
                    <code>{{ formatDiffVal(a.expected) }}</code>
                  </div>
                  <div class="wb-res__diff-box">
                    <span class="wb-res__diff-lbl">{{
                      $t("upstreamConnector.assertActual")
                    }}</span>
                    <code>{{ formatDiffVal(a.actual) }}</code>
                  </div>
                </div>
              </div>
            </div>
          </section>

          <section class="wb-res__sec">
            <header class="wb-res__sec-head">
              <span>{{ $t("upstreamConnector.capturesTab") }}</span>
              <span class="tabular-nums">{{ runCapturedEntries.length }}</span>
            </header>
            <div v-if="!runCapturedEntries.length" class="wb-res__muted">
              {{ $t("upstreamConnector.capturedEmpty") }}
            </div>
            <div v-else class="wb-res__htable">
              <div
                v-for="[k, v] in runCapturedEntries"
                :key="k"
                class="wb-res__hrow"
                @click="copyVal(`{{captures.${k}}}`)"
              >
                <span class="k is-capture">{{ `\{\{captures.${k}\}\}` }}</span>
                <span class="v">{{ v }}</span>
              </div>
            </div>
          </section>
        </div>

        <!-- 5. Pipeline & Stage Waterfall -->
        <div v-else-if="activeTab === 'pipeline'" class="wb-res__scroll wb-res__stack">
          <WorkbenchPipeline :run="run" :inbound="inbound" />

          <section v-if="stages.length" class="wb-res__sec">
            <header class="wb-res__sec-head">
              <span>{{ $t("upstreamConnector.timelineTab") }}</span>
              <span class="tabular-nums">{{ run.latency_ms ?? 0 }} ms</span>
            </header>
            <div class="wb-res__waterfall">
              <button
                v-for="s in stages"
                :key="s.name"
                type="button"
                class="wb-res__wrow"
                :class="{ open: openStage === s.name, fail: !s.ok }"
                @click="openStage = openStage === s.name ? '' : s.name"
              >
                <span class="wb-res__wname">{{ stageLabel(s.name) }}</span>
                <div class="wb-res__wtrack">
                  <span
                    class="wb-res__wbar"
                    :class="s.ok ? 'ok' : 'fail'"
                    :style="{ width: `${stageWidthPct(s.latency_ms)}%` }"
                  />
                </div>
                <span class="wb-res__wms tabular-nums">{{ s.latency_ms }} ms</span>
              </button>
            </div>
            <pre v-if="openStageDetail" class="wb-res__stage-pre">{{
              openStageDetail
            }}</pre>
          </section>
        </div>
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import type { ConnectorRun, StageResult } from "@/api/upstream-connectors";
import { $t } from "@/locales";
import { copyWithFeedback } from "@/utils/ui/copy-feedback";
import {
  byteSizeOf,
  detectResponseBodyKind,
  extractRunResponse,
  jsonTreeFits,
  prettyJson,
  tryParseJsonValue,
} from "./workbenchHelpers";
import type { ResponseSubTabId } from "./useWorkbenchStore";
import WorkbenchCodeEditor from "./WorkbenchCodeEditor.vue";
import WorkbenchPipeline from "./WorkbenchPipeline.vue";
import WorkbenchJsonTree from "./hopp/WorkbenchJsonTree.vue";
import WorkbenchResponseMeta from "./hopp/WorkbenchResponseMeta.vue";
import { extractRetryInfo } from "./authInvalidation";

const activeTab = defineModel<ResponseSubTabId>("tab", { default: "json" });

const props = defineProps<{
  run: ConnectorRun | null;
  runs: ConnectorRun[];
  loading?: boolean;
  adapter?: string;
  sendBlockReason?: string;
  inbound?: { method?: string; path?: string; body?: unknown };
}>();

const emit = defineEmits<{
  "select-run": [ConnectorRun];
  "open-shortcuts": [];
}>();

const openStage = ref("");

const idleTitle = computed(() => {
  if (props.sendBlockReason === "url") {
    return $t("upstreamConnector.urlMissingSend");
  }
  return $t("upstreamConnector.responseIdle");
});

const view = computed(() => extractRunResponse(props.run));

const requestId = computed(
  () => props.run?.response_preview?.request_id?.trim() || "",
);
const upstreamRequestId = computed(
  () => props.run?.response_preview?.upstream_request_id?.trim() || "",
);
const traceId = computed(
  () => props.run?.response_preview?.trace_id?.trim() || "",
);
const hasTraceBar = computed(() =>
  Boolean(requestId.value || upstreamRequestId.value || traceId.value),
);

const engineErrorText = computed(() => {
  const run = props.run;
  if (!run) return "";
  if (run.error_code === "ASSERTION_FAILED" && view.value.status != null) {
    return "";
  }
  const msg = (run.error_message || "").trim();
  const code = (run.error_code || "").trim();
  if (msg && code && !msg.includes(code)) return `${code}: ${msg}`;
  return msg || code;
});

const rawBody = computed(() => {
  const body = view.value.body || "";
  if (body.trim()) return body;
  return engineErrorText.value;
});

const responseContentType = computed(() => {
  const hdrs = view.value.headers || {};
  for (const [k, v] of Object.entries(hdrs)) {
    if (k.toLowerCase() === "content-type") return v;
  }
  return "";
});

const bodyKind = computed(() =>
  detectResponseBodyKind(
    rawBody.value,
    props.adapter,
    responseContentType.value,
  ),
);

const parsedJson = computed(() => tryParseJsonValue(rawBody.value));
const jsonTooLarge = computed(
  () =>
    bodyKind.value === "json" &&
    parsedJson.value.ok &&
    !jsonTreeFits(parsedJson.value.value),
);

const displayBody = computed(() =>
  bodyKind.value === "json" ? prettyJson(rawBody.value) : rawBody.value,
);

const rawLang = computed(() => {
  if (bodyKind.value === "xml") return "XML";
  if (bodyKind.value === "json") return "JSON";
  return "TEXT";
});

const bodyBytes = computed(() => byteSizeOf(rawBody.value));
const downloadName = computed(() => {
  if (bodyKind.value === "xml") return "response.xml";
  if (bodyKind.value === "json") return "response.json";
  return "response.txt";
});

const headerEntries = computed(() => Object.entries(view.value.headers || {}));
const stages = computed<StageResult[]>(() =>
  Array.isArray(props.run?.stages) ? props.run!.stages : [],
);
const asserts = computed(() =>
  Array.isArray(props.run?.assertions) ? props.run!.assertions : [],
);
const passedAssertCount = computed(
  () => asserts.value.filter((a) => a.ok).length,
);
const runCapturedEntries = computed(() =>
  Object.entries(props.run?.response_preview?.captured || {}),
);

const subTabs = computed(() => {
  const assertTotal = asserts.value.length + runCapturedEntries.value.length;
  const assertFailed = asserts.value.some((a) => !a.ok);
  return [
    {
      id: "json" as ResponseSubTabId,
      label: $t("upstreamConnector.resJsonTreeTab"),
      badge: "",
      tone: "neutral",
    },
    {
      id: "raw" as ResponseSubTabId,
      label: $t("upstreamConnector.resRawTab"),
      badge: "",
      tone: "neutral",
    },
    {
      id: "headers" as ResponseSubTabId,
      label: $t("upstreamConnector.resHeadersTab"),
      badge: headerEntries.value.length
        ? String(headerEntries.value.length)
        : "",
      tone: "neutral",
    },
    {
      id: "assertions" as ResponseSubTabId,
      label: $t("upstreamConnector.resAssertCaptureTab"),
      badge: assertTotal
        ? asserts.value.length
          ? `${passedAssertCount.value}/${asserts.value.length}`
          : String(runCapturedEntries.value.length)
        : "",
      tone: assertFailed ? "danger" : asserts.value.length ? "success" : "neutral",
    },
    {
      id: "pipeline" as ResponseSubTabId,
      label: $t("upstreamConnector.resPipelineTab"),
      badge: stages.value.length ? String(stages.value.length) : "",
      tone: "neutral",
    },
  ];
});

const maxStageMs = computed(() => {
  const max = Math.max(1, ...stages.value.map((s) => s.latency_ms || 0));
  return max;
});

function stageWidthPct(ms: number): number {
  if (!ms || ms <= 0) return 4;
  return Math.min(100, Math.max(6, Math.round((ms / maxStageMs.value) * 100)));
}

function stageLabel(name: string) {
  const key = `upstreamConnector.stage.${name}`;
  const t = $t(key);
  return t === key ? name : t;
}

const retryInfo = computed(() => extractRetryInfo(stages.value));

const openStageDetail = computed(() => {
  const s = stages.value.find((x) => x.name === openStage.value);
  if (!s) return "";
  return prettyJson({
    ok: s.ok,
    latency_ms: s.latency_ms,
    error: s.error,
    request: s.request_summary,
    response: s.response_summary,
  });
});

function formatDiffVal(v: unknown): string {
  if (v === undefined) return "—";
  if (v === null) return "null";
  if (typeof v === "string") return v;
  try {
    return JSON.stringify(v);
  } catch {
    return String(v);
  }
}

async function copyVal(val: string) {
  if (!val) return;
  await copyWithFeedback(val);
}
</script>

<style scoped lang="scss">
.wb-res {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
  background: var(--wb-surface);
}

.wb-res__top {
  flex-shrink: 0;
  border-bottom: 1px solid var(--wb-divider);

  :deep(.wb-res-meta) {
    padding: 6px 12px;
  }
}

.wb-res__tracebar {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: center;
  padding: 4px 12px 6px;
  border-top: 1px solid var(--wb-divider-light);
}

.wb-res__trace-chip {
  display: inline-flex;
  gap: 5px;
  align-items: center;
  height: 22px;
  padding: 0 7px;
  font-size: 11px;
  color: var(--wb-text-muted);
  cursor: pointer;
  background: var(--wb-surface-subtle);
  border: 1px solid var(--wb-divider);
  border-radius: 3px;

  code {
    max-width: 200px;
    overflow: hidden;
    text-overflow: ellipsis;
    font-family: var(--wb-font-mono);
    font-size: 10.5px;
    color: var(--wb-text);
    white-space: nowrap;
  }

  &:hover {
    background: var(--wb-surface-hover);
  }
}

.wb-res__trace-lbl {
  color: var(--wb-text-faint);
}

.wb-res__loading,
.wb-res__idle {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: 4px;
  align-items: center;
  justify-content: center;
  padding: 10px 16px;
  color: var(--wb-text-muted);
  text-align: center;
}

.wb-res__spin {
  font-size: 20px;
  color: var(--wb-primary);
  animation: wb-res-spin 0.8s linear infinite;
}

@keyframes wb-res-spin {
  to {
    transform: rotate(360deg);
  }
}

.wb-res__idle-icon {
  font-size: 22px;
  color: var(--wb-text-faint);
}

.wb-res__idle-title {
  margin: 0;
  font-size: 13px;
  font-weight: 500;
  color: var(--wb-text);
}

.wb-res__idle-hint {
  margin: 0;
  font-size: 11.5px;
  color: var(--wb-text-faint);
}

.wb-res__idle-link {
  padding: 4px 8px;
  margin-top: 4px;
  font-size: 11.5px;
  color: var(--wb-text-muted);
  cursor: pointer;
  background: var(--wb-surface-subtle);
  border: 1px solid var(--wb-divider);
  border-radius: var(--wb-radius);

  &:hover {
    color: var(--wb-text);
    background: var(--wb-surface-hover);
  }
}

.wb-res__subnav {
  display: flex;
  flex-shrink: 0;
  gap: 2px;
  align-items: stretch;
  height: 34px;
  padding: 0 8px;
  overflow-x: auto;
  background: var(--wb-surface-subtle);
  border-bottom: 1px solid var(--wb-divider);
}

.wb-res__subtab {
  position: relative;
  display: inline-flex;
  flex-shrink: 0;
  gap: 6px;
  align-items: center;
  padding: 0 10px;
  font-size: 12px;
  font-weight: 500;
  color: var(--wb-text-muted);
  cursor: pointer;
  background: transparent;
  border: none;

  &:hover {
    color: var(--wb-text);
  }

  &.is-active {
    font-weight: 600;
    color: var(--wb-primary);

    &::after {
      position: absolute;
      right: 6px;
      bottom: 0;
      left: 6px;
      height: 2px;
      content: "";
      background: var(--wb-primary);
    }
  }
}

.wb-res__subbadge {
  padding: 0 5px;
  font-family: var(--wb-font-mono);
  font-size: 10px;
  color: var(--wb-text-muted);
  background: var(--wb-surface);
  border: 1px solid var(--wb-divider);
  border-radius: 999px;

  &[data-tone="success"] {
    color: var(--wb-success);
  }

  &[data-tone="danger"] {
    color: var(--wb-danger);
  }
}

.wb-res__content {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
}

.wb-res__fill {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-height: 0;
  padding: 8px;
}

.wb-res__scroll {
  flex: 1;
  min-height: 0;
  padding: 10px 12px;
  overflow: auto;
}

.wb-res__stack {
  display: flex;
  flex-direction: column;
  gap: 14px;

  > * {
    flex-shrink: 0;
  }
}

.wb-res__too-large {
  margin: 0 0 6px;
  font-size: 11.5px;
  color: var(--wb-text-muted);
}

.wb-res__htable {
  border-top: 1px solid var(--wb-divider);
  border-left: 1px solid var(--wb-divider);
}

.wb-res__hrow {
  display: grid;
  grid-template-columns: minmax(150px, 0.7fr) minmax(0, 1.5fr);
  font-size: 12px;
  cursor: pointer;

  &:hover {
    background: var(--wb-surface-hover);
  }

  .k,
  .v {
    padding: 6px 10px;
    font-family: var(--wb-font-mono);
    word-break: break-all;
    border-right: 1px solid var(--wb-divider);
    border-bottom: 1px solid var(--wb-divider);
  }

  .k {
    color: var(--wb-text-muted);

    &.is-capture {
      color: var(--wb-success);
    }
  }

  .v {
    color: var(--wb-text);
  }
}

.wb-res__sec {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.wb-res__sec-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 12px;
  font-weight: 600;
  color: var(--wb-text);
}

.wb-res__muted {
  font-size: 12px;
  color: var(--wb-text-faint);
}

.wb-res__assert-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.wb-res__assert-card {
  padding: 8px 10px;
  border: 1px solid var(--wb-divider);
  border-left-width: 3px;
  border-radius: var(--wb-radius);

  &.ok {
    border-left-color: var(--wb-success);
  }

  &.fail {
    border-left-color: var(--wb-danger);
  }
}

.wb-res__assert-head {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: center;
  font-size: 12px;
}

.wb-res__assert-badge {
  font-weight: 700;
}

.wb-res__assert-card.ok .wb-res__assert-badge {
  color: var(--wb-success);
}

.wb-res__assert-card.fail .wb-res__assert-badge {
  color: var(--wb-danger);
}

.wb-res__assert-name {
  font-family: var(--wb-font-mono);
  font-weight: 600;
  color: var(--wb-text);
}

.wb-res__assert-msg {
  color: var(--wb-text-muted);
}

.wb-res__assert-diff {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 8px;
  margin-top: 6px;
}

.wb-res__diff-box {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 6px 8px;
  background: var(--wb-surface-subtle);
  border: 1px solid var(--wb-divider);
  border-radius: 3px;

  code {
    font-family: var(--wb-font-mono);
    font-size: 11.5px;
    color: var(--wb-text);
    word-break: break-all;
  }
}

.wb-res__diff-lbl {
  font-size: 10.5px;
  color: var(--wb-text-faint);
}

.wb-res__waterfall {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.wb-res__wrow {
  display: grid;
  grid-template-columns: 120px 1fr 68px;
  gap: 8px;
  align-items: center;
  padding: 5px 8px;
  font-size: 11.5px;
  text-align: left;
  cursor: pointer;
  background: var(--wb-surface-subtle);
  border: 1px solid var(--wb-divider);
  border-radius: 3px;

  &.open {
    border-color: var(--wb-text-muted);
  }
}

.wb-res__wname {
  overflow: hidden;
  text-overflow: ellipsis;
  color: var(--wb-text);
  white-space: nowrap;
}

.wb-res__wtrack {
  height: 6px;
  overflow: hidden;
  background: var(--wb-divider-light);
  border-radius: 999px;
}

.wb-res__wbar {
  display: block;
  height: 100%;
  border-radius: 999px;

  &.ok {
    background: var(--wb-primary);
  }

  &.fail {
    background: var(--wb-danger);
  }
}

.wb-res__wms {
  font-family: var(--wb-font-mono);
  color: var(--wb-text-muted);
  text-align: right;
}

.wb-res__stage-pre {
  max-height: 220px;
  padding: 8px;
  margin: 0;
  overflow: auto;
  font-family: var(--wb-font-mono);
  font-size: 11px;
  color: var(--wb-text);
  white-space: pre-wrap;
  word-break: break-all;
  background: var(--wb-surface-subtle);
  border: 1px solid var(--wb-divider);
  border-radius: var(--wb-radius);
}
</style>
