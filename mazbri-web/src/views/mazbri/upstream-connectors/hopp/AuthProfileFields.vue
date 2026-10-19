<template>
  <div v-if="group === 'empty'" class="wb-auth__empty">
    <template v-if="picker === 'inherit'">
      {{ $t("upstreamConnector.authInheritHint") }}
      <div class="wb-auth__readonly">
        {{ $t("upstreamConnector.authSessionDefault") }}:
        {{ sessionLabel }}
      </div>
    </template>
    <template v-else>
      {{ $t("upstreamConnector.authNoneEmpty") }}
    </template>
  </div>

  <div v-else class="wb-auth__fields">
    <p class="wb-auth__hint">{{ hintText }}</p>

    <template v-if="group === 'basic' || group === 'digest'">
      <WorkbenchGhostField
        :label="$t('upstreamConnector.authUsername')"
        :model-value="cred('username')"
        :disabled="disabled"
        @update:model-value="setCred('username', String($event))"
      />
      <WorkbenchGhostField
        :label="$t('upstreamConnector.authPassword')"
        :model-value="cred('password')"
        :disabled="disabled"
        @update:model-value="setCred('password', String($event))"
      />
      <template v-if="group === 'digest'">
        <WorkbenchGhostField
          :label="$t('upstreamConnector.authDigestAlgo')"
          kind="select"
          :model-value="flow.digest.algorithm || 'MD5'"
          :options="digestAlgoOptions"
          :append-to="appendTo"
          :disabled="disabled"
          @update:model-value="flow.digest.algorithm = String($event)"
        />
        <WorkbenchGhostField
          :label="$t('upstreamConnector.authDigestRealm')"
          :model-value="flow.digest.realm"
          :disabled="disabled"
          @update:model-value="flow.digest.realm = String($event)"
        />
      </template>
    </template>

    <template v-else-if="group === 'api_key' || group === 'bearer'">
      <WorkbenchGhostField
        v-if="group === 'api_key'"
        :label="$t('upstreamConnector.authApiKeyLabel')"
        :model-value="cred('api_key')"
        :disabled="disabled"
        @update:model-value="setCred('api_key', String($event))"
      />
      <WorkbenchGhostField
        v-else
        :label="$t('upstreamConnector.authTokenLabel')"
        :model-value="cred('token')"
        :disabled="disabled"
        @update:model-value="setCred('token', String($event))"
      />
      <AuthInjectFields
        :flow="flow"
        :disabled="disabled"
        :append-to="appendTo"
      />
    </template>

    <template v-else-if="group === 'ticket'">
      <WorkbenchGhostField
        :label="$t('supplier.config.tokenUrl')"
        :model-value="flow.token_url"
        placeholder="https://api.upstream.com/get/token"
        :disabled="disabled"
        @update:model-value="flow.token_url = String($event)"
      />
      <WorkbenchGhostField
        :label="$t('upstreamConnector.authTokenPath')"
        :model-value="flow.token_path"
        placeholder="/oauth/token"
        :disabled="disabled"
        @update:model-value="flow.token_path = String($event)"
      />
      <WorkbenchGhostField
        :label="$t('supplier.config.method')"
        kind="select"
        :model-value="flow.method || (picker === 'gettoken' ? 'GET' : 'POST')"
        :options="methodOptions"
        :append-to="appendTo"
        :disabled="disabled"
        @update:model-value="flow.method = String($event)"
      />
      <WorkbenchKvRows
        v-model="headerRows"
        variant="headers"
        empty-compact
        foldable
        :title="$t('supplier.config.authHeaders')"
        :add-title="$t('upstreamConnector.addHeader')"
        :clear-title="$t('upstreamConnector.clearKv')"
        :env="env"
        :append-to="appendTo"
      />
      <WorkbenchGhostField
        v-if="picker === 'oauth_cc_form'"
        :label="$t('upstreamConnector.authClientId')"
        :model-value="cred('client_id')"
        :disabled="disabled"
        @update:model-value="setCred('client_id', String($event))"
      />
      <WorkbenchGhostField
        v-if="picker === 'oauth_cc_form'"
        :label="$t('upstreamConnector.authClientSecret')"
        :model-value="cred('client_secret')"
        :disabled="disabled"
        @update:model-value="setCred('client_secret', String($event))"
      />
      <WorkbenchGhostField
        v-if="picker === 'gettoken'"
        :label="$t('upstreamConnector.authAppId')"
        :model-value="cred('app_id')"
        :disabled="disabled"
        @update:model-value="setCred('app_id', String($event))"
      />
      <WorkbenchGhostField
        v-if="picker === 'gettoken'"
        :label="$t('upstreamConnector.authSecret')"
        :model-value="cred('secret')"
        :disabled="disabled"
        @update:model-value="setCred('secret', String($event))"
      />
      <div v-if="picker === 'oauth_cc_form'" class="wb-section wb-section--nested">
        <button
          type="button"
          class="wb-section__head is-toggle"
          @click="bodyOpen = !bodyOpen"
        >
          <span class="wb-section__title">{{
            $t("supplier.config.requestBodyTpl")
          }}</span>
        </button>
        <div v-show="bodyOpen" class="wb-section__body">
          <JsonTemplateEditor
            v-model="flow.request_body_tpl"
            empty-placeholder
            :variables="authVariables"
            :key-column-label="$t('upstreamConnector.kvKey')"
            :value-column-label="$t('upstreamConnector.kvValue')"
            :op-column-label="$t('common.fields.operations')"
            :key-placeholder="$t('upstreamConnector.kvPhKey')"
            :value-placeholder="$t('upstreamConnector.kvPhValue')"
          />
        </div>
      </div>
      <WorkbenchGhostField
        :label="$t('supplier.config.tokenJsonpath')"
        :model-value="flow.token_jsonpath"
        :placeholder="picker === 'gettoken' ? '$.token' : '$.access_token'"
        :disabled="disabled"
        @update:model-value="flow.token_jsonpath = String($event)"
      >
        <JsonPathHelper
          plain
          :seed-json="seedJson"
          @select="(p: string) => (flow.token_jsonpath = p)"
        />
      </WorkbenchGhostField>
      <WorkbenchGhostField
        :label="$t('supplier.config.expiryJsonpath')"
        :model-value="flow.expiry_jsonpath"
        placeholder="$.expires_in"
        :disabled="disabled"
        @update:model-value="flow.expiry_jsonpath = String($event)"
      >
        <JsonPathHelper
          plain
          :seed-json="seedJson"
          @select="(p: string) => (flow.expiry_jsonpath = p)"
        />
      </WorkbenchGhostField>
      <AuthInjectFields
        :flow="flow"
        :disabled="disabled"
        :append-to="appendTo"
      />
    </template>

    <template v-else-if="group === 'sign_form'">
      <WorkbenchGhostField
        :label="$t('upstreamConnector.authAppId')"
        :model-value="cred('app_id')"
        :disabled="disabled"
        @update:model-value="setCred('app_id', String($event))"
      />
      <WorkbenchGhostField
        :label="$t('upstreamConnector.authClientSecret')"
        :model-value="cred('client_secret')"
        :disabled="disabled"
        @update:model-value="setCred('client_secret', String($event))"
      />
      <p v-if="adapter && adapter !== 'sign_form'" class="wb-auth__warn">
        {{ $t("upstreamConnector.authSignAdapterHint") }}
      </p>
    </template>

    <template v-else-if="group === 'grpc_metadata'">
      <WorkbenchGhostField
        :label="$t('supplier.config.injectKey')"
        :model-value="flow.inject_key"
        placeholder="Authorization"
        :disabled="disabled"
        @update:model-value="flow.inject_key = String($event)"
      />
      <WorkbenchGhostField
        :label="$t('supplier.config.injectFormat')"
        :model-value="flow.inject_value_format"
        placeholder="{}"
        :disabled="disabled"
        @update:model-value="flow.inject_value_format = String($event)"
      />
      <WorkbenchGhostField
        :label="$t('upstreamConnector.authSecret')"
        :model-value="cred('api_key') || cred('secret')"
        :disabled="disabled"
        @update:model-value="setCred('api_key', String($event))"
      />
    </template>

    <template v-else-if="group === 'jwt'">
      <WorkbenchGhostField
        :label="$t('upstreamConnector.authJwtAlgo')"
        kind="select"
        :model-value="flow.jwt.algorithm || 'HS256'"
        :options="jwtAlgoOptions"
        :append-to="appendTo"
        :disabled="disabled"
        @update:model-value="flow.jwt.algorithm = String($event)"
      />
      <WorkbenchGhostField
        :label="$t('upstreamConnector.authJwtTtl')"
        kind="number"
        :model-value="flow.jwt.ttl_secs || 300"
        :disabled="disabled"
        @update:model-value="flow.jwt.ttl_secs = Number($event) || 300"
      />
      <WorkbenchGhostField
        :label="$t('upstreamConnector.authJwtKid')"
        :model-value="flow.jwt.kid"
        :disabled="disabled"
        @update:model-value="flow.jwt.kid = String($event)"
      />
      <div class="wb-section wb-section--nested">
        <div class="wb-section__head">
          <span class="wb-section__title">{{
            $t("upstreamConnector.authJwtClaims")
          }}</span>
        </div>
        <div class="wb-section__body">
          <JsonTemplateEditor
            v-model="flow.jwt.claims_tpl"
            empty-placeholder
            :variables="authVariables"
            :key-column-label="$t('upstreamConnector.kvKey')"
            :value-column-label="$t('upstreamConnector.kvValue')"
            :op-column-label="$t('common.fields.operations')"
            :key-placeholder="$t('upstreamConnector.kvPhKey')"
            :value-placeholder="$t('upstreamConnector.kvPhValue')"
          />
        </div>
      </div>
      <WorkbenchGhostField
        v-if="isHsJwt"
        :label="$t('upstreamConnector.authJwtSecret')"
        :model-value="cred('jwt_secret')"
        :disabled="disabled"
        @update:model-value="setCred('jwt_secret', String($event))"
      />
      <label v-else class="wb-ghost">
        <span class="wb-ghost__label">{{
          $t("upstreamConnector.authJwtPrivateKey")
        }}</span>
        <textarea
          class="wb-ghost__input wb-auth__pem"
          :value="cred('jwt_private_key')"
          :disabled="disabled"
          rows="4"
          @input="
            setCred(
              'jwt_private_key',
              ($event.target as HTMLTextAreaElement).value,
            )
          "
        />
      </label>
      <AuthInjectFields
        :flow="flow"
        :disabled="disabled"
        :append-to="appendTo"
      />
    </template>

    <template v-else-if="group === 'aws_sigv4'">
      <WorkbenchGhostField
        :label="$t('upstreamConnector.authAwsRegion')"
        :model-value="flow.aws.region"
        :disabled="disabled"
        @update:model-value="flow.aws.region = String($event)"
      />
      <WorkbenchGhostField
        :label="$t('upstreamConnector.authAwsService')"
        :model-value="flow.aws.service"
        :disabled="disabled"
        @update:model-value="flow.aws.service = String($event)"
      />
      <WorkbenchGhostField
        :label="$t('upstreamConnector.authAwsAccessKey')"
        :model-value="cred('access_key_id')"
        :disabled="disabled"
        @update:model-value="setCred('access_key_id', String($event))"
      />
      <WorkbenchGhostField
        :label="$t('upstreamConnector.authAwsSecretKey')"
        :model-value="cred('secret_access_key')"
        :disabled="disabled"
        @update:model-value="setCred('secret_access_key', String($event))"
      />
      <WorkbenchGhostField
        :label="$t('upstreamConnector.authAwsSession')"
        :model-value="cred('session_token')"
        :disabled="disabled"
        @update:model-value="setCred('session_token', String($event))"
      />
    </template>

    <template v-else-if="group === 'hawk'">
      <WorkbenchGhostField
        :label="$t('upstreamConnector.authHawkId')"
        :model-value="cred('hawk_id')"
        :disabled="disabled"
        @update:model-value="setCred('hawk_id', String($event))"
      />
      <WorkbenchGhostField
        :label="$t('upstreamConnector.authHawkKey')"
        :model-value="cred('hawk_key')"
        :disabled="disabled"
        @update:model-value="setCred('hawk_key', String($event))"
      />
      <WorkbenchGhostField
        :label="$t('upstreamConnector.authHawkAlgo')"
        kind="select"
        :model-value="flow.hawk.algorithm || 'sha256'"
        :options="hawkAlgoOptions"
        :append-to="appendTo"
        :disabled="disabled"
        @update:model-value="flow.hawk.algorithm = String($event)"
      />
      <WorkbenchGhostField
        :label="$t('upstreamConnector.authHawkExt')"
        :model-value="flow.hawk.ext"
        :disabled="disabled"
        @update:model-value="flow.hawk.ext = String($event)"
      />
    </template>

    <div class="wb-section wb-section--nested">
      <button
        type="button"
        class="wb-section__head is-toggle"
        @click="rawOpen = !rawOpen"
      >
        <span class="wb-section__title">{{
          $t("upstreamConnector.authRawCredentials")
        }}</span>
      </button>
      <div v-show="rawOpen" class="wb-section__body">
        <WorkbenchKvRows
          v-model="credRows"
          secret-values
          empty-compact
          :title="$t('supplier.config.credentials')"
          :add-title="$t('supplier.config.addCredential')"
          :clear-title="$t('upstreamConnector.clearKv')"
          :env="env"
          :append-to="appendTo"
        />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { $t } from "@/locales";
