<template>
  <EditDialog
    v-model="visible"
    :title="dialogTitle"
    width="820px"
    dialog-class="config-edit-dialog"
    :confirm-text="$t('common.actions.save')"
    :saving="saving"
    :confirm-disabled="authorizationDisabled || loadingDetail"
    :confirm-disabled-reason="authorizationDisabledReason"
    @confirm="handleSave"
    @closed="handleClosed"
  >
    <ElForm
      ref="formRef"
      v-loading="loadingDetail"
      :model="form"
      :rules="rules"
      label-position="top"
    >
      <template v-if="!isEdit">
        <ServiceNameSelector
          v-model="form.service_name"
          allow-custom
          :existing-names="existingNames"
          @change="handleServiceChange"
        />
      </template>

      <template v-else>
        <ElFormItem :label="$t('configCenter.editDialog.fields.serviceName')">
          <ElInput :model-value="config?.service_name" disabled />
        </ElFormItem>
      </template>

      <template v-if="form.service_name === 'mazbri-gateway'">
        <ElTabs
          v-model="activeTab"
          class="gateway-config-tabs"
          @tab-change="handleTabChange"
        >
          <ElTabPane
            :label="$t('configCenter.editDialog.tabs.toml')"
            name="toml"
            lazy
          >
            <ElFormItem
              :label="$t('configCenter.editDialog.fields.content')"
              prop="content"
              class="toml-editor-item"
            >
              <TomlEditor
                ref="tomlEditorRef"
                :content="form.content"
                :validation-error="validationError"
                @change="handleContentChange"
              />
            </ElFormItem>
          </ElTabPane>

          <ElTabPane
            :label="$t('configCenter.editDialog.tabs.cors')"
            name="visual"
            lazy
          >
            <div class="visual-config-container">
              <ElFormItem
                :label="$t('configCenter.editDialog.cors.allowedOrigins')"
              >
                <div class="origins-tag-container">
                  <ElTag
                    v-for="tag in gatewayCors.allowed_origins"
                    :key="tag"
                    closable
                    :disable-transitions="false"
                    @close="handleRemoveOrigin(tag)"
                  >
                    {{ tag }}
                  </ElTag>
                  <ElInput
                    v-if="inputOriginVisible"
                    ref="inputOriginRef"
                    v-model="inputOriginValue"
                    class="origin-input"
                    @keyup.enter="handleInputOriginConfirm"
                    @blur="handleInputOriginConfirm"
                  />
                  <ElButton v-else @click="showInputOrigin">
                    + {{ $t("common.actions.add") }}
                  </ElButton>
                </div>
                <div class="form-tip">
                  {{ $t("configCenter.editDialog.cors.allowedOriginsTip") }}
                </div>
              </ElFormItem>

              <ElFormItem
                :label="$t('configCenter.editDialog.cors.allowCredentials')"
              >
                <ElSwitch v-model="gatewayCors.allow_credentials" />
              </ElFormItem>
            </div>
          </ElTabPane>
        </ElTabs>
      </template>

      <template v-else>
        <ElFormItem
          :label="$t('configCenter.editDialog.fields.content')"
          prop="content"
        >
          <TomlEditor
            ref="tomlEditorRef"
            :content="form.content"
            :validation-error="validationError"
            @change="handleContentChange"
          />
        </ElFormItem>
      </template>
    </ElForm>
  </EditDialog>
</template>

<script setup lang="ts">
import { computed, nextTick, reactive, ref, watch } from "vue";
import { ElInput, ElMessage, type TabPaneName } from "element-plus";
import type { FormInstance, FormRules } from "element-plus";
import { $t } from "@/locales";
import type {
  ConfigSummary,
  CreateConfigRequest,
} from "@/types/api/config-center";
import {
  createConfig,
  fetchConfig,
  updateConfig,
  validateToml,
} from "@/api/config-center";
import EditDialog from "@/components/common/EditDialog.vue";
import TomlEditor from "./TomlEditor.vue";
import ServiceNameSelector from "./ServiceNameSelector.vue";
import { getLicenseDisabledReason } from "@/composables/useLicenseGate";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-dialog-service";
import { logger } from "@/utils/sys";
import { parseGatewayCors, serializeGatewayCors } from "../helpers";

