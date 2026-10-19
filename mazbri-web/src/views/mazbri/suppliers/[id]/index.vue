<template>
  <div class="supplier-detail-page">
    <ElPageHeader @back="handleBack">
      <template #content>
        <div class="page-header-content">
          <span class="text-lg font-medium">{{
            displayName || $t("supplier.detail.pageTitle")
          }}</span>
          <ElTag
            v-if="!loading"
            size="small"
            :type="upstreamType === 'transparent' ? 'primary' : 'info'"
            effect="plain"
          >
            {{ upstreamTypeLabel }}
          </ElTag>
          <ElTag v-if="editing" size="small" type="warning" effect="plain">
            {{ $t("supplier.detail.editingBadge") }}
          </ElTag>
        </div>
      </template>
      <template #extra>

        <div class="header-extra">
          <template v-if="editing">
            <ElButton @click="cancelEdit">{{
              $t("supplier.form.cancelBack")
            }}</ElButton>
            <ElButton type="primary" :loading="submitting" @click="submitForm">
              {{ $t("supplier.form.saveAndBack") }}
            </ElButton>
          </template>
          <template v-else>
            <ElTag :type="statusValue === 'active' ? 'success' : 'info'">
              {{ statusLabel }}
            </ElTag>
            <ElButton
              v-if="upstreamType === 'transparent'"
              v-roles="['R_ADMIN', 'R_OPERATOR']"
              :loading="openingWorkbench"
              @click="openWorkbenchEdit"
            >
              {{ $t("upstreamConnector.editInWorkbench") }}
            </ElButton>
            <ElButton
              v-roles="['R_ADMIN', 'R_OPERATOR']"
              type="primary"
              @click="enterEdit"
            >
              {{ $t("common.actions.modify") }}
            </ElButton>
            <ElButton
              v-roles="['R_ADMIN', 'R_OPERATOR']"
              type="danger"
              @click="handleDelete"
            >
              <template #icon>
                <ArtSvgIcon icon="ri:delete-bin-line" />
              </template>
              {{ $t("common.actions.delete") }}
            </ElButton>
          </template>
        </div>
      </template>
    </ElPageHeader>

    <ElForm
      ref="formRef"
      v-loading="loading"
      :model="form"
      :rules="rules"
      class="detail-form"
      @submit.prevent
    >

      <ElCard class="mt-5" shadow="never">
        <template #header>
          <SurfaceCardHeader :title="$t('supplier.detail.basicInfoTitle')" />
        </template>

        <ElAlert
          v-if="editing"
          type="info"
          :closable="false"
          show-icon
          class="type-locked-alert"
          :title="$t('supplier.form.typeLockedHint')"
        />

        <ElDescriptions class="detail-descriptions" :column="2" border>
          <ElDescriptionsItem :label="$t('supplier.fields.id')">
            {{ supplier?.id || "-" }}
          </ElDescriptionsItem>

          <ElDescriptionsItem :label="$t('supplier.fields.upstreamType')">
            <ElTag
              size="small"
              :type="upstreamType === 'transparent' ? 'primary' : 'info'"
              effect="plain"
            >
              {{ upstreamTypeLabel }}
            </ElTag>
          </ElDescriptionsItem>

          <ElDescriptionsItem :label="$t('supplier.fields.code')">
            <ElFormItem
              v-if="editing"
              prop="supplier_code"
              class="inline-form-item"
            >
              <ElInput
                v-model="form.supplier_code"
                :placeholder="$t('supplier.form.codePlaceholder')"
              />
            </ElFormItem>
            <span v-else>{{ supplier?.supplier_code || "-" }}</span>
          </ElDescriptionsItem>

          <ElDescriptionsItem :label="$t('supplier.fields.name')">
            <ElFormItem v-if="editing" prop="name" class="inline-form-item">
              <ElInput
                v-model="form.name"
                :placeholder="$t('supplier.form.namePlaceholder')"
              />
            </ElFormItem>
            <span v-else>{{ supplier?.name || "-" }}</span>
          </ElDescriptionsItem>

          <ElDescriptionsItem
            :label="
              upstreamType === 'transparent'
                ? $t('supplier.fields.targetAddress')
                : $t('supplier.fields.accessAddress')
            "
            :span="2"
          >
            <ElFormItem
              v-if="editing"
              prop="upstream_url"
              class="inline-form-item"
            >
              <ElInput
                v-model="form.upstream_url"
                :placeholder="
                  upstreamType === 'transparent'
                    ? $t('supplier.form.targetAddressPlaceholder')
                    : $t('supplier.form.accessAddressPlaceholder')
                "
                @blur="handleUrlBlur"
              />
            </ElFormItem>
            <span v-else>
              {{
                upstreamType === "transparent"
                  ? supplier?.upstream_url || "-"
                  : formatUrlDisplay(supplier?.upstream_url || "")
              }}
            </span>
          </ElDescriptionsItem>

          <ElDescriptionsItem
            v-if="upstreamType !== 'transparent'"
            :label="$t('supplier.fields.contractId')"
          >
            <ElFormItem v-if="editing" class="inline-form-item">
              <ElInput
                v-model="form.config.contract_id"
                :placeholder="$t('supplier.placeholders.contractId')"
              />
            </ElFormItem>
            <span v-else>{{ contractId || "-" }}</span>
          </ElDescriptionsItem>

          <ElDescriptionsItem :label="$t('supplier.fields.status')">
            <ElFormItem v-if="editing" class="inline-form-item">
              <ElSelect v-model="form.status" class="status-select">
                <ElOption
                  :label="$t('common.status.active')"
                  value="active"
                />
                <ElOption
                  :label="$t('common.status.disabled')"
                  value="inactive"
                />
                <ElOption
                  :label="$t('common.status.suspended')"
                  value="suspended"
                />
              </ElSelect>
            </ElFormItem>
            <ElTag
              v-else
              size="small"
              :type="statusValue === 'active' ? 'success' : 'info'"
            >
              {{ statusLabel }}
            </ElTag>
          </ElDescriptionsItem>

          <ElDescriptionsItem :label="$t('common.fields.createdAt')">
            {{ formatDateTime(supplier?.created_at) }}
          </ElDescriptionsItem>
          <ElDescriptionsItem :label="$t('common.fields.updatedAt')">
            {{ formatDateTime(supplier?.updated_at) }}
          </ElDescriptionsItem>
        </ElDescriptions>
      </ElCard>


      <ElCard
        v-if="upstreamType === 'transparent'"
        class="mt-5"
        shadow="never"
      >
        <template #header>
          <SurfaceCardHeader
            :title="$t('supplier.detail.advancedInfoTitle')"
            stack-on-mobile
          />
        </template>


        <div class="config-section">
          <div class="config-section-head">
            <span class="config-section-label">{{
              $t("supplier.detail.authSectionTitle")
            }}</span>
            <div class="config-section-control">
              <ElSwitch
                v-if="editing"
                v-model="form.config.auth_flow.enabled"
              />
              <ElTag
                v-else
                size="small"
                :type="authEnabled ? 'success' : 'info'"
              >
                {{
                  authEnabled
                    ? $t("supplier.detail.enabled")
                    : $t("supplier.detail.disabled")
                }}
              </ElTag>
            </div>
          </div>

          <div
            v-if="editing || authEnabled"
            :class="{
              'section-body': true,
              'is-disabled': editing && !form.config.auth_flow.enabled,
            }"
          >
            <ElDescriptions class="detail-descriptions" :column="2" border>
              <ElDescriptionsItem
                :label="$t('supplier.config.tokenUrl')"
                :span="2"
              >
                <ElInput
                  v-if="editing"
                  v-model="form.config.auth_flow.token_url"
                  placeholder="https://api.upstream.com/get/token"
                  :disabled="!form.config.auth_flow.enabled"
                />
                <span v-else>{{ authFlow?.token_url || "-" }}</span>
              </ElDescriptionsItem>


              <ElDescriptionsItem
                :label="$t('supplier.config.method')"
                :span="2"
              >
                <ElSelect
                  v-if="editing"
                  v-model="form.config.auth_flow.method"
                  class="method-select"
                  :disabled="!form.config.auth_flow.enabled"
                >
                  <ElOption label="POST" value="POST" />
                  <ElOption label="GET" value="GET" />
                </ElSelect>
                <span v-else>{{ authFlow?.method || "-" }}</span>
              </ElDescriptionsItem>


              <ElDescriptionsItem
                :label="$t('supplier.config.credentials')"
                :span="2"
              >
                <template v-if="editing">
                  <div class="credentials-block">
                    <ElTable
                      :data="form.config.auth_flow.credentials"
                      border
                      stripe
                      size="small"
                      class="credentials-table"
                      empty-text="—"
                    >
                      <ElTableColumn
                        :label="$t('supplier.config.credKey')"
                        min-width="140"
                      >
                        <template #default="scope">
                          <ElInput
                            v-model="scope.row.key"
                            placeholder="key"
                            :disabled="!form.config.auth_flow.enabled"
                          />
                        </template>
                      </ElTableColumn>
                      <ElTableColumn
                        :label="$t('supplier.config.credValue')"
                        min-width="180"
                      >
                        <template #default="scope">
                          <ElInput
                            v-model="scope.row.value"
                            type="password"
                            show-password
                            placeholder="value"
                            :disabled="!form.config.auth_flow.enabled"
                          />
                        </template>
                      </ElTableColumn>
                      <ElTableColumn
                        :label="$t('common.fields.operations')"
                        width="88"
                        align="center"
                      >
                        <template #default="scope">
                          <ElButton
                            type="danger"
                            link
                            :disabled="!form.config.auth_flow.enabled"
                            @click="removeCredential(scope.$index)"
                          >
                            <ArtSvgIcon icon="ri:delete-bin-line" />
                          </ElButton>
                        </template>
                      </ElTableColumn>
                    </ElTable>
                    <div class="credentials-footer">
                      <ElButton
                        link
                        type="primary"
                        :disabled="!form.config.auth_flow.enabled"
                        @click="addCredential"
                      >
                        <ArtSvgIcon icon="ri:add-circle-line" class="mr-1" />
                        {{ $t("supplier.config.addCredential") }}
                      </ElButton>
                    </div>
                  </div>
                </template>
                <template v-else-if="credentialEntries.length">
                  <ElTag
                    v-for="[key, value] in credentialEntries"
                    :key="key"
                    class="mr-1 mb-1"
                    effect="plain"
                  >
                    {{ key }}={{ value }}
                  </ElTag>
                </template>
                <span v-else>{{ $t("supplier.detail.noCredentials") }}</span>
              </ElDescriptionsItem>


              <ElDescriptionsItem
                :label="$t('supplier.config.requestBodyTpl')"
                :span="2"
              >
                <ElInput
                  v-if="editing"
                  v-model="form.config.auth_flow.request_body_tpl"
                  type="textarea"
                  :rows="3"
                  class="tpl-textarea"
                  :placeholder="$t('supplier.config.requestBodyTplHint')"
                  :disabled="!form.config.auth_flow.enabled"
                />
                <pre v-else class="mono-block">{{
                  authFlow?.request_body_tpl || "-"
                }}</pre>
              </ElDescriptionsItem>

              <ElDescriptionsItem :label="$t('supplier.config.tokenJsonpath')">
                <ElInput
                  v-if="editing"
                  v-model="form.config.auth_flow.token_jsonpath"
                  placeholder="$.token"
                  :disabled="!form.config.auth_flow.enabled"
                />
                <span v-else>{{ authFlow?.token_jsonpath || "-" }}</span>
              </ElDescriptionsItem>
              <ElDescriptionsItem :label="$t('supplier.config.expiryJsonpath')">
                <ElInput
                  v-if="editing"
                  v-model="form.config.auth_flow.expiry_jsonpath"
                  placeholder="$.expires_in"
                  :disabled="!form.config.auth_flow.enabled"
                />
                <span v-else>{{ authFlow?.expiry_jsonpath || "-" }}</span>
              </ElDescriptionsItem>

              <ElDescriptionsItem :label="$t('supplier.config.injectKey')">
                <div
                  v-if="editing"
                  class="inject-field-combo"
                  :class="{ 'is-disabled': !form.config.auth_flow.enabled }"
                >
                  <ElSelect
                    v-model="form.config.auth_flow.inject_to"
                    class="inject-to-select"
                    :disabled="!form.config.auth_flow.enabled"
                  >
                    <ElOption label="Header" value="header" />
                    <ElOption label="Query" value="query" />
                    <ElOption label="Body" value="body" />
                  </ElSelect>
                  <ElInput
                    v-model="form.config.auth_flow.inject_key"
                    class="inject-key-input"
                    placeholder="Authorization"
                    :disabled="!form.config.auth_flow.enabled"
                  />
                </div>
                <span v-else>
                  {{ authFlow?.inject_to || "-" }} /
                  {{ authFlow?.inject_key || "-" }}
                </span>
              </ElDescriptionsItem>
              <ElDescriptionsItem :label="$t('supplier.config.injectFormat')">
                <ElInput
                  v-if="editing"
                  v-model="form.config.auth_flow.inject_value_format"
                  placeholder="Bearer {}"
                  :disabled="!form.config.auth_flow.enabled"
                />
                <span v-else>{{ authFlow?.inject_value_format || "-" }}</span>
              </ElDescriptionsItem>
            </ElDescriptions>


            <div
              v-if="editing && form.config.auth_flow.enabled"
              class="inject-preview-wrap"
            >
              <div class="inject-preview-label">
                {{ $t("supplier.config.injectPreview") }}
              </div>
              <InjectPreviewCard
                compact
                :inject-to="form.config.auth_flow.inject_to"
                :inject-key="form.config.auth_flow.inject_key"
                :inject-value-format="form.config.auth_flow.inject_value_format"
              />
            </div>
          </div>
        </div>


        <div class="config-section config-section--last">
          <div class="config-section-head">
            <span class="config-section-label">{{
              $t("supplier.detail.transformSectionTitle")
            }}</span>
            <div class="config-section-control">
              <ElSwitch
                v-if="editing"
                v-model="form.config.transform.enabled"
              />
              <ElTag
                v-else
                size="small"
                :type="transformEnabled ? 'success' : 'info'"
              >
                {{
                  transformEnabled
                    ? $t("supplier.detail.enabled")
                    : $t("supplier.detail.disabled")
                }}
              </ElTag>
            </div>
          </div>

          <div
            v-if="editing || transformEnabled"
            :class="{
              'section-body': true,
              'is-disabled': editing && !form.config.transform.enabled,
            }"
          >
            <ElDescriptions class="detail-descriptions" :column="1" border>
              <ElDescriptionsItem :label="$t('supplier.config.rewritePath')">
                <ElInput
                  v-if="editing"
                  v-model="form.config.transform.rewrite_path"
                  placeholder="/api/proxy"
                  :disabled="!form.config.transform.enabled"
                />
                <span v-else>{{ transform?.rewrite_path || "-" }}</span>
              </ElDescriptionsItem>
              <ElDescriptionsItem
                :label="$t('supplier.config.requestBodyTemplate')"
              >
                <ElInput
                  v-if="editing"
                  v-model="form.config.transform.request_body_template"
                  type="textarea"
                  :rows="3"
                  class="tpl-textarea"
                  :placeholder="$t('supplier.config.requestBodyTemplateHint')"
                  :disabled="!form.config.transform.enabled"
                />
                <pre v-else class="mono-block">{{
                  transform?.request_body_template || "-"
                }}</pre>
              </ElDescriptionsItem>
              <ElDescriptionsItem
                :label="$t('supplier.config.responseBodyTemplate')"
              >
                <ElInput
                  v-if="editing"
                  v-model="form.config.transform.response_body_template"
                  type="textarea"
                  :rows="2"
                  class="tpl-textarea"
                  placeholder="{{response.body}}"
                  :disabled="!form.config.transform.enabled"
                />
                <pre v-else class="mono-block">{{
                  transform?.response_body_template || "-"
                }}</pre>
              </ElDescriptionsItem>
            </ElDescriptions>
          </div>
        </div>
      </ElCard>
    </ElForm>
  </div>