import type { AuthFlowForm } from "@/views/mazbri/suppliers/components/SupplierTransparentConfig.vue";
import JsonTemplateEditor from "@/views/mazbri/suppliers/components/JsonTemplateEditor.vue";
import JsonPathHelper from "@/views/mazbri/suppliers/components/JsonPathHelper.vue";
import WorkbenchKvRows from "../WorkbenchKvRows.vue";
import WorkbenchGhostField from "./WorkbenchGhostField.vue";
import AuthInjectFields from "./AuthInjectFields.vue";
import {
  type AuthPickerId,
  type KvRow,
  normalizeKvRow,
} from "../workbenchHelpers";
import {
  authFieldGroup,
  credValue,
  sessionProfileLabel,
  setCredValue,
} from "../authMatrix";

const props = withDefaults(
  defineProps<{
    picker: AuthPickerId;
    flow: AuthFlowForm;
    adapter?: string;
    env?: Record<string, string>;
    seedJson?: string;
    appendTo?: HTMLElement | string;
    disabled?: boolean;
  }>(),
  {
    adapter: "",
    env: () => ({}),
    seedJson: "",
    appendTo: undefined,
    disabled: false,
  },
);

if (!props.flow.jwt) {
  props.flow.jwt = { algorithm: "HS256", ttl_secs: 300, claims_tpl: '{"sub":"mazbri"}', kid: "" };
}
if (!props.flow.digest) {
  props.flow.digest = { algorithm: "MD5", realm: "" };
}
if (!props.flow.aws) {
  props.flow.aws = { region: "", service: "" };
}
if (!props.flow.hawk) {
  props.flow.hawk = { algorithm: "sha256", ext: "" };
}
if (!Array.isArray(props.flow.credentials)) {
  props.flow.credentials = [];
}

