<template>
  <CenteredDialog
    v-model="visible"
    :title="$t('upstreamOnboarding.mode.microservice')"
    width="560px"
    :destroy-on-close="true"
    :close-on-click-modal="false"
    dialog-class="upstream-access-dialog"
    @closed="handleClosed"
  >
    <ElSteps :active="step" finish-status="success" align-center class="steps">
      <ElStep :title="$t('upstreamOnboarding.steps.basic')" />
      <ElStep :title="$t('upstreamOnboarding.steps.route')" />
      <ElStep :title="$t('upstreamOnboarding.steps.confirm')" />
      <ElStep :title="$t('upstreamOnboarding.steps.done')" />
    </ElSteps>

    <div class="wizard-body">

      <div v-show="step === 0" class="panel">
        <p class="hint">{{ $t("upstreamOnboarding.mode.microserviceHint") }}</p>
        <ElForm
          ref="basicFormRef"
          :model="form"
          :rules="basicRules"
          label-width="100px"
          class="wizard-form"
        >
          <ElFormItem :label="$t('upstreamOnboarding.fields.supplierCode')" prop="supplier_code">
            <ElInput v-model="form.supplier_code" placeholder="PARTNER-ACME" class="control-full" />
          </ElFormItem>
          <ElFormItem :label="$t('upstreamOnboarding.fields.name')" prop="name">
            <ElInput
              v-model="form.name"
              class="control-full"
              :placeholder="$t('upstreamOnboarding.placeholders.name')"
            />
          </ElFormItem>
          <ElFormItem :label="$t('upstreamOnboarding.fields.upstreamUrl')" prop="upstream_url">
            <ElInput
              v-model="form.upstream_url"
              class="control-full"
              :placeholder="$t('upstreamOnboarding.placeholders.upstreamUrl')"
              @blur="handleUrlBlur"
            />
          </ElFormItem>
          <ElFormItem :label="$t('upstreamOnboarding.fields.contractId')" prop="contract_id">
            <ElInput
              v-model="form.contract_id"
              class="control-full"
              :placeholder="$t('upstreamOnboarding.placeholders.contractId')"
            />
          </ElFormItem>
          <ElFormItem :label="$t('upstreamOnboarding.fields.timeoutMs')">
            <ElInputNumber
              v-model="form.timeout_ms"
              class="control-full"
              align="left"
              :min="1000"
              :max="300000"
              :step="1000"
              controls-position="right"
            />
          </ElFormItem>
          <ElFormItem :label="$t('upstreamOnboarding.fields.channelCode')" prop="channel_code">
            <ElInput v-model="form.channel_code" placeholder="ACME-DEFAULT" class="control-full" />
          </ElFormItem>
          <ElFormItem :label="$t('upstreamOnboarding.fields.channelName')" prop="channel_name">
            <ElInput
              v-model="form.channel_name"
              class="control-full"
              :placeholder="$t('upstreamOnboarding.placeholders.channelName')"
            />
          </ElFormItem>
          <ElFormItem :label="$t('upstreamOnboarding.fields.pathPrefix')">
            <ElInput v-model="form.path_prefix" placeholder="/" class="control-full" />
          </ElFormItem>
        </ElForm>
      </div>


      <div v-show="step === 1" class="panel">
        <ElForm :model="form" label-width="100px" class="wizard-form">
          <ElFormItem :label="$t('upstreamOnboarding.fields.productId')" required>
            <ElInput v-model="form.product_id" placeholder="8012345678" class="control-full" />
          </ElFormItem>
          <ElFormItem :label="$t('upstreamOnboarding.fields.weight')">
            <ElInputNumber
              v-model="form.weight"
              class="control-full"
              align="left"
              :min="0"
              :max="100"
              controls-position="right"
            />
          </ElFormItem>
          <ElFormItem :label="$t('upstreamOnboarding.fields.priority')">
            <ElInputNumber
              v-model="form.priority"
              class="control-full"
              align="left"
              :min="1"
              :max="1000"
              controls-position="right"
            />
          </ElFormItem>
        </ElForm>
      </div>


      <div v-show="step === 2" class="panel">
        <ElAlert
          v-if="dryRunResult"
          :type="dryRunResult.warnings?.length ? 'warning' : 'success'"
          :title="$t('upstreamOnboarding.confirm.dryRunOk')"
          show-icon
          :closable="false"
          class="mb"
        />
        <pre v-if="dryRunResult?.plan" class="plan">{{ formatPlan(dryRunResult.plan) }}</pre>
        <ElEmpty v-else :description="$t('upstreamOnboarding.confirm.needDryRun')" />
      </div>


      <div v-show="step === 3" class="panel success-panel">
        <ElResult
          icon="success"
          :title="$t('upstreamOnboarding.done.title')"
          :sub-title="
            $t('upstreamOnboarding.done.subtitle', {
              version: result?.route_version ?? '-',
            })
          "
        />
        <ElDescriptions v-if="result" :column="1" border class="mb">
          <ElDescriptionsItem label="supplier_id">{{ result.supplier_id }}</ElDescriptionsItem>
          <ElDescriptionsItem label="channel_id">{{ result.channel_id }}</ElDescriptionsItem>
          <ElDescriptionsItem label="route_rule_id">{{ result.route_rule_id }}</ElDescriptionsItem>
          <ElDescriptionsItem label="binding_id">{{ result.binding_id }}</ElDescriptionsItem>
          <ElDescriptionsItem label="route_version">{{ result.route_version }}</ElDescriptionsItem>
        </ElDescriptions>
        <p class="hint">{{ $t("upstreamOnboarding.done.reloadHint") }}</p>
        <template v-if="curlSample">
          <ElInput
            :model-value="curlSample"
            type="textarea"
            :rows="5"
            readonly
            class="mb"
          />
          <ElButton type="primary" @click="copyCurl">
            {{ $t("upstreamOnboarding.done.copyCurl") }}
          </ElButton>
        </template>
        <ElAlert
          v-else
          type="warning"
          :closable="false"
          :title="$t('upstreamOnboarding.done.gatewayUrlMissing')"
        />
      </div>
    </div>

    <template #footer>
      <div v-if="step < 3" class="actions">
        <ElButton :disabled="step === 0 || submitting" @click="step -= 1">
          {{ $t("common.actions.prev") }}
        </ElButton>
        <ElButton v-if="step < 2" type="primary" @click="nextStep">
          {{ $t("common.actions.next") }}
        </ElButton>
        <template v-else>
          <ElButton :loading="submitting" @click="runDryRun">
            {{ $t("upstreamOnboarding.actions.dryRun") }}
          </ElButton>
          <ElButton
            type="primary"
            :loading="submitting"
            :disabled="!dryRunResult"
            @click="submit"
          >
            {{ $t("upstreamOnboarding.actions.submit") }}
          </ElButton>
        </template>
      </div>
      <div v-else class="actions">
        <ElButton type="primary" @click="visible = false">
          {{ $t("common.actions.confirm") }}
        </ElButton>
      </div>
    </template>
  </CenteredDialog>
