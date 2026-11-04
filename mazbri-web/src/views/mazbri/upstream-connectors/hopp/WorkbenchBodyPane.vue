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

    <div v-if="missingFields.length" class="wb-body__missing">
      <p class="wb-hint is-warn">
        {{ $t("upstreamConnector.adapterFieldsMissing") }}
      </p>
      <ul>
        <li v-for="m in missingFields" :key="m.path">
          <code>{{ m.path }}</code>
          — {{ $t(m.labelKey) }}
        </li>
      </ul>
    </div>

    <div v-if="adapterKind === 'graphql'" class="wb-body__extra">
      <p class="wb-body__section">{{ $t("upstreamConnector.adapterParamsSection") }}</p>
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
      <p class="wb-body__section">{{ $t("upstreamConnector.adapterParamsSection") }}</p>
      <p class="wb-hint is-warn">
        {{ $t("upstreamConnector.jobPollAsyncHint") }}
      </p>
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
    </div>

    <div v-if="adapterKind === 'grpc'" class="wb-body__extra">
      <p class="wb-body__section">{{ $t("upstreamConnector.adapterParamsSection") }}</p>
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

    <div v-if="adapterKind === 'http_envelope'" class="wb-body__extra">
      <p class="wb-body__section">{{ $t("upstreamConnector.adapterParamsSection") }}</p>
      <WorkbenchGhostField
        :label="$t('upstreamConnector.operation')"
        :model-value="endpoint.operation || ''"
        :placeholder="$t('upstreamConnector.operationHintEnvelope')"
        @update:model-value="endpoint.operation = String($event)"
      />
      <WorkbenchGhostField
        :label="$t('upstreamConnector.wrap')"
        kind="select"
        :model-value="endpoint.wrap || 'method_biz_content'"
        :options="wrapOptions"
        :append-to="appendTo"
        @update:model-value="endpoint.wrap = String($event)"
      />
      <WorkbenchGhostField
        :label="$t('upstreamConnector.unwrap')"
        :model-value="endpoint.unwrap || ''"
        placeholder="$.data"
        @update:model-value="endpoint.unwrap = String($event)"
      />
    </div>

    <div v-if="adapterKind === 'jsonrpc'" class="wb-body__extra">
      <p class="wb-body__section">{{ $t("upstreamConnector.adapterParamsSection") }}</p>
      <WorkbenchGhostField
        :label="$t('upstreamConnector.jsonrpcMethod')"
        :model-value="endpoint.operation || ''"
        :placeholder="$t('upstreamConnector.jsonrpcMethodHint')"
        @update:model-value="endpoint.operation = String($event)"
      />
      <p class="wb-hint">{{ $t("upstreamConnector.jsonrpcIdHint") }}</p>
    </div>

    <div v-if="adapterKind === 'soap_xml'" class="wb-body__extra">
      <p class="wb-body__section">{{ $t("upstreamConnector.adapterParamsSection") }}</p>
      <WorkbenchGhostField
        :label="$t('upstreamConnector.soapOperation')"
        :model-value="endpoint.operation || ''"
        :placeholder="$t('upstreamConnector.soapOperationHint')"
        @update:model-value="onSoapOperation(String($event))"
      />
      <WorkbenchGhostField
        :label="$t('upstreamConnector.soapAction')"
        :model-value="soapAction"
        :placeholder="$t('upstreamConnector.soapActionHint')"
        @update:model-value="soapAction = String($event)"
      />
      <WorkbenchKvRows
        v-model="soapFieldMapRows"
        :title="$t('upstreamConnector.soapFieldMap')"
        :add-title="$t('upstreamConnector.addQuery')"
        :clear-title="$t('upstreamConnector.clearKv')"
        :env="env"
      />
      <WorkbenchGhostField
        :label="$t('upstreamConnector.soapNumberFields')"
        :model-value="soapNumberFieldsText"
        :placeholder="$t('upstreamConnector.soapNumberFieldsHint')"
        @update:model-value="soapNumberFieldsText = String($event)"
      />
    </div>

    <div v-if="adapterKind === 'ws_unary'" class="wb-body__extra">
      <p class="wb-body__section">{{ $t("upstreamConnector.adapterParamsSection") }}</p>
      <WorkbenchGhostField
        :label="$t('upstreamConnector.operation')"
        :model-value="endpoint.operation || ''"
        :placeholder="$t('upstreamConnector.wsOperationHint')"
        @update:model-value="endpoint.operation = String($event)"
      />
      <WorkbenchGhostField
        :label="$t('upstreamConnector.wsPath')"
        :model-value="wsPath"
        :placeholder="endpoint.path || '/ws'"
        @update:model-value="wsPath = String($event)"
      />
      <WorkbenchGhostField
        :label="$t('upstreamConnector.wrap')"
        kind="select"
        :model-value="endpoint.wrap || 'none'"
        :options="wrapOptions"
        :append-to="appendTo"
        @update:model-value="endpoint.wrap = String($event)"
      />
      <WorkbenchGhostField
        :label="$t('upstreamConnector.wsAuthIn')"
        kind="select"
        :model-value="wsAuthInPrimary"
        :options="wsAuthInOptions"
        :append-to="appendTo"
        @update:model-value="setWsAuthIn(String($event))"
      />
      <p class="wb-hint">{{ $t("upstreamConnector.wsAuthInHint") }}</p>
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
  listMissingAdapterFields,
  mapToKv,
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

const wrapOptions = computed(() => [
  {
    label: $t("upstreamConnector.wrapNames.method_biz_content"),
    value: "method_biz_content",
  },
  { label: $t("upstreamConnector.wrapNames.none"), value: "none" },
]);

