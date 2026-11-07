<template>
  <AdaptiveDrawer v-model="visible" :title="$t('upstreamConnector.publish')" size="560px">
    <template v-if="result">
      <ElAlert
        type="success"
        :title="$t('upstreamConnector.publishOk')"
        :description="resultDesc"
        show-icon
        class="mb-3"
      />
      <ElForm label-width="120px">
        <ElFormItem :label="$t('upstreamConnector.fields.supplierCode')">
          {{ form.supplier_code }}
        </ElFormItem>
        <ElFormItem :label="$t('upstreamConnector.fields.versionNo')">
          v{{ result.version_no }}
        </ElFormItem>
        <ElFormItem
          v-if="result.sample_curl"
          :label="$t('upstreamConnector.sampleCurl')"
        >
          <pre class="wb-pub__curl">{{ result.sample_curl }}</pre>
        </ElFormItem>
      </ElForm>
      <ElButton type="primary" @click="visible = false">{{ $t("common.actions.close") }}</ElButton>
    </template>
    <template v-else>
      <ElForm label-width="120px">
        <ElFormItem :label="$t('upstreamConnector.publishMode')">
          <HoppSelect
            :model-value="form.mode"
            :options="modeOptions"
            block
            @update:model-value="form.mode = $event"
          />
        </ElFormItem>
        <ElFormItem :label="$t('upstreamConnector.fields.supplierCode')" required>
          <ElInput v-model="form.supplier_code" />
        </ElFormItem>
        <ElFormItem :label="$t('upstreamConnector.fields.name')" required>
          <ElInput v-model="form.supplier_name" />
        </ElFormItem>
        <ElFormItem
          v-if="form.mode === 'full_onboarding'"
          :label="$t('upstreamConnector.fields.product')"
          required
        >
          <ElSelect
            v-model="form.product_id"
            filterable
            remote
            :remote-method="searchProducts"
            :loading="productLoading"
            class="control-full"
            :placeholder="$t('upstreamConnector.productPlaceholder')"
          >
            <ElOption
              v-for="p in products"
              :key="p.product_id"
              :label="`${p.name} (${p.product_id})`"
              :value="p.product_id"
            />
          </ElSelect>
        </ElFormItem>
      </ElForm>

      <div
        v-if="form.mode !== 'supplier_only'"
        class="wb-pub__section"
      >
        <div class="wb-pub__h">{{ $t("upstreamConnector.channelMapping") }}</div>
        <ElTable :data="channels" size="small" class="wb-pub__table">
          <ElTableColumn :label="$t('upstreamConnector.fields.epName')" min-width="100">
            <template #default="{ row }">{{ row.name }}</template>
          </ElTableColumn>
          <ElTableColumn :label="$t('upstreamConnector.fields.channelCode')" min-width="120">
            <template #default="{ row }">
              <ElInput v-model="row.channel_code" size="small" />
            </template>
          </ElTableColumn>
          <ElTableColumn :label="$t('upstreamConnector.fields.pathPrefix')" min-width="110">
            <template #default="{ row }">
              <ElInput v-model="row.path_prefix" size="small" />
            </template>
          </ElTableColumn>
        </ElTable>
      </div>

      <ElAlert
        v-if="unresolvedVars.length"
        type="error"
        :title="$t('upstreamConnector.unresolvedVarsBlock', { vars: unresolvedVars.join(', ') })"
        show-icon
        :closable="false"
        class="mb-3"
      />
      <ElAlert
        v-if="missingAdapterSummary.length"
        type="error"
        show-icon
        :closable="false"
        class="mb-3"
      >
        <template #title>{{ $t("upstreamConnector.adapterFieldsMissingPublish") }}</template>
        <ul class="wb-pub__missing">
          <li v-for="row in missingAdapterSummary" :key="row.id">
            <strong>{{ row.name }}</strong>:
            <code v-for="f in row.fields" :key="f.path" class="wb-pub__field">{{ f.path }}</code>
          </li>
        </ul>
      </ElAlert>
      <ElAlert
        v-if="leftoverTokens.length"
        type="warning"
        :title="$t('upstreamConnector.staticTokenHint', { headers: leftoverTokens.join(', ') })"
        show-icon
        :closable="false"
        class="mb-3"
      />
      <ElAlert
        v-if="hasJobPollEndpoint"
        type="warning"
        :title="$t('upstreamConnector.jobPollRiskPublish')"
        show-icon
        :closable="false"
        class="mb-3"
      />

      <div class="wb-pub__section">
        <div class="wb-pub__h">{{ $t("upstreamConnector.gateChecklist") }}</div>
        <ElTag
          size="small"
          :type="gate.ok === gate.total ? 'success' : 'warning'"
          effect="plain"
        >
          {{ gate.ok }}/{{ gate.total }}
        </ElTag>
        <ul class="wb-pub__gates">
          <li v-for="g in gate.items.filter((x) => x.gated)" :key="g.id">
            <span :class="{ ok: g.ok, fail: !g.ok }">{{ g.ok ? "✓" : "✗" }}</span>
            {{ g.name }}
            <span v-if="!g.ok && g.reason" class="wb-pub__reason">
              {{ gateReasonText(g.reason) }}
            </span>
          </li>
        </ul>
      </div>

      <ElForm label-width="120px">
        <ElFormItem :label="$t('upstreamConnector.requireGate')">
          <ElSwitch v-model="form.require_gate" />
        </ElFormItem>
        <ElFormItem
          v-if="showForce"
          :label="$t('upstreamConnector.forcePublish')"
        >
          <ElSwitch v-model="form.force_publish" />
        </ElFormItem>
        <ElFormItem
          v-if="showForce && form.force_publish"
          :label="$t('upstreamConnector.forceReason')"
        >
          <ElInput v-model="form.force_reason" type="textarea" />
        </ElFormItem>
        <ElFormItem :label="$t('upstreamConnector.comment')">
          <ElInput v-model="form.comment" />
        </ElFormItem>
      </ElForm>
      <div class="drawer-footer">
        <ElButton
          type="primary"
          :loading="publishing"
          :disabled="unresolvedVars.length > 0 || missingAdapterSummary.length > 0"
          @click="emit('confirm')"
        >
          {{ $t("upstreamConnector.publish") }}
        </ElButton>
      </div>
    </template>
  </AdaptiveDrawer>
