<template>
  <div class="wb-res">
    <WorkbenchResponseMeta
      v-if="run && !loading"
      :status="view.status"
      :latency-ms="run.latency_ms"
      :size-bytes="bodyBytes"
      :mode="run.mode"
      :error-code="run.error_code"
      :body="displayBody"
      :download-name="downloadName"
    />
    <div v-else-if="!loading" class="wb-res__empty-title">
      {{ $t("upstreamConnector.trace") }}
    </div>

    <div v-if="loading" class="wb-res__loading">
      <ArtSvgIcon icon="ri:refresh-line" class="wb-res__spin" />
      <p>{{ $t("upstreamConnector.requesting") }}</p>
    </div>
    <div v-else-if="!run" class="wb-res__idle" :class="{ compact: runs.length }">
      <ArtSvgIcon icon="ri:send-plane-line" class="wb-res__idle-icon" />
      <p>{{ idleTitle }}</p>
      <p class="hint">{{ $t("upstreamConnector.responseIdleHint") }}</p>
      <button type="button" class="wb-res__idle-link" @click="emit('open-shortcuts')">
        {{ $t("upstreamConnector.shortcutsAll") }}
      </button>
    </div>

    <template v-else>
      <ElTabs v-model="tab" class="wb-res__tabs">
        <ElTabPane :label="$t('upstreamConnector.bodyTab')" name="body">
          <div class="wb-res__body">
            <div class="wb-res__body-bar">
              <button
                v-if="bodyKind === 'json'"
                type="button"
                class="wb-hopp-icon"
                :class="{ 'is-on': showTree }"
                :title="$t('upstreamConnector.jsonTree')"
                @click="setView('tree')"
              >
                <ArtSvgIcon icon="ri:node-tree" />
              </button>
              <button
                type="button"
                class="wb-hopp-icon"
                :class="{ 'is-on': !showTree }"
                :title="$t('upstreamConnector.rawBody')"
                @click="setView('raw')"
              >
                <ArtSvgIcon icon="ri:code-s-slash-line" />
              </button>
            </div>
            <p v-if="jsonTooLarge" class="wb-res__too-large">
              {{ $t("upstreamConnector.jsonTooLarge") }}
            </p>
            <WorkbenchJsonTree v-if="showTree && parsedJson.ok" :value="parsedJson.value" />
            <WorkbenchCodeEditor
              v-else
              :model-value="rawText"
              readonly
              fill
              :show-toolbar="false"
              :lang-label="rawLang"
              height="100%"
            />
          </div>
        </ElTabPane>
        <ElTabPane :label="$t('upstreamConnector.responseHeadersTab')" name="headers">
          <ElEmpty
            v-if="!headerEntries.length"
            :description="$t('common.feedback.noData')"
            :image-size="48"
          />
          <div v-else class="wb-res__headers">
            <div v-for="[k, v] in headerEntries" :key="k" class="wb-res__hrow">
              <span class="k">{{ k }}</span>
              <span class="v">{{ v }}</span>
            </div>
          </div>
        </ElTabPane>
        <ElTabPane :label="$t('upstreamConnector.timelineTab')" name="timeline">
          <div class="wb-res__stages">
            <button
              v-for="s in stages"
              :key="s.name"
              type="button"
              class="wb-res__stage"
              :class="{ ok: s.ok, fail: !s.ok, open: openStage === s.name }"
              @click="openStage = openStage === s.name ? '' : s.name"
            >
              <span class="name">{{ stageLabel(s.name) }}</span>
              <span class="ms">{{ s.latency_ms }}ms</span>
            </button>
          </div>
          <pre v-if="openStageDetail" class="wb-res__stage-pre">{{ openStageDetail }}</pre>
        </ElTabPane>
        <ElTabPane :label="$t('upstreamConnector.assertions')" name="tests">
          <div v-if="asserts.length" class="wb-res__asserts">
            <div
              v-for="(a, i) in asserts"
              :key="i"
              class="wb-res__assert"
              :class="{ ok: a.ok, fail: !a.ok }"
            >
              {{ a.ok ? "✓" : "✗" }} {{ a.name }}
              <div v-if="a.message" class="msg">{{ a.message }}</div>
            </div>
          </div>
          <ElEmpty v-else :description="$t('upstreamConnector.noAssertions')" :image-size="48" />
        </ElTabPane>
      </ElTabs>
    </template>


  </div>