const group = computed(() => authFieldGroup(props.picker));

const sessionLabel = computed(() => {
  const p = sessionProfileLabel(props.flow.profile);
  return p || $t("upstreamConnector.authSessionUnset");
});

const hintText = computed(() => {
  switch (props.picker) {
    case "basic":
      return $t("upstreamConnector.authHintBasic");
    case "api_key":
      return $t("upstreamConnector.authHintApiKey");
    case "oauth_cc_form":
      return $t("upstreamConnector.authHintOauth");
    case "gettoken":
      return $t("upstreamConnector.authHintGettoken");
    case "sign_form":
      return $t("upstreamConnector.authHintSign");
    case "grpc_metadata":
      return $t("upstreamConnector.authHintGrpc");
    case "bearer":
      return $t("upstreamConnector.authHintBearer");
    case "jwt":
      return $t("upstreamConnector.authHintJwt");
    case "digest":
      return $t("upstreamConnector.authHintDigest");
    case "aws_sigv4":
      return $t("upstreamConnector.authHintAws");
    case "hawk":
      return $t("upstreamConnector.authHintHawk");
    default:
      return "";
  }
});

const methodOptions = [
  { label: "GET", value: "GET" },
  { label: "POST", value: "POST" },
];
const digestAlgoOptions = [
  { label: "MD5", value: "MD5" },
  { label: "SHA-256", value: "SHA-256" },
];
const jwtAlgoOptions = ["HS256", "HS384", "HS512", "RS256", "RS384", "RS512"].map(
  (v) => ({ label: v, value: v }),
);
const hawkAlgoOptions = [{ label: "sha256", value: "sha256" }];

