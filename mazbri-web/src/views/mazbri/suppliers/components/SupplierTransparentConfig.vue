<template>
  <div class="advanced-config-container">
    <ElDivider v-if="section === 'all'" border-style="dashed" class="config-divider" />


    <ElFormItem :label="$t('supplier.config.authTitle')" class="section-switch-item">
      <ElSwitch v-model="config.auth_flow.enabled" />
    </ElFormItem>

    <ElFormItem :label="$t('upstreamConnector.authSessionDefault')">
      <ElSelect
        v-model="config.auth_flow.profile"
        class="control-full"
        :disabled="!config.auth_flow.enabled"
        clearable
        @change="onSupplierProfile"
      >
        <ElOption
          v-for="id in sessionProfiles"
          :key="id"
          :label="profileLabel(id)"
          :value="id"
        />
      </ElSelect>
    </ElFormItem>
    <div :class="{ 'disabled-section-content': !config.auth_flow.enabled }">
      <AuthProfileFields
        :picker="supplierPicker"
        :flow="config.auth_flow"
        :disabled="!config.auth_flow.enabled"
        :seed-json="seedJson"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import {
  AUTH_PROFILES,
  applyPickerDefaults,
  authPickerFromState,
  type AuthPickerId,
} from "@/views/mazbri/upstream-connectors/authMatrix";
import AuthProfileFields from "@/views/mazbri/upstream-connectors/hopp/AuthProfileFields.vue";
import { $t } from "@/locales";

export interface AuthJwtForm {
  algorithm: string;
  ttl_secs: number;
  claims_tpl: string;
  kid: string;
}

export interface AuthDigestForm {
  algorithm: string;
  realm: string;
}

export interface AuthAwsForm {
  region: string;
  service: string;
}

export interface AuthHawkForm {
  algorithm: string;
  ext: string;
}

export interface AuthFlowForm {
  enabled: boolean;
  profile: string;
  token_url: string;
  token_path: string;
  method: string;
  headers: { key: string; value: string }[];
  request_body_tpl: string;
  credentials: { key: string; value: string }[];
  token_jsonpath: string;
  expiry_jsonpath: string;
  inject_to: string;
  inject_key: string;
  inject_value_format: string;
  jwt: AuthJwtForm;
  digest: AuthDigestForm;
  aws: AuthAwsForm;
  hawk: AuthHawkForm;
}

export interface SupplierConfigForm {
  upstream_type: string;

  contract_id?: string;
  auth_flow: AuthFlowForm;
}

withDefaults(
  defineProps<{
    section?: "all" | "auth";
    seedJson?: string;
  }>(),
  { section: "all", seedJson: "" },
);

const config = defineModel<SupplierConfigForm>({ required: true });

if (!config.value.auth_flow.jwt) {
  config.value.auth_flow.jwt = {
    algorithm: "HS256",
    ttl_secs: 300,
    claims_tpl: '{"sub":"mazbri"}',
    kid: "",
  };
}
if (!config.value.auth_flow.digest) {
  config.value.auth_flow.digest = { algorithm: "MD5", realm: "" };
}
if (!config.value.auth_flow.aws) {
  config.value.auth_flow.aws = { region: "", service: "" };
}
if (!config.value.auth_flow.hawk) {
  config.value.auth_flow.hawk = { algorithm: "sha256", ext: "" };
}
if (config.value.auth_flow.profile == null) {
  config.value.auth_flow.profile = "";
}
if (config.value.auth_flow.token_path == null) {
  config.value.auth_flow.token_path = "";
}
if (!Array.isArray(config.value.auth_flow.headers)) {
  const raw = config.value.auth_flow.headers as unknown;
  if (raw && typeof raw === "object" && !Array.isArray(raw)) {
    config.value.auth_flow.headers = Object.entries(
      raw as Record<string, string>,
    ).map(([key, value]) => ({ key, value: String(value ?? "") }));
  } else {
    config.value.auth_flow.headers = [];
  }
}

const sessionProfiles = AUTH_PROFILES.filter((id) => id !== "none");

const supplierPicker = computed(() =>
  authPickerFromState(config.value.auth_flow.profile, config.value.auth_flow.enabled),
);

function profileLabel(id: string): string {
  const map: Record<string, string> = {
    api_key: $t("upstreamConnector.authApiKey"),
    oauth_cc_form: $t("upstreamConnector.authOauthCc"),
    gettoken: $t("upstreamConnector.authGettoken"),
    basic: $t("upstreamConnector.authBasic"),
    sign_form: $t("upstreamConnector.authSignForm"),
    grpc_metadata: $t("upstreamConnector.authGrpcMeta"),
    bearer: $t("upstreamConnector.authBearer"),
    jwt: $t("upstreamConnector.authJwt"),
    digest: $t("upstreamConnector.authDigest"),
    aws_sigv4: $t("upstreamConnector.authAws"),
    hawk: $t("upstreamConnector.authHawk"),
  };
  return map[id] || id;
}