</template>

<script setup lang="ts">
import type { ConnectorRun, StageResult } from "@/api/upstream-connectors";
import { $t } from "@/locales";
import {
  byteSizeOf,
  detectResponseBodyKind,
  extractRunResponse,
  jsonTreeFits,
  prettyJson,
  tryParseJsonValue,
} from "./workbenchHelpers";
import WorkbenchCodeEditor from "./WorkbenchCodeEditor.vue";
import WorkbenchJsonTree from "./hopp/WorkbenchJsonTree.vue";
import WorkbenchResponseMeta from "./hopp/WorkbenchResponseMeta.vue";

const VIEW_KEY = "mazbri.workbench.res.view";

const props = defineProps<{
  run: ConnectorRun | null;
  runs: ConnectorRun[];
  loading?: boolean;
  adapter?: string;
  sendBlockReason?: string;
}>();

const emit = defineEmits<{
  "select-run": [ConnectorRun];
  "open-shortcuts": [];
}>();

const idleTitle = computed(() => {
  if (props.sendBlockReason === "url") {
    return $t("upstreamConnector.urlMissingSend");
  }
  return $t("upstreamConnector.responseIdle");
});

const tab = ref("body");
const openStage = ref("");
const viewPref = ref<"tree" | "raw">(readViewPref());