const props = defineProps<{
  modelValue: boolean;
  config: ConfigSummary | null;
  existingNames?: string[];
  authorizationDisabled?: boolean;
  authorizationDisabledReason?: string;
}>();

const emit = defineEmits<{
  "update:modelValue": [value: boolean];
  success: [];
}>();

const visible = computed({
  get: () => props.modelValue,
  set: (value) => emit("update:modelValue", value),
});

const formRef = ref<FormInstance>();
const tomlEditorRef = ref<InstanceType<typeof TomlEditor> | null>(null);
const loadingDetail = ref(false);
const saving = ref(false);
const validationError = ref<string | null>(null);
let loadSeq = 0;

const isEdit = computed(() => !!props.config);
const dialogTitle = computed(() =>
  isEdit.value
    ? $t("configCenter.editDialog.actions.edit")
    : $t("configCenter.editDialog.actions.create"),
);

const form = reactive<CreateConfigRequest>({
  service_name: "",
  content: "",
  updated_by: "admin",
});

const rules = computed<FormRules>(() => ({
  service_name: [
    {
      required: true,
      message: $t("configCenter.editDialog.validation.serviceNameRequired"),
      trigger: "change",
    },
  ],
  content: [
    {
      required: true,
      message: $t("configCenter.editDialog.validation.contentRequired"),
      trigger: "blur",
    },
  ],
}));

const authorizationDisabled = computed(
  () => props.authorizationDisabled ?? false,
);
const authorizationDisabledReason = computed(
  () => props.authorizationDisabledReason ?? getLicenseDisabledReason(),
);

const activeTab = ref<"toml" | "visual">("toml");
const gatewayCors = reactive({
  allowed_origins: [] as string[],
  allow_credentials: true,
});
const inputOriginVisible = ref(false);
const inputOriginValue = ref("");
const inputOriginRef = ref<InstanceType<typeof ElInput> | null>(null);

const handleRemoveOrigin = (tag: string) => {
  gatewayCors.allowed_origins = gatewayCors.allowed_origins.filter(
    (o) => o !== tag,
  );
};

const showInputOrigin = () => {
  inputOriginVisible.value = true;
  void nextTick(() => {
    inputOriginRef.value?.focus();
  });
};

const handleInputOriginConfirm = () => {
  const value = inputOriginValue.value.trim();
  if (value && !gatewayCors.allowed_origins.includes(value)) {
    gatewayCors.allowed_origins.push(value);
  }
  inputOriginVisible.value = false;
  inputOriginValue.value = "";
};

const syncToContent = () => {
  const updatedContent = serializeGatewayCors(
    form.content,
    gatewayCors.allowed_origins,
    gatewayCors.allow_credentials,
  );
  if (form.content !== updatedContent) {
    form.content = updatedContent;
  }
};

watch(
  () => [gatewayCors.allowed_origins, gatewayCors.allow_credentials],
  () => {
    if (form.service_name === "mazbri-gateway" && activeTab.value === "visual") {
      syncToContent();
    }
  },
  { deep: true },
);

const handleTabChange = (newTab: TabPaneName) => {
  if (form.service_name !== "mazbri-gateway") {
    return;
  }
  if (newTab === "visual") {
    const parsed = parseGatewayCors(form.content);
    gatewayCors.allowed_origins = parsed.allowed_origins;
    gatewayCors.allow_credentials = parsed.allow_credentials;
    return;
  }
  syncToContent();
  void nextTick(() => {
    tomlEditorRef.value?.requestMeasure();
  });
};

const loadConfigDetail = async (serviceName: string) => {
  const seq = ++loadSeq;
  loadingDetail.value = true;
  try {
    const detail = await fetchConfig(serviceName);
    if (seq !== loadSeq || !visible.value) {
      return;
    }
    form.service_name = detail.service_name;
    form.content = detail.content;
    if (detail.service_name === "mazbri-gateway") {
      const parsed = parseGatewayCors(detail.content);
      gatewayCors.allowed_origins = parsed.allowed_origins;
      gatewayCors.allow_credentials = parsed.allow_credentials;
    }
    await nextTick();
    tomlEditorRef.value?.requestMeasure();
  } catch (error: unknown) {
    if (seq !== loadSeq) {
      return;
    }
    if (!isMfaStepUpCancelled(error)) {
      logger.error("Failed to load config detail:", error);
    }
    visible.value = false;
  } finally {
    if (seq === loadSeq) {
      loadingDetail.value = false;
    }
  }
};