const isHsJwt = computed(() =>
  String(props.flow.jwt?.algorithm || "HS256").startsWith("HS"),
);

function cred(key: string): string {
  return credValue(props.flow, key);
}
function setCred(key: string, value: string) {
  setCredValue(props.flow, key, value);
}

const authVariables = computed(() => {
  const keys = props.flow.credentials.map((c) => c.key.trim()).filter(Boolean);
  const extra = keys.map((k) => `credential.${k}`);
  return [...keys, ...extra, "username", "password"];
});

const headerRows = computed({
  get: (): KvRow[] =>
    (props.flow.headers || []).map((row) => normalizeKvRow(row)),
  set: (rows: KvRow[]) => {
    props.flow.headers = rows;
  },
});

const credRows = computed({
  get: (): KvRow[] =>
    (props.flow.credentials || []).map((row) =>
      normalizeKvRow({ key: row.key, value: row.value }),
    ),
  set: (rows: KvRow[]) => {
    props.flow.credentials = rows.map((row) => ({
      key: row.key,
      value: row.value,
    }));
  },
});

const bodyOpen = ref(false);
const rawOpen = ref(false);
</script>

<style scoped lang="scss">
.wb-auth__empty {
  padding: 32px 16px;
  text-align: center;
  font-size: 13px;
  color: var(--wb-text-faint, var(--el-text-color-placeholder));
}
.wb-auth__readonly {
  margin-top: 8px;
  font-size: 12px;
}
.wb-auth__fields {
  display: flex;
  flex-direction: column;
}
.wb-auth__hint {
  margin: 8px 16px 4px;
  font-size: 12px;
  color: var(--wb-text-muted, var(--el-text-color-secondary));
}
.wb-auth__warn {
  margin: 8px 16px;
  font-size: 12px;
  color: var(--el-color-warning);
}
.wb-auth__pem {
  min-height: 72px;
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  font-size: 12px;
}
.wb-section--nested {
  border-bottom: none;
}
</style>