</template>

<script setup lang="ts">
import { useI18n } from "vue-i18n";
import AdaptiveDrawer from "@/components/common/AdaptiveDrawer.vue";
import HoppSelect from "@/components/common/hopp-select/index.vue";
import { fetchProductList } from "@/api/product";
import type { Product } from "@/types/api/product";
import type { PublishResult } from "@/api/upstream-connectors";
import type { ConnectorEndpoint } from "@/api/upstream-connectors";
import {
  listMissingAdapterFields,
  type ChannelDraft,
  type GateEpStatus,
  type MissingAdapterField,
} from "./workbenchHelpers";

export type PublishFormModel = {
  mode: string;
  supplier_code: string;
  supplier_name: string;
  product_id: string;
  require_gate: boolean;
  force_publish: boolean;
  force_reason: string;
  comment: string;
};

const props = defineProps<{
  publishing: boolean;
  result?: PublishResult | null;
  resultDesc?: string;
  gate: { ok: number; total: number; items: GateEpStatus[] };
  canForcePublish: boolean;
  leftoverTokens: string[];
  unresolvedVars: string[];
  endpoints?: ConnectorEndpoint[];
}>();

const visible = defineModel<boolean>({ required: true });
const form = defineModel<PublishFormModel>("form", { required: true });
const channels = defineModel<ChannelDraft[]>("channels", { required: true });