</template>

<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import { ElMessage, type FormInstance, type FormRules } from "element-plus";
import { $t } from "@/locales";
import CenteredDialog from "@/components/common/CenteredDialog.vue";
import {
  createOnboarding,
  dryRunOnboarding,
  type OnboardingRequest,
  type OnboardingResult,
} from "@/api/upstream-onboarding";
import { fetchSystemSettings } from "@/api/settings";
import { useSettingStore } from "@/store/modules/setting";
import { stripUrlScheme } from "@/utils/sensitive";


defineOptions({ name: "UpstreamAccessWizard" });

const visible = defineModel<boolean>({ default: false });
const emit = defineEmits<{ success: [] }>();

const step = ref(0);
const submitting = ref(false);
const dryRunResult = ref<OnboardingResult | null>(null);
const result = ref<OnboardingResult | null>(null);
const basicFormRef = ref<FormInstance>();

const form = reactive({
  supplier_code: "",
  name: "",
  upstream_url: "",
  contract_id: "",
  timeout_ms: 8000,
  channel_code: "",
  channel_name: "",
  path_prefix: "/",
  product_id: "",
  weight: 100,
  priority: 100,
});

const basicRules = computed<FormRules>(() => ({
  supplier_code: [
    {
      required: true,
      message: $t("upstreamOnboarding.validation.basicRequired"),
      trigger: "blur",
    },
  ],
  name: [
    {
      required: true,
      message: $t("upstreamOnboarding.validation.basicRequired"),
      trigger: "blur",
    },
  ],
  upstream_url: [
    {
      required: true,
      message: $t("upstreamOnboarding.validation.basicRequired"),
      trigger: "blur",
    },
  ],
  channel_code: [
    {
      required: true,
      message: $t("upstreamOnboarding.validation.channelCode"),
      trigger: "blur",
    },
  ],
  channel_name: [
    {
      required: true,
      message: $t("upstreamOnboarding.validation.basicRequired"),
      trigger: "blur",
    },
  ],
  contract_id: [
    {
      required: true,
      validator: (_rule, value, callback) => {
        if (!String(value || "").trim()) {
          callback(
            new Error($t("upstreamOnboarding.validation.contractIdRequired")),
          );
        } else {
          callback();
        }
      },
      trigger: "blur",
    },
  ],
}));

