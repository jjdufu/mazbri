<template>
  <div class="wb-body">
    <div class="wb-hopp-sticky">
      <span class="wb-hopp-sticky__label">{{
        $t("upstreamConnector.bodyContentType")
      }}</span>
      <WorkbenchHoppSelect
        :model-value="contentType"
        :options="contentTypeOptions"
        :append-to="appendTo"
        @update:model-value="setContentType"
      />
    </div>
    <div v-if="contentType === 'none'" class="wb-body__empty">
      {{ $t("upstreamConnector.bodyNoneEmpty") }}
    </div>
    <template v-else>
      <p v-if="jsonCoerceFailed" class="wb-hint is-warn">
        {{ $t("upstreamConnector.bodyNeedsJsonObject") }}
      </p>
      <WorkbenchKvRows
        v-if="isFormEncoding"
        v-model="formRows"
        :title="$t('upstreamConnector.formBody')"
        :add-title="$t('upstreamConnector.addQuery')"
        :clear-title="$t('upstreamConnector.clearKv')"
        :env="env"
      />
      <WorkbenchCodeEditor
        v-else
        v-model="jsonText"
        fill
        height="280px"
        lang-label="JSON"
      />
    </template>

    <div v-if="adapterKind === 'graphql'" class="wb-body__extra">
      <WorkbenchCodeEditor
        v-model="gqlDocument"
        fill
        height="180px"
        lang-label="GraphQL"
        placeholder="query { }"
      />
      <WorkbenchGhostField
        :label="$t('upstreamConnector.graphqlOperationName')"
        :model-value="gqlOperation"
        :append-to="appendTo"
        @update:model-value="gqlOperation = String($event)"
      />
      <WorkbenchKvRows
        v-model="gqlVarRows"
        :title="$t('upstreamConnector.graphqlVariablesMap')"
        :add-title="$t('upstreamConnector.addQuery')"
        :clear-title="$t('upstreamConnector.clearKv')"
        :env="env"
      />
    </div>

    <div v-if="adapterKind === 'job_poll'" class="wb-body__extra">
      <WorkbenchGhostField
        :label="$t('upstreamConnector.jobSubmitPath')"
        :model-value="jobField('submit_path')"
        :placeholder="endpoint.path || '/'"
        @update:model-value="setJob('submit_path', String($event))"
      />
      <WorkbenchGhostField
        :label="$t('upstreamConnector.jobQueryPath')"
        :model-value="jobField('query_path')"
        placeholder="/jobs/{job_id}"
        @update:model-value="setJob('query_path', String($event))"
      />
      <WorkbenchGhostField
        :label="$t('upstreamConnector.jobIdPath')"
        :model-value="jobField('job_id_jsonpath')"
        placeholder="$.job_id"
        @update:model-value="setJob('job_id_jsonpath', String($event))"
      />
      <WorkbenchGhostField
        :label="$t('upstreamConnector.jobStatusPath')"
        :model-value="jobField('status_jsonpath')"
        placeholder="$.status"
        @update:model-value="setJob('status_jsonpath', String($event))"
      />
      <WorkbenchGhostField
        :label="$t('upstreamConnector.jobResultPath')"
        :model-value="jobField('result_jsonpath')"
        placeholder="$.result"
        @update:model-value="setJob('result_jsonpath', String($event))"
      />
      <WorkbenchGhostField
        :label="$t('upstreamConnector.jobMessagePath')"
        :model-value="jobField('message_jsonpath')"
        placeholder="$.message"
        @update:model-value="setJob('message_jsonpath', String($event))"
      />
      <WorkbenchGhostField
        :label="$t('upstreamConnector.jobPollInterval')"
        kind="number"
        :model-value="jobInterval"
        :min="1"
        :step="100"
        @update:model-value="setJob('poll_interval_ms', Number($event) || 500)"
      />
    </div>

    <div v-if="adapterKind === 'grpc'" class="wb-body__extra">
      <WorkbenchGhostField
        :label="$t('upstreamConnector.artifactId')"
        :model-value="endpoint.artifact_id || ''"
        placeholder="uuid"
        @update:model-value="endpoint.artifact_id = String($event)"
      />
      <div class="wb-body__upload">
        <button
          type="button"
          class="wb-hopp-action"
          :disabled="uploadingArtifact"
          @click="emit('pick-artifact')"
        >
          {{ $t("upstreamConnector.artifactUpload") }}
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { $t } from "@/locales";
import type { ConnectorEndpoint } from "@/api/upstream-connectors";
import type { KvRow } from "../workbenchHelpers";
import {
  adapterBodyKind,
  bodyAsFormRows,
  bodyToEditorText,
  coerceEndpointJsonBody,
  defaultBodyContentType,
  editorTextToBody,
  ensureEndpointAdapterFields,
  kvToMap,
  resolvedBodyEncoding,
  setTenantBodyKind,
  stripEmptyKv,
  type BodyContentType,
} from "../workbenchHelpers";
import WorkbenchCodeEditor from "../WorkbenchCodeEditor.vue";
import WorkbenchKvRows from "../WorkbenchKvRows.vue";
import WorkbenchGhostField from "./WorkbenchGhostField.vue";
import WorkbenchHoppSelect from "./WorkbenchHoppSelect.vue";