function onSupplierProfile(value: string) {
  const v = String(value || "");
  config.value.auth_flow.profile = v;
  if (v) applyPickerDefaults(v as AuthPickerId, config.value.auth_flow);
}
</script>

<style scoped lang="scss">
.advanced-config-container {
  margin-top: 4px;
  width: 100%;
  max-width: 100%;
  box-sizing: border-box;
}

.kv-block {
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.kv-row {
  display: flex;
  gap: 8px;
  align-items: center;
  width: 100%;

  .el-input {
    flex: 1;
    min-width: 0;
  }
}

.config-divider {
  margin: 12px 0 16px;
}

.section-switch-item {
  :deep(.el-form-item__content) {
    justify-content: flex-start;
  }
}

.disabled-section-content {
  opacity: 0.6;
  pointer-events: none;
  user-select: none;
}

.path-with-helper {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  .el-input {
    flex: 1;
    min-width: 0;
  }
}


.control-full {
  width: 100%;
}

.advanced-config-container {
  :deep(.el-form-item__content) {
    flex: 1;
    min-width: 0;
  }


  :deep(.el-input:not(.inject-key-input)),
  :deep(.el-textarea) {
    width: 100%;
  }

  :deep(.el-select:not(.inject-to-select)) {
    width: 100%;
  }
}





.inject-field-combo {
  display: flex;
  align-items: stretch;
  width: 100%;
  max-width: 100%;
  box-sizing: border-box;
  border: 1px solid var(--el-border-color);
  border-radius: var(--el-border-radius-base);
  background-color: var(--el-fill-color-blank);
  overflow: hidden;
  transition: border-color 0.2s;

  &:hover:not(.is-disabled) {
    border-color: var(--el-border-color-hover);
  }

  &:focus-within:not(.is-disabled) {
    border-color: var(--el-color-primary);
  }

  &.is-disabled {
    background-color: var(--el-disabled-bg-color);
    border-color: var(--el-border-color-lighter);
    cursor: not-allowed;
  }

  .inject-to-select {
    width: 110px;
    min-width: 110px;
    max-width: 110px;
    flex-shrink: 0;
  }

  .inject-key-input {
    flex: 1 1 0;
    min-width: 0;
  }


  :deep(.inject-to-select .el-select__wrapper) {
    height: 100%;
    min-height: var(--el-component-size, 32px);
    box-shadow: none !important;
    border: none !important;
    border-radius: 0 !important;
    background-color: var(--el-fill-color-light) !important;
    padding: 0 10px;
  }

  :deep(.inject-to-select .el-select__wrapper.is-hovering),
  :deep(.inject-to-select .el-select__wrapper.is-focused) {
    box-shadow: none !important;
    background-color: var(--el-fill-color-light) !important;
  }

  :deep(.inject-to-select .el-select__wrapper.is-disabled) {
    background-color: var(--el-disabled-bg-color) !important;
  }

  :deep(.inject-key-input .el-input__wrapper) {
    height: 100%;
    min-height: var(--el-component-size, 32px);
    box-shadow: none !important;
    border: none !important;
    border-radius: 0 !important;
    border-left: 1px solid var(--el-border-color) !important;
    background-color: transparent !important;
  }

  :deep(.inject-key-input .el-input__wrapper.is-focus),
  :deep(.inject-key-input .el-input__wrapper:hover) {
    box-shadow: none !important;
  }

  :deep(.inject-key-input.is-disabled .el-input__wrapper) {
    background-color: transparent !important;
  }
}

.credentials-item {
  :deep(.el-form-item__content) {
    display: block;
    width: 100%;
    max-width: 100%;
  }
}

.credentials-block {
  width: 100%;
  max-width: 100%;
  box-sizing: border-box;
}

.credentials-toolbar {
  display: flex;
  justify-content: flex-end;
  margin-bottom: 8px;
}

.credentials-table {
  width: 100% !important;
  max-width: 100%;
  table-layout: fixed;
  box-sizing: border-box;

  :deep(.el-table__inner-wrapper),
  :deep(.el-table__header-wrapper),
  :deep(.el-table__body-wrapper),
  :deep(table) {
    width: 100% !important;
  }

  :deep(.el-table__cell) {
    padding-left: 8px;
    padding-right: 8px;
  }

  :deep(.el-input) {
    width: 100%;
  }
}

.opacity-50 {
  opacity: 0.5;
}

.mr-1 {
  margin-right: 4px;
}
</style>