</template>

<script setup lang="ts">
import {
  ElMessage,
  ElMessageBox,
  type FormInstance,
  type FormRules,
} from "element-plus";
import { computed, onMounted, reactive, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import {
  deleteSupplier,
  fetchSupplierById,
  updateSupplier,
  type Supplier,
} from "@/api/supplier";
import SurfaceCardHeader from "@/components/common/surface-card-header/index.vue";
import { runWithLoading } from "@/composables/useAsyncState";
import { $t } from "@/locales";
import { formatDateTime } from "@/utils/format";
import { formatUrlDisplay, stripUrlScheme } from "@/utils/sensitive";
import { logger } from "@/utils/sys";
import InjectPreviewCard from "../components/InjectPreviewCard.vue";
import {
  buildSupplierUpdatePayload,
  createEmptySupplierForm,
  resolveUpstreamType,
  supplierToForm,
  type SupplierFormModel,
} from "../supplier-form";
defineOptions({ name: "SupplierDetail" });

const route = useRoute();
const router = useRouter();
const formRef = ref<FormInstance>();
const loading = ref(false);
const submitting = ref(false);
const editing = ref(false);
const openingWorkbench = ref(false);
const supplier = ref<Supplier | null>(null);
const form = reactive<SupplierFormModel>(createEmptySupplierForm());

const supplierId = computed(() => route.params.id as string);


const openWorkbenchEdit = () => {
  openingWorkbench.value = true;
  void router
    .push({
      name: "UpstreamConnectorWorkbench",
      query: { supplier_id: supplierId.value },
    })
    .finally(() => {
      openingWorkbench.value = false;
    });
};
const upstreamType = computed(() =>
  editing.value
    ? form.upstream_type
    : supplier.value
      ? resolveUpstreamType(supplier.value)
      : "microservice",
);
const upstreamTypeLabel = computed(() =>
  upstreamType.value === "transparent"
    ? $t("supplier.upstreamOptions.transparent")
    : $t("supplier.upstreamOptions.microservice"),
);
const displayName = computed(() =>
  editing.value ? form.name || supplier.value?.name : supplier.value?.name,
);
const statusValue = computed(() =>
  editing.value ? form.status : supplier.value?.status || "",
);
const statusLabel = computed(() => {
  const s = statusValue.value;
  if (s === "active") return $t("common.status.active");
  if (s === "inactive") return $t("common.status.disabled");
  if (s === "suspended") return $t("common.status.suspended");
  return s || "-";
});
const contractId = computed(
  () =>
    (supplier.value?.config as { contract_id?: string } | undefined)
      ?.contract_id || "",
);
const authFlow = computed(() => supplier.value?.config?.auth_flow);
const transform = computed(() => supplier.value?.config?.transform);
const authEnabled = computed(() => Boolean(authFlow.value?.enabled));
const transformEnabled = computed(() => Boolean(transform.value?.enabled));
const credentialEntries = computed(() => {
  const creds = authFlow.value?.credentials;
  if (!creds || typeof creds !== "object") return [] as [string, string][];
  return Object.entries(creds).map(
    ([k, v]) => [k, String(v)] as [string, string],
  );
});

const rules = computed<FormRules>(() => {
  const baseRules: FormRules = {
    supplier_code: [
      {
        required: true,
        message: $t("supplier.validation.codeRequired"),
        trigger: "blur",
      },
    ],
    name: [
      {
        required: true,
        message: $t("supplier.validation.nameRequired"),
        trigger: "blur",
      },
    ],
    upstream_url: [
      {
        required: true,
        message: $t("supplier.validation.accessAddressRequired"),
        trigger: "blur",
      },
    ],
  };
  return baseRules;
});

const goList = () => {
  router.push({ name: "UpstreamSupply", query: { tab: "suppliers" } });
};

const handleBack = () => {
  if (editing.value) {
    cancelEdit();
    return;
  }
  goList();
};

const loadData = async () => {
  try {
    await runWithLoading(loading, async () => {
      supplier.value = await fetchSupplierById(supplierId.value);
      Object.assign(form, supplierToForm(supplier.value));
    });
  } catch (error) {
    logger.error("Failed to load supplier:", error);
    ElMessage.error($t("supplier.detail.loadFailed"));
  }
};

const enterEdit = () => {
  if (!supplier.value) return;
  Object.assign(form, supplierToForm(supplier.value));
  editing.value = true;
  formRef.value?.clearValidate();
};

const cancelEdit = () => {
  if (supplier.value) {
    Object.assign(form, supplierToForm(supplier.value));
  }
  editing.value = false;
  formRef.value?.clearValidate();
  if (route.query.edit != null) {
    router.replace({
      name: "SupplierDetail",
      params: { id: supplierId.value },
      query: {},
    });
  }
};

const addCredential = () => {
  form.config.auth_flow.credentials.push({ key: "", value: "" });
};

const removeCredential = (index: number) => {
  form.config.auth_flow.credentials.splice(index, 1);
};

const handleUrlBlur = () => {
  if (form.upstream_type === "transparent") {
    if (
      form.upstream_url.trim() &&
      !form.upstream_url.startsWith("http://") &&
      !form.upstream_url.startsWith("https://")
    ) {
      form.upstream_url = "https://" + form.upstream_url.trim();
    }
  } else {
    form.upstream_url = stripUrlScheme(form.upstream_url);
  }
};

const submitForm = async () => {
  const valid = await formRef.value?.validate().catch(() => false);
  if (!valid) return;

  if (
    form.upstream_type === "microservice" &&
    !(form.config.contract_id || "").trim()
  ) {
    ElMessage.error($t("supplier.validation.contractIdRequired"));
    return;
  }
  if (form.config.auth_flow.enabled) {
    if (!form.config.auth_flow.token_url.trim()) {
      ElMessage.error($t("upstreamOnboarding.validation.authTokenUrl"));
      return;
    }
    if (!form.config.auth_flow.token_jsonpath.trim()) {
      ElMessage.error($t("upstreamOnboarding.validation.authTokenPath"));
      return;
    }
    if (!form.config.auth_flow.inject_key.trim()) {
      ElMessage.error($t("upstreamOnboarding.validation.authInjectKey"));
      return;
    }
  }
  if (form.config.transform.enabled) {
    const t = form.config.transform;
    if (
      !t.rewrite_path.trim() &&
      !t.request_body_template.trim() &&
      !t.response_body_template.trim()
    ) {
      ElMessage.error($t("upstreamOnboarding.validation.transformEmpty"));
      return;
    }
  }

  await runWithLoading(submitting, async () => {
    const updated = await updateSupplier(
      supplierId.value,
      buildSupplierUpdatePayload(form),
    );
    supplier.value = updated;
    Object.assign(form, supplierToForm(updated));
    editing.value = false;
    ElMessage.success($t("common.feedback.saveSuccess"));
    if (route.query.edit != null) {
      router.replace({
        name: "SupplierDetail",
        params: { id: supplierId.value },
        query: {},
      });
    }
  });
};

const handleDelete = async () => {
  if (!supplier.value) return;
  await ElMessageBox.confirm(
    $t("supplier.messages.deleteConfirm", { name: supplier.value.name }),
    $t("common.tips"),
    { type: "warning" },
  );
  await deleteSupplier(supplier.value.id);
  ElMessage.success($t("common.feedback.deleteSuccess"));
  goList();
};

watch(
  () => route.query.edit,
  (val) => {
    if (val != null && supplier.value && !editing.value) {
      enterEdit();
    }
  },
);

onMounted(async () => {
  await loadData();
  if (route.query.edit != null) {
    enterEdit();
  }
});
</script>

<style scoped lang="scss">
.supplier-detail-page {
  padding-bottom: 24px;
}

.page-header-content {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}

.header-extra {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.detail-form {
  max-width: 100%;
}

.type-locked-alert {
  margin-bottom: 16px;
}

.detail-descriptions {
  :deep(.el-descriptions__label) {
    width: 140px;
    min-width: 140px;
    font-weight: 500;
    vertical-align: middle;
  }

  :deep(.el-descriptions__content) {
    min-width: 0;
  }

  :deep(.el-descriptions__cell) {
    padding-top: 12px;
    padding-bottom: 12px;
  }
}

.inline-form-item {
  margin-bottom: 0 !important;
  width: 100%;

  :deep(.el-form-item__content) {
    line-height: normal;
  }

  :deep(.el-form-item__error) {
    position: static;
    padding-top: 2px;
  }
}

.status-select {
  width: 160px;
  max-width: 100%;
}

.method-select {
  width: 140px;
  max-width: 100%;
}

.cell-placeholder {
  color: var(--el-text-color-placeholder);
}

.config-section {
  margin-bottom: 28px;

  &--last {
    margin-bottom: 0;
  }
}


.config-section-head {
  display: grid;
  grid-template-columns: 140px 1fr;
  align-items: center;
  min-height: 40px;
  margin-bottom: 12px;
  padding: 8px 12px;
  border-radius: 8px;
  background: var(--el-fill-color-light);
  border-left: 3px solid var(--el-color-primary);
}

.config-section-label {
  font-size: 14px;
  font-weight: 600;
  color: var(--el-text-color-primary);
}

.config-section-control {
  display: flex;
  align-items: center;
  justify-content: flex-start;
}

.section-body {
  transition: opacity 0.2s;

  &.is-disabled {
    opacity: 0.55;
    pointer-events: none;
  }
}

.mono-block {
  margin: 0;
  min-height: 1.5em;
  white-space: pre-wrap;
  word-break: break-all;
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas,
    "Liberation Mono", "Courier New", monospace;
  font-size: 12px;
  line-height: 1.5;
}

.tpl-textarea {
  width: 100%;

  :deep(.el-textarea__inner) {
    min-height: 72px;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas,
      monospace;
    font-size: 12px;
    line-height: 1.5;
  }
}

.credentials-block {
  width: 100%;
}

.credentials-table {
  width: 100%;

  :deep(.el-input__wrapper) {
    box-shadow: none !important;
    background: transparent;
  }
}

.credentials-footer {
  display: flex;
  justify-content: flex-start;
  margin-top: 8px;
  padding-top: 4px;
}

.inject-preview-wrap {
  margin-top: 16px;
  max-width: 720px;
}

.inject-preview-label {
  margin-bottom: 8px;
  font-size: 13px;
  font-weight: 500;
  color: var(--el-text-color-regular);
}

.inject-field-combo {
  display: flex;
  align-items: stretch;
  width: 100%;
  max-width: 420px;
  border: 1px solid var(--el-border-color);
  border-radius: var(--el-border-radius-base);
  background-color: var(--el-fill-color-blank);
  overflow: hidden;

  &.is-disabled {
    background-color: var(--el-disabled-bg-color);
    border-color: var(--el-border-color-lighter);
  }

  .inject-to-select {
    width: 110px;
    min-width: 110px;
    flex-shrink: 0;
  }

  .inject-key-input {
    flex: 1 1 0;
    min-width: 0;
  }

  :deep(.inject-to-select .el-select__wrapper) {
    box-shadow: none !important;
    border: none !important;
    border-radius: 0 !important;
    background-color: var(--el-fill-color-light) !important;
  }

  :deep(.inject-key-input .el-input__wrapper) {
    box-shadow: none !important;
    border: none !important;
    border-radius: 0 !important;
    border-left: 1px solid var(--el-border-color) !important;
    background-color: transparent !important;
  }
}

.mr-1 {
  margin-right: 4px;
}

.mb-1 {
  margin-bottom: 4px;
}

@media (max-width: 768px) {
  .config-section-head {
    grid-template-columns: 1fr auto;
  }

  .status-select,
  .method-select {
    width: 100%;
  }
}
</style>