watch(
  () => [props.modelValue, props.config] as const,
  ([isOpen, targetConfig]) => {
    if (!isOpen) {
      loadSeq++;
      loadingDetail.value = false;
      return;
    }
    activeTab.value = "toml";
    validationError.value = null;
    if (targetConfig) {
      form.service_name = targetConfig.service_name;
      form.content = "";
      gatewayCors.allowed_origins = [];
      gatewayCors.allow_credentials = true;
      void loadConfigDetail(targetConfig.service_name);
    } else {
      loadingDetail.value = false;
      form.service_name = "";
      form.content = "";
      gatewayCors.allowed_origins = [];
      gatewayCors.allow_credentials = true;
      void nextTick(() => {
        tomlEditorRef.value?.requestMeasure();
      });
    }
  },
  { immediate: true },
);

const handleServiceChange = (value: string) => {
  activeTab.value = "toml";
  if (value === "mazbri-gateway") {
    const parsed = parseGatewayCors(form.content);
    gatewayCors.allowed_origins = parsed.allowed_origins;
    gatewayCors.allow_credentials = parsed.allow_credentials;
  }
  void nextTick(() => {
    tomlEditorRef.value?.requestMeasure();
  });
};

const handleContentChange = (content: string) => {
  form.content = content;
  validationError.value = null;
  if (form.service_name === "mazbri-gateway") {
    const parsed = parseGatewayCors(content);
    if (
      JSON.stringify(gatewayCors.allowed_origins) !==
      JSON.stringify(parsed.allowed_origins)
    ) {
      gatewayCors.allowed_origins = parsed.allowed_origins;
    }
    if (gatewayCors.allow_credentials !== parsed.allow_credentials) {
      gatewayCors.allow_credentials = parsed.allow_credentials;
    }
  }
};

const handleSave = async () => {
  if (authorizationDisabled.value) {
    ElMessage.warning(authorizationDisabledReason.value);
    return;
  }
  if (loadingDetail.value) {
    return;
  }
  if (form.service_name === "mazbri-gateway" && activeTab.value === "visual") {
    syncToContent();
  }
  await formRef.value?.validate();
  saving.value = true;
  validationError.value = null;
  try {
    const validateResult = await validateToml(form.content);
    if (!validateResult.valid) {
      const { line, column, message } = validateResult.error!;
      validationError.value = $t(
        "configCenter.editDialog.validation.tomlError",
        { line, column, message },
      );
      if (form.service_name === "mazbri-gateway") {
        activeTab.value = "toml";
      }
      return;
    }
    if (isEdit.value && props.config) {
      await updateConfig(props.config.service_name, {
        content: form.content,
        updated_by: "admin",
      });
    } else {
      await createConfig(form);
    }
    ElMessage.success($t("configCenter.editDialog.messages.saveSuccess"));
    visible.value = false;
    emit("success");
  } catch (error: unknown) {
    if (!isMfaStepUpCancelled(error)) {
      logger.error("Failed to save config:", error);
    }
  } finally {
    saving.value = false;
  }
};

const handleClosed = () => {
  loadSeq++;
  loadingDetail.value = false;
  formRef.value?.resetFields();
  validationError.value = null;
  inputOriginVisible.value = false;
  inputOriginValue.value = "";
};
</script>

<style scoped lang="scss">
.gateway-config-tabs {
  width: 100%;
}

.visual-config-container {
  padding: 12px 0 4px;
}

.origins-tag-container {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
  width: 100%;
}

.form-tip {
  width: 100%;
  margin-top: 6px;
  line-height: 1.5;
  color: var(--el-text-color-secondary);
  font-size: 12px;
}

.origin-input {
  width: 220px;
}

.toml-editor-item {
  margin-top: 8px;
}
</style>
