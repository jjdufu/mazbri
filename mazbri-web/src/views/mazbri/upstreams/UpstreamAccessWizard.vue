<template>
  <CenteredDialog
    v-model="visible"
    :title="$t('upstreamOnboarding.mode.microservice')"
    width="600px"
    :destroy-on-close="true"
    :close-on-click-modal="false"
    dialog-class="upstream-access-dialog"
  >
    <ElSteps :active="step" finish-status="success" align-center class="steps">
      <ElStep :title="$t('upstreamOnboarding.steps.basic')" />
      <ElStep :title="$t('upstreamOnboarding.steps.route')" />
      <ElStep :title="$t('upstreamOnboarding.steps.confirm')" />
      <ElStep :title="$t('upstreamOnboarding.steps.done')" />
    </ElSteps>

    <div class="wizard-body">
      <div v-show="step === 0" class="panel">
        <p class="hint">
          {{ $t("upstreamOnboarding.mode.microserviceHint") }}
        </p>
        <ElForm
          ref="basicFormRef"
          :model="form"
          :rules="basicRules"
          label-width="124px"
          class="wizard-form"
        >
          <ElFormItem
            :label="$t('upstreamOnboarding.fields.supplierCode')"
            prop="supplier_code"
          >
            <ElInput
              v-model="form.supplier_code"
              placeholder="PARTNER-ACME"
              class="control-full"
            />
          </ElFormItem>
          <ElFormItem :label="$t('upstreamOnboarding.fields.name')" prop="name">
            <ElInput
              v-model="form.name"
              class="control-full"
              :placeholder="$t('upstreamOnboarding.placeholders.name')"
            />
          </ElFormItem>
          <ElFormItem
            :label="$t('upstreamOnboarding.fields.upstreamUrl')"
            prop="upstream_url"
          >
            <ElInput
              v-model="form.upstream_url"
              class="control-full"
              :placeholder="$t('upstreamOnboarding.placeholders.upstreamUrl')"
              @blur="handleUrlBlur"
            />
          </ElFormItem>
          <ElFormItem
            :label="$t('upstreamOnboarding.fields.contractId')"
            prop="contract_id"
          >
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
          <ElFormItem
            :label="$t('upstreamOnboarding.fields.channelCode')"
            prop="channel_code"
          >
            <ElInput
              v-model="form.channel_code"
              placeholder="ACME-DEFAULT"
              class="control-full"
            />
          </ElFormItem>
          <ElFormItem
            :label="$t('upstreamOnboarding.fields.channelName')"
            prop="channel_name"
          >
            <ElInput
              v-model="form.channel_name"
              class="control-full"
              :placeholder="$t('upstreamOnboarding.placeholders.channelName')"
            />
          </ElFormItem>
          <ElFormItem :label="$t('upstreamOnboarding.fields.pathPrefix')">
            <ElInput
              v-model="form.path_prefix"
              placeholder="/"
              class="control-full"
            />
          </ElFormItem>
        </ElForm>
      </div>

      <div v-show="step === 1" class="panel">
        <ElForm :model="form" label-width="124px" class="wizard-form">
          <ElFormItem
            :label="$t('upstreamOnboarding.fields.productId')"
            required
          >
            <ElSelect
              v-model="form.product_id"
              filterable
              remote
              reserve-keyword
              :remote-method="searchProducts"
              :loading="productLoading"
              :placeholder="$t('upstreamOnboarding.placeholders.productSelect')"
              class="control-full"
              @focus="ensureProductsLoaded"
            >
              <ElOption
                v-for="item in productOptions"
                :key="item.id"
                :label="`${item.name} (${item.product_id})`"
                :value="item.product_id"
              />
            </ElSelect>
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
        <template v-if="dryRunResult?.plan">
          <ElAlert
            :type="dryRunResult.warnings?.length ? 'warning' : 'success'"
            :title="$t('upstreamOnboarding.confirm.dryRunOk')"
            show-icon
            :closable="false"
            class="mb"
          />
          <ElAlert
            v-if="dryRunResult.warnings?.length"
            type="warning"
            :title="$t('upstreamOnboarding.confirm.warningsTitle')"
            :closable="false"
            class="mb"
          >
            <ul class="warning-list">
              <li v-for="(warn, idx) in dryRunResult.warnings" :key="idx">
                {{ warn }}
              </li>
            </ul>
          </ElAlert>
          <ElDescriptions
            :title="$t('upstreamOnboarding.confirm.summaryTitle')"
            :column="1"
            border
          >
            <ElDescriptionsItem
              :label="$t('upstreamOnboarding.fields.supplierCode')"
            >
              {{ form.name.trim() }} ({{ dryRunResult.plan.supplier_code }})
            </ElDescriptionsItem>
            <ElDescriptionsItem
              :label="$t('upstreamOnboarding.confirm.upstreamType')"
            >
              {{
                dryRunResult.plan.upstream_type === "microservice"
                  ? $t("supplier.upstreamOptions.microservice")
                  : $t("supplier.upstreamOptions.transparent")
              }}
              · {{ dryRunResult.plan.upstream_url }}
            </ElDescriptionsItem>
            <ElDescriptionsItem
              :label="$t('upstreamOnboarding.fields.channelCode')"
            >
              {{ (form.channel_name || form.name).trim() }} ({{
                dryRunResult.plan.channel_code
              }}) · {{ form.path_prefix.trim() || "/" }}
            </ElDescriptionsItem>
            <ElDescriptionsItem
              :label="$t('upstreamOnboarding.confirm.targetProduct')"
            >
              {{ selectedProductSummary }}
            </ElDescriptionsItem>
            <ElDescriptionsItem
              :label="`${$t('upstreamOnboarding.fields.priority')} / ${$t('upstreamOnboarding.fields.weight')}`"
            >
              {{ form.priority }} / {{ form.weight }}
            </ElDescriptionsItem>
            <ElDescriptionsItem
              :label="$t('upstreamOnboarding.confirm.plannedActions')"
            >
              <div class="action-tags">
                <ElTag
                  v-for="action in dryRunResult.plan.actions"
                  :key="action"
                  size="small"
                  effect="light"
                  :type="action.startsWith('skip') ? 'info' : 'primary'"
                >
                  {{ formatPlanAction(action) }}
                </ElTag>
              </div>
            </ElDescriptionsItem>
          </ElDescriptions>
        </template>
        <ElEmpty
          v-else
          :description="$t('upstreamOnboarding.confirm.needDryRun')"
        />
      </div>

      <div v-show="step === 3" class="panel success-panel">
        <ElResult
          icon="success"
          :title="$t('upstreamOnboarding.done.title')"
          :sub-title="
            $t('upstreamOnboarding.done.subtitle', {
              version: result?.route_version ?? '—',
            })
          "
        />
        <ElDescriptions v-if="result" :column="1" border class="mb">
          <ElDescriptionsItem
            :label="$t('upstreamOnboarding.done.supplierId')"
          >
            <CopyableText
              :value="result.supplier_id"
              :label="$t('upstreamOnboarding.done.supplierId')"
            />
          </ElDescriptionsItem>
          <ElDescriptionsItem :label="$t('upstreamOnboarding.done.channelId')">
            <CopyableText
              :value="result.channel_id"
              :label="$t('upstreamOnboarding.done.channelId')"
            />
          </ElDescriptionsItem>
          <ElDescriptionsItem
            :label="$t('upstreamOnboarding.done.routeRuleId')"
          >
            <CopyableText
              :value="result.route_rule_id"
              :label="$t('upstreamOnboarding.done.routeRuleId')"
            />
          </ElDescriptionsItem>
          <ElDescriptionsItem :label="$t('upstreamOnboarding.done.bindingId')">
            <CopyableText
              :value="result.binding_id"
              :label="$t('upstreamOnboarding.done.bindingId')"
            />
          </ElDescriptionsItem>
          <ElDescriptionsItem
            :label="$t('upstreamOnboarding.done.routeVersion')"
          >
            {{ result.route_version }}
          </ElDescriptionsItem>
        </ElDescriptions>
        <p class="hint">{{ $t("upstreamOnboarding.done.reloadHint") }}</p>
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
import { fetchProductList } from "@/api/product";
import {
  createOnboarding,
  dryRunOnboarding,
  type OnboardingRequest,
  type OnboardingResult,
} from "@/api/upstream-onboarding";
import CenteredDialog from "@/components/common/CenteredDialog.vue";
import CopyableText from "@/components/common/copyable-text/index.vue";
import { $t } from "@/locales";
import type { Product } from "@/types/api/product";
import { stripUrlScheme } from "@/utils/sensitive";