const props = withDefaults(
  defineProps<{
    endpoint: ConnectorEndpoint;
    gqlVarRows: KvRow[];
    env?: Record<string, string>;
    uploadingArtifact?: boolean;
    appendTo?: HTMLElement | string;
  }>(),
  { env: () => ({}), uploadingArtifact: false, appendTo: undefined },
);

const emit = defineEmits<{
  "update:gqlVarRows": [KvRow[]];
  "pick-artifact": [];
}>();

const jsonCoerceFailed = ref(false);

const contentType = computed(() => defaultBodyContentType(props.endpoint));
const isFormEncoding = computed(
  () =>
    contentType.value === "json" &&
    resolvedBodyEncoding(props.endpoint) === "form",
);

const contentTypeOptions = computed(() => [
  { label: $t("upstreamConnector.bodyTypeNone"), value: "none" },
  { label: $t("upstreamConnector.bodyTypeJson"), value: "json" },
]);

function setContentType(value: string) {
  const next = value === "none" ? "none" : "json";
  setTenantBodyKind(props.endpoint, next as BodyContentType);
  jsonCoerceFailed.value = false;
  if (next === "json") {
    jsonCoerceFailed.value = !coerceEndpointJsonBody(props.endpoint);
  }
}

const adapterKind = computed(() => adapterBodyKind(props.endpoint.adapter));

watch(
  () => props.endpoint.adapter,
  () => {
    if (defaultBodyContentType(props.endpoint) === "none") {
      jsonCoerceFailed.value = false;
      return;
    }
    jsonCoerceFailed.value = !coerceEndpointJsonBody(props.endpoint);
  },
  { immediate: true },
);

const jsonText = computed({
  get: () => bodyToEditorText(props.endpoint.body, "json"),
  set: (v: string) => {
    props.endpoint.body = editorTextToBody(v, "json");
  },
});

const formRows = ref<KvRow[]>([]);
let formSyncing = false;

watch(
  () =>
    [
      resolvedBodyEncoding(props.endpoint),
      props.endpoint.body,
    ] as const,
  () => {
    if (resolvedBodyEncoding(props.endpoint) !== "form") return;
    if (formSyncing) return;
    formRows.value = bodyAsFormRows(props.endpoint.body);
  },
  { immediate: true, deep: true },
);

watch(
  formRows,
  (rows) => {
    if (resolvedBodyEncoding(props.endpoint) !== "form") return;
    formSyncing = true;
    props.endpoint.body = kvToMap(stripEmptyKv(rows));
    void nextTick(() => {
      formSyncing = false;
    });
  },
  { deep: true },
);

const gqlVarRows = computed({
  get: () => props.gqlVarRows,
  set: (v: KvRow[]) => emit("update:gqlVarRows", v),
});

const gqlDocument = computed({
  get: () => String(props.endpoint.graphql?.document ?? ""),
  set: (v: string) => {
    ensureEndpointAdapterFields(props.endpoint);
    props.endpoint.graphql = { ...(props.endpoint.graphql || {}), document: v };
  },
});

const gqlOperation = computed({
  get: () => String(props.endpoint.graphql?.operation_name ?? ""),
  set: (v: string) => {
    ensureEndpointAdapterFields(props.endpoint);
    props.endpoint.graphql = {
      ...(props.endpoint.graphql || {}),
      operation_name: v,
    };
  },
});

function jobField(key: string): string {
  const raw = (props.endpoint.job_poll || {}) as Record<string, unknown>;
  return String(raw[key] ?? "");
}

const jobInterval = computed(() => {
  const n = Number(
    (props.endpoint.job_poll as Record<string, unknown> | undefined)
      ?.poll_interval_ms,
  );
  return Number.isFinite(n) && n > 0 ? n : 500;
});

function setJob(key: string, value: unknown) {
  ensureEndpointAdapterFields(props.endpoint);
  props.endpoint.job_poll = { ...(props.endpoint.job_poll || {}), [key]: value };
}
</script>

<style scoped lang="scss">
.wb-body {
  display: flex;
  flex-direction: column;
  min-height: 0;
  height: 100%;
  gap: 8px;
}
.wb-body__extra {
  display: flex;
  flex-direction: column;
}
.wb-body__upload {
  padding: 8px 16px;
}
.wb-hint {
  font-size: 12px;
  color: var(--wb-text-muted, var(--el-text-color-secondary));
  margin: 0 0 8px;
  &.is-warn {
    color: var(--wb-danger, var(--el-color-danger));
  }
}
.wb-body__empty {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  min-height: 120px;
  padding: 24px 16px;
  font-size: 13px;
  color: var(--wb-text-muted, var(--el-text-color-secondary));
  text-align: center;
  gap: 8px;
}
</style>