const emit = defineEmits<{
  confirm: [];
}>();

const { t } = useI18n();

const gateAllMet = computed(
  () => props.gate.total === 0 || props.gate.ok === props.gate.total,
);

const showForce = computed(
  () => props.canForcePublish && form.value.require_gate && !gateAllMet.value,
);

function gateReasonText(reason: string) {
  if (reason === "spec_changed") return t("upstreamConnector.gateReasonSpecChanged");
  if (reason === "unresolved_vars") return t("upstreamConnector.gateReasonUnresolved");
  if (reason === "not_run") return t("upstreamConnector.gateReasonNotRun");
  if (reason === "spec_invalid") return t("upstreamConnector.gateReasonSpecInvalid");
  return "";
}

type MissingRow = {
  id: string;
  name: string;
  fields: MissingAdapterField[];
};

const hasJobPollEndpoint = computed(() =>
  (props.endpoints || []).some((ep) => (ep.adapter || "http_json") === "job_poll"),
);

const missingAdapterSummary = computed((): MissingRow[] => {
  const eps = props.endpoints || [];
  const out: MissingRow[] = [];
  for (let i = 0; i < eps.length; i++) {
    const ep = eps[i];
    if (ep.gate === false) continue;
    const fields = listMissingAdapterFields(ep);
    if (!fields.length) continue;
    const id = ep.id || `ep-${i + 1}`;
    out.push({
      id,
      name: ep.name || ep.path || id,
      fields,
    });
  }
  return out;
});

watch(
  () => form.value.require_gate,
  (on) => {
    if (!on) {
      form.value.force_publish = false;
      form.value.force_reason = "";
    }
  },
);

watch(showForce, (show) => {
  if (!show) {
    form.value.force_publish = false;
    form.value.force_reason = "";
  }
});

const modeOptions = computed(() => [
  { value: "supplier_only", label: t("upstreamConnector.modeSupplierOnly") },
  {
    value: "supplier_channels",
    label: t("upstreamConnector.modeSupplierChannels"),
  },
  { value: "full_onboarding", label: t("upstreamConnector.modeFull") },
]);

const products = ref<Product[]>([]);
const productLoading = ref(false);

async function searchProducts(q: string) {
  productLoading.value = true;
  try {
    const r = await fetchProductList({ q: q || undefined, page_size: 30 });
    products.value = r.products || [];
  } finally {
    productLoading.value = false;
  }
}

watch(visible, (v) => {
  if (v && form.value.mode === "full_onboarding" && !products.value.length) {
    void searchProducts("");
  }
});
</script>

<style scoped lang="scss">
.control-full {
  width: 100%;
}
.drawer-footer {
  margin-top: 16px;
}
.mb-3 {
  margin-bottom: 12px;
}
.wb-pub__section {
  margin: 12px 0 16px;
}
.wb-pub__h {
  font-weight: 600;
  font-size: 13px;
  margin-bottom: 8px;
}
.wb-pub__table {
  width: 100%;
}
.wb-pub__gates {
  list-style: none;
  padding: 8px 0 0;
  margin: 0;
  font-size: 12px;
  li {
    padding: 2px 0;
  }
  .ok {
    color: var(--el-color-success);
  }
  .fail {
    color: var(--el-color-danger);
  }
  .wb-pub__reason {
    color: var(--el-text-color-secondary);
    margin-left: 6px;
  }
}
.wb-pub__missing {
  margin: 6px 0 0;
  padding-left: 18px;
  font-size: 12px;
  li {
    margin: 2px 0;
  }
}
.wb-pub__field {
  margin-left: 6px;
  font-size: 11px;
}
.wb-pub__curl {
  font-size: 11px;
  white-space: pre-wrap;
  word-break: break-all;
  margin: 0;
  background: var(--el-fill-color-light);
  padding: 8px;
  border-radius: var(--wb-radius, var(--art-form-control-radius));
}
</style>