const wsAuthInOptions = computed(() => [
  { label: $t("upstreamConnector.wsAuthInNone"), value: "" },
  { label: $t("upstreamConnector.injectToHeader"), value: "header" },
  { label: $t("upstreamConnector.injectToQuery"), value: "query" },
  { label: $t("upstreamConnector.wsAuthInFirstFrame"), value: "first_frame" },
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

const missingFields = computed(() => listMissingAdapterFields(props.endpoint));

watch(
  () => props.endpoint.adapter,
  () => {
    ensureEndpointAdapterFields(props.endpoint);
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

function setJob(key: string, value: unknown) {
  ensureEndpointAdapterFields(props.endpoint);
  props.endpoint.job_poll = { ...(props.endpoint.job_poll || {}), [key]: value };
}

function headerValue(name: string): string {
  const entries = props.endpoint.header_entries;
  if (entries?.length) {
    const hit = entries.find(
      (r) => r.enabled !== false && r.key.toLowerCase() === name.toLowerCase(),
    );
    if (hit) return hit.value;
  }
  const headers = props.endpoint.headers || {};
  for (const [k, v] of Object.entries(headers)) {
    if (k.toLowerCase() === name.toLowerCase()) return String(v);
  }
  return "";
}

function setHeaderValue(name: string, value: string) {
  const trimmed = value.trim();
  if (!props.endpoint.headers) props.endpoint.headers = {};
  const headers = { ...props.endpoint.headers };
  const existingKey = Object.keys(headers).find(
    (k) => k.toLowerCase() === name.toLowerCase(),
  );
  if (!trimmed) {
    if (existingKey) delete headers[existingKey];
  } else if (existingKey) {
    headers[existingKey] = trimmed;
  } else {
    headers[name] = trimmed;
  }
  props.endpoint.headers = headers;

  if (props.endpoint.header_entries?.length) {
    const idx = props.endpoint.header_entries.findIndex(
      (r) => r.key.toLowerCase() === name.toLowerCase(),
    );
    if (!trimmed) {
      if (idx >= 0) props.endpoint.header_entries.splice(idx, 1);
    } else if (idx >= 0) {
      props.endpoint.header_entries[idx] = {
        ...props.endpoint.header_entries[idx],
        value: trimmed,
        enabled: true,
      };
    } else {
      props.endpoint.header_entries.push({
        key: name,
        value: trimmed,
        enabled: true,
      });
    }
  }
}

const soapAction = computed({
  get: () => headerValue("SOAPAction"),
  set: (v: string) => setHeaderValue("SOAPAction", v),
});

function onSoapOperation(v: string) {
  props.endpoint.operation = v;
  ensureEndpointAdapterFields(props.endpoint);
}

const soapFieldMapRows = ref<KvRow[]>([]);
let soapMapSyncing = false;

watch(
  () => props.endpoint.soap,
  () => {
    if (soapMapSyncing) return;
    ensureEndpointAdapterFields(props.endpoint);
    const fm = (props.endpoint.soap?.field_map || {}) as Record<string, string>;
    soapFieldMapRows.value = mapToKv(fm);
  },
  { immediate: true, deep: true },
);

watch(
  soapFieldMapRows,
  (rows) => {
    if (adapterKind.value !== "soap_xml") return;
    soapMapSyncing = true;
    ensureEndpointAdapterFields(props.endpoint);
    props.endpoint.soap = {
      ...(props.endpoint.soap || {}),
      field_map: kvToMap(stripEmptyKv(rows)),
    };
    void nextTick(() => {
      soapMapSyncing = false;
    });
  },
  { deep: true },
);

const soapNumberFieldsText = computed({
  get: () => {
    const arr = props.endpoint.soap?.number_fields;
    return Array.isArray(arr) ? arr.map(String).join(", ") : "";
  },
  set: (v: string) => {
    ensureEndpointAdapterFields(props.endpoint);
    const fields = v
      .split(/[,，\s]+/)
      .map((s) => s.trim())
      .filter(Boolean);
    props.endpoint.soap = {
      ...(props.endpoint.soap || {}),
      number_fields: fields,
    };
  },
});

const wsPath = computed({
  get: () => String((props.endpoint.ws as { path?: string } | undefined)?.path ?? ""),
  set: (v: string) => {
    ensureEndpointAdapterFields(props.endpoint);
    props.endpoint.ws = { ...(props.endpoint.ws || {}), path: v };
  },
});

const wsAuthInPrimary = computed(() => {
  const auth = (props.endpoint.ws as { auth_in?: string[] } | undefined)?.auth_in;
  if (!Array.isArray(auth) || !auth.length) return "";
  return String(auth[0] || "");
});

function setWsAuthIn(value: string) {
  ensureEndpointAdapterFields(props.endpoint);
  const next = value.trim() ? [value.trim()] : [];
  props.endpoint.ws = { ...(props.endpoint.ws || {}), auth_in: next };
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
.wb-body__section {
  margin: 8px 16px 4px;
  font-size: 12px;
  font-weight: 600;
  color: var(--wb-text-muted, var(--el-text-color-secondary));
}
.wb-body__upload {
  padding: 8px 16px;
}
.wb-body__missing {
  margin: 0 12px;
  padding: 8px 10px;
  border-radius: 6px;
  background: color-mix(in srgb, var(--wb-danger, var(--el-color-danger)) 8%, transparent);
  ul {
    margin: 4px 0 0;
    padding-left: 18px;
    font-size: 12px;
  }
  code {
    font-size: 11px;
  }
}
.wb-hint {
  font-size: 12px;
  color: var(--wb-text-muted, var(--el-text-color-secondary));
  margin: 0 16px 8px;
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