watch(visible, (open) => {
  if (open) {
    resetAll();
    void refreshTenantGatewayUrl();
  }
});

function resetAll() {
  step.value = 0;
  dryRunResult.value = null;
  result.value = null;
  form.supplier_code = "";
  form.name = "";
  form.upstream_url = "";
  form.contract_id = "";
  form.timeout_ms = 8000;
  form.channel_code = "";
  form.channel_name = "";
  form.path_prefix = "/";
  form.product_id = "";
  form.weight = 100;
  form.priority = 100;
}

function handleUrlBlur() {
  form.upstream_url = stripUrlScheme(form.upstream_url);
}

function buildPayload(dryRun: boolean): OnboardingRequest {
  const contractId = form.contract_id.trim();
  return {
    supplier: {
      supplier_code: form.supplier_code.trim(),
      name: form.name.trim(),
      upstream_type: "microservice",
      upstream_url: form.upstream_url.trim(),
      status: "active",
      config: {
        upstream_type: "microservice",
        timeouts: { total_ms: form.timeout_ms },
        ...(contractId ? { contract_id: contractId } : {}),
      },
    },
    channel: {
      channel_code: form.channel_code.trim().toUpperCase(),
      name: form.channel_name.trim() || form.name.trim(),
      path_prefix: form.path_prefix.trim() || "/",
      cost_price: "0",
      qps_limit: 0,
    },
    route: {
      product_ref: { product_id: form.product_id.trim() },
      tenant_id: null,
      priority: form.priority,
      weight: form.weight,
      version_tag: "v1.0",
    },
    options: {
      dry_run: dryRun,
      fail_if_route_exists: true,
    },
  };
}

async function nextStep() {
  if (step.value === 0) {
    const ok = await basicFormRef.value?.validate().catch(() => false);
    if (!ok) return;
    if (!/^[A-Z][A-Z0-9-]*$/.test(form.channel_code.trim().toUpperCase())) {
      ElMessage.error($t("upstreamOnboarding.validation.channelCodePattern"));
      return;
    }
  }
  if (step.value === 1 && !form.product_id.trim()) {
    ElMessage.error($t("upstreamOnboarding.validation.productRequired"));
    return;
  }
  if (step.value === 1) {
    dryRunResult.value = null;
  }
  step.value += 1;
}

async function runDryRun() {
  submitting.value = true;
  try {
    dryRunResult.value = await dryRunOnboarding(buildPayload(true));
    ElMessage.success($t("upstreamOnboarding.confirm.dryRunOk"));
  } finally {
    submitting.value = false;
  }
}