const view = computed(() => extractRunResponse(props.run));
const engineErrorText = computed(() => {
  const run = props.run;
  if (!run) return "";
  if (run.error_code === "ASSERTION_FAILED" && view.value.status != null) return "";
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
const bodyKind = computed(() =>
  detectResponseBodyKind(rawBody.value, props.adapter),
);
const parsedJson = computed(() => tryParseJsonValue(rawBody.value));
const jsonTooLarge = computed(
  () =>
    bodyKind.value === "json" &&
    parsedJson.value.ok &&
    !jsonTreeFits(parsedJson.value.value),
);
const showTree = computed(
  () =>
    bodyKind.value === "json" &&
    viewPref.value === "tree" &&
    parsedJson.value.ok &&
    !jsonTooLarge.value,
);
const displayBody = computed(() =>
  bodyKind.value === "json" ? prettyJson(rawBody.value) : rawBody.value,
);
const rawText = computed(() => displayBody.value);
const rawLang = computed(() => {
  if (bodyKind.value === "xml") return "XML";
  if (bodyKind.value === "json") return "JSON";
  return "JSON";
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

const STAGE_I18N: Record<string, string> = {
  validate: "validate",
  ssrf: "ssrf",
  auth: "auth",
  request_transform: "request_transform",
  upstream: "upstream",
  response_transform: "response_transform",
};

function stageLabel(name: string) {
  const key = `upstreamConnector.stage.${name}`;
  const t = $t(key);
  return t === key ? STAGE_I18N[name] || name : t;
}

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

function readViewPref(): "tree" | "raw" {
  try {
    return localStorage.getItem(VIEW_KEY) === "raw" ? "raw" : "tree";
  } catch {
    return "tree";
  }
}

function setView(next: "tree" | "raw") {
  viewPref.value = next;
  try {
    localStorage.setItem(VIEW_KEY, next);
  } catch {
    void 0;
  }
}
</script>

<style scoped lang="scss">
.wb-res {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}
.wb-res__empty-title {
  font-weight: 600;
  font-size: 13px;
  padding: 16px;
}
.wb-res__idle {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  color: var(--el-text-color-secondary);
  text-align: center;
  padding: 16px;
  p {
    margin: 4px 0;
  }
  .hint {
    font-size: 12px;
    color: var(--el-text-color-placeholder);
  }
  &.compact {
    flex: 0 0 auto;
    justify-content: flex-start;
    padding: 12px 8px 16px;
  }
}
.wb-res__idle-icon {
  font-size: 32px;
  margin-bottom: 8px;
  color: var(--el-color-primary-light-5);
}
.wb-res__idle-link {
  margin-top: 8px;
  border: none;
  background: transparent;
  color: var(--wb-accent, var(--el-color-primary));
  font-size: 12px;
  cursor: pointer;
}
.wb-res__loading {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  color: var(--wb-text-muted, var(--el-text-color-secondary));
  font-size: 13px;
  p {
    margin: 0;
  }
}
.wb-res__spin {
  font-size: 22px;
  animation: wb-res-spin 0.8s linear infinite;
}
@keyframes wb-res-spin {
  to {
    transform: rotate(360deg);
  }
}
.wb-res__tabs {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  padding: 0 8px;
  :deep(.el-tabs__header) {
    flex-shrink: 0;
  }
  :deep(.el-tabs__content) {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }
  :deep(.el-tab-pane) {
    height: 100%;
    min-height: 140px;
  }
}
.wb-res__body {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}
.wb-res__body-bar {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 2px;
  flex-shrink: 0;
  min-height: 32px;
  padding: 0 4px;
  border-bottom: 1px solid var(--wb-divider-light, var(--el-border-color-extra-light));
}
.wb-res__body-bar :deep(.wb-hopp-icon.is-on) {
  color: var(--wb-accent, var(--el-color-primary));
}
.wb-res__too-large {
  margin: 8px 12px 0;
  font-size: 12px;
  color: var(--wb-text-muted, var(--el-text-color-secondary));
}
.wb-res__headers {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 12px;
}
.wb-res__hrow {
  display: grid;
  grid-template-columns: 140px 1fr;
  gap: 8px;
  .k {
    color: var(--el-text-color-secondary);
    word-break: break-all;
  }
  .v {
    word-break: break-all;
  }
}
.wb-res__stages {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}
.wb-res__stage {
  border: 1px solid var(--el-border-color-lighter);
  background: var(--el-fill-color-blank);
  border-radius: var(--wb-radius, var(--art-form-control-radius));
  padding: 6px 8px;
  cursor: pointer;
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  font-size: 11px;
  &.ok {
    border-color: var(--el-color-success-light-5);
  }
  &.fail {
    border-color: var(--el-color-danger-light-5);
  }
  &.open {
    box-shadow: 0 0 0 1px var(--el-color-primary-light-7);
  }
  .ms {
    color: var(--el-text-color-placeholder);
  }
}
.wb-res__stage-pre {
  margin: 8px 0 0;
  font-size: 11px;
  white-space: pre-wrap;
  word-break: break-all;
  max-height: 220px;
  overflow: auto;
  background: var(--el-fill-color-light);
  padding: 8px;
  border-radius: var(--wb-radius, var(--art-form-control-radius));
}
.wb-res__assert {
  font-size: 12px;
  padding: 4px 0;
  &.ok {
    color: var(--el-color-success);
  }
  &.fail {
    color: var(--el-color-danger);
  }
  .msg {
    color: var(--el-text-color-secondary);
    font-size: 11px;
  }
}
.wb-res__hist {
  flex-shrink: 0;
  border-top: 1px solid var(--el-border-color-extra-light);
  margin-top: 8px;
  padding: 8px 10px 0;
  max-height: 32%;
  display: flex;
  flex-direction: column;
  min-height: 0;
}
.wb-res__hist-title {
  font-size: 12px;
  font-weight: 600;
  margin-bottom: 4px;
}
.wb-res__hist-list {
  overflow: auto;
  flex: 1;
}
.wb-res__hist-item {
  display: grid;
  grid-template-columns: auto 1fr auto auto;
  gap: 6px;
  align-items: center;
  width: 100%;
  text-align: left;
  background: none;
  border: none;
  padding: 4px 2px;
  cursor: pointer;
  font-size: 11px;
  color: var(--el-text-color-secondary);
  &.active {
    background: var(--el-fill-color-light);
    border-radius: var(--wb-radius, var(--art-form-control-radius));
  }
  .mode {
    font-size: 10px;
    font-weight: 600;
    padding: 1px 6px;
    border: 1px solid var(--wb-divider, var(--el-border-color));
    border-radius: var(--wb-radius, var(--art-form-control-radius));
    color: var(--wb-text-muted, var(--el-text-color-secondary));
  }
  .ep {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
}
</style>