defineOptions({ name: "UpstreamAccessWizard" });

const visible = defineModel<boolean>({ default: false });
const emit = defineEmits<{ success: [] }>();

const step = ref(0);
const submitting = ref(false);
const productLoading = ref(false);
const productOptions = ref<Product[]>([]);
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

const selectedProductSummary = computed(() => {
  const externalId =
    dryRunResult.value?.plan?.product_external_id || form.product_id.trim();
  const matched = productOptions.value.find(
    (item) => item.product_id === externalId,
  );
  if (matched) {
    return `${matched.name} (${matched.product_id})`;
  }
  return externalId || "—";
});

async function searchProducts(keyword = "") {
  productLoading.value = true;
  try {
    const res = await fetchProductList({
      page: 1,
      page_size: 20,
      q: keyword.trim() || undefined,
    });
    productOptions.value = (res.products ?? []).filter((item) => item.enabled);
  } catch {
    productOptions.value = [];
  } finally {
    productLoading.value = false;
  }
}

function ensureProductsLoaded() {
  if (productOptions.value.length === 0 && !productLoading.value) {
    void searchProducts("");
  }
}

function formatPlanAction(action: string): string {
  switch (action) {
    case "insert suppliers":
      return $t("upstreamOnboarding.confirm.actionInsertSuppliers");
    case "insert channels":
      return $t("upstreamOnboarding.confirm.actionInsertChannels");
    case "insert route_rules":
      return $t("upstreamOnboarding.confirm.actionInsertRouteRules");
    case "insert route_channel_bindings":
      return $t("upstreamOnboarding.confirm.actionInsertRouteBindings");
    case "grant tenant_products":
      return $t("upstreamOnboarding.confirm.actionGrantTenantProducts");
    case "skip authorization":
      return $t("upstreamOnboarding.confirm.actionSkipAuthorization");
    default:
      return action;
  }
}

watch(visible, (open) => {
  if (open) {
    resetAll();
  }
});

function resetAll() {
  step.value = 0;
  dryRunResult.value = null;
  result.value = null;
  productOptions.value = [];
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
    ensureProductsLoaded();
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

.action-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.warning-list {
  margin: 4px 0 0;
  padding-left: 18px;
  font-size: 12px;
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