async function submit() {
  submitting.value = true;
  try {
    result.value = await createOnboarding(buildPayload(false));
    step.value = 3;
    ElMessage.success($t("upstreamOnboarding.done.title"));
    emit("success");
  } finally {
    submitting.value = false;
  }
}

function formatPlan(plan: NonNullable<OnboardingResult["plan"]>) {
  return JSON.stringify(plan, null, 2);
}

const settingStore = useSettingStore();
const runtimeGatewayUrl = ref("");

function normalizeGatewayBase(raw: string): string {
  return raw.trim().replace(/\/+$/, "");
}

function resolveGatewayBaseUrl(): string {
  const fromRuntime = normalizeGatewayBase(runtimeGatewayUrl.value);
  if (fromRuntime) return fromRuntime;
  const fromStore = normalizeGatewayBase(settingStore.tenantGatewayUrl || "");
  return fromStore;
}

async function refreshTenantGatewayUrl() {
  try {
    const settings = await fetchSystemSettings();
    const url = normalizeGatewayBase(settings.tenant_gateway_url || "");
    if (url) {
      runtimeGatewayUrl.value = url;
      settingStore.applySystemSettings({
        ...settings,
        tenant_gateway_url: url,
      });
    }
  } catch {
    void 0;
  }
}

const curlSample = computed(() => {
  const product = form.product_id.trim() || "8012345678";
  const gateway = resolveGatewayBaseUrl();
  if (!gateway) return "";
  const pathPrefix = (form.path_prefix || "/").trim() || "/";
  return `curl -sS -X POST '${gateway}/api/v1/gateway' \\
  -H 'Authorization: Bearer <access_token>' \\
  -H 'Content-Type: application/json' \\
  -d '{"product_id":"${product}","method":"POST","path":"${pathPrefix}","body":{}}'

# gateway=${gateway}
# route_version=${result.value?.route_version ?? "N/A"}；热加载约 ≤5s
# 修改地址：控制台 → 系统设置 → 租户网关地址（tenant_gateway_url）`;
});

async function copyCurl() {
  try {
    await navigator.clipboard.writeText(curlSample.value);
    ElMessage.success($t("common.feedback.copySuccess"));
  } catch {
    ElMessage.error($t("common.feedback.copyFailed"));
  }
}

function handleClosed() {
  if (result.value) {
    emit("success");
  }
}

defineExpose({
  open: () => {
    visible.value = true;
  },
});
</script>

<style scoped lang="scss">
.steps {
  margin: 0 0 16px;
  padding: 0;
  width: 100%;
  max-width: 100%;
  box-sizing: border-box;
  flex-shrink: 0;

  :deep(.el-step__title) {
    font-size: 12px;
    line-height: 1.35;
    max-width: 100%;
  }

  :deep(.el-step__icon) {
    width: 24px;
    height: 24px;
    font-size: 12px;
  }

  :deep(.el-step.is-horizontal .el-step__line) {
    top: 11px;
  }

  :deep(.el-step__head.is-success),
  :deep(.el-step__head.is-process),
  :deep(.el-step__head.is-wait) {
    .el-step__icon.is-text {
      width: 24px;
      height: 24px;
    }
  }
}

.wizard-body {
  min-height: 280px;
}

.panel {
  min-width: 0;
}

.hint {
  margin: 0 0 12px;
  color: var(--el-text-color-secondary);
  font-size: 13px;
  line-height: 1.5;
}

.wizard-form {
  .control-full {
    width: 100%;
  }

  :deep(.el-input-number) {
    width: 100%;
  }
}

.plan {
  margin: 0;
  padding: 12px;
  max-height: 240px;
  overflow: auto;
  font-size: 12px;
  background: var(--el-fill-color-light);
  border-radius: 6px;
  white-space: pre-wrap;
  word-break: break-all;
}

.mb {
  margin-bottom: 12px;
}

.success-panel {
  text-align: left;
}

.actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  width: 100%;
}
</style>
