<template>
  <EditDialog
    v-model="visible"
    :title="dialogTitle"
    width="800px"
    dialog-class="config-edit-dialog"
    :confirm-text="$t('common.actions.save')"
    :saving="saving"
    :confirm-disabled="authorizationDisabled"
    :confirm-disabled-reason="authorizationDisabledReason"
    @confirm="handleSave"
    @closed="handleClosed"
  >
    <ElForm ref="formRef" :model="form" :rules="rules" label-width="100px">
      <template v-if="!isEdit">
        <ServiceNameSelector
          v-model="form.service_name"
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
        <ElTabs v-model="activeTab" class="gateway-config-tabs">
          <ElTabPane label="可视化配置" name="visual">
            <div class="visual-config-container">
              <ElFormItem label="允许跨域域名">
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
                    size="small"
                    class="w-40"
                    @keyup.enter="handleInputOriginConfirm"
                    @blur="handleInputOriginConfirm"
                  />
                  <ElButton v-else size="small" @click="showInputOrigin">
                    + {{ $t("common.actions.add") || "添加" }}
                  </ElButton>
                </div>
                <div class="form-tip">
                  开发环境可填写 *，生产环境必须配置具体的受信任域名（如 https://mgmt.jkcredit.com）
                </div>
              </ElFormItem>
              <ElFormItem label="允许携带凭证">
                <ElSwitch v-model="gatewayCors.allow_credentials" />
              </ElFormItem>
            </div>
          </ElTabPane>
          <ElTabPane label="完整 TOML 源码" name="toml">
            <ElFormItem
              :label="$t('configCenter.editDialog.fields.content')"
              prop="content"
              class="toml-editor-item"
            >
              <TomlEditor
                :content="form.content"
                :validation-error="validationError"
                @change="handleContentChange"
              />
            </ElFormItem>
          </ElTabPane>
        </ElTabs>
      </template>
      <template v-else>
        <ElFormItem
          :label="$t('configCenter.editDialog.fields.content')"
          prop="content"
        >
          <TomlEditor
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
import { ElInput, ElMessage } from "element-plus";
import type { FormInstance, FormRules } from "element-plus";
import { $t } from "@/locales";
import type {
  ConfigEntry,
  CreateConfigRequest,
} from "@/types/api/config-center";
import { createConfig, updateConfig, validateToml } from "@/api/config-center";
import EditDialog from "@/components/common/EditDialog.vue";
import TomlEditor from "./TomlEditor.vue";
import ServiceNameSelector from "./ServiceNameSelector.vue";
import { getLicenseDisabledReason } from "@/composables/useLicenseGate";
import { logger } from "@/utils/sys";
const props = defineProps<{
  modelValue: boolean;
  config: ConfigEntry | null;
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
const saving = ref(false);
const validationError = ref<string | null>(null);
const isEdit = computed(() => !!props.config);
const dialogTitle = computed(() => {
  return isEdit.value
    ? $t("configCenter.editDialog.actions.edit")
    : $t("configCenter.editDialog.actions.create");
});
const form = reactive<CreateConfigRequest>({
  service_name: "",
  content: "",
  updated_by: "admin",
});
const rules: FormRules = {
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
};
const authorizationDisabled = computed(
  () => props.authorizationDisabled ?? false,
);
const authorizationDisabledReason = computed(
  () =>
    props.authorizationDisabledReason ?? getLicenseDisabledReason(),
);
function parseGatewayCors(tomlContent: string) {
  const corsSectionRegex = /\[cors\][\s\S]*?(?=\n\s*\[[a-zA-Z_]|$)/;
  const match = tomlContent.match(corsSectionRegex);
  if (!match) {
    return {
      allowed_origins: [] as string[],
      allow_credentials: true,
    };
  }
  const corsSection = match[0];

  let allowed_origins: string[] = [];
  const originsMatch = corsSection.match(/allowed_origins\s*=\s*\[([^\]]*)\]/);
  if (originsMatch) {
    const rawOrigins = originsMatch[1];
    allowed_origins = rawOrigins
      .split(",")
      .map((s) => s.trim().replace(/^["']|["']$/g, ""))
      .filter((s) => s.length > 0);
  }

  let allow_credentials = true;
  const credentialsMatch = corsSection.match(/allow_credentials\s*=\s*(true|false)/);
  if (credentialsMatch) {
    allow_credentials = credentialsMatch[1] === "true";
  }

  return { allowed_origins, allow_credentials };
}

function serializeGatewayCors(
  tomlContent: string,
  allowedOrigins: string[],
  allowCredentials: boolean,
): string {
  const corsSectionRegex = /\[cors\][\s\S]*?(?=\n\s*\[[a-zA-Z_]|$)/;
  const match = tomlContent.match(corsSectionRegex);

  const formattedOrigins = `allowed_origins = [${allowedOrigins.map((s) => `"${s}"`).join(", ")}]`;
  const formattedCredentials = `allow_credentials = ${allowCredentials}`;
  const newCorsSection = `[cors]\n${formattedOrigins}\n${formattedCredentials}\n\n`;

  if (match) {
    return tomlContent.replace(match[0], newCorsSection);
  } else {
    const trimmed = tomlContent.trim();
    if (trimmed.length === 0) {
      return newCorsSection;
    }
    return `${trimmed}\n\n${newCorsSection}`;
  }
}

const activeTab = ref("visual");
const gatewayCors = reactive({
  allowed_origins: [] as string[],
  allow_credentials: true,
});
const inputOriginVisible = ref(false);
const inputOriginValue = ref("");
const inputOriginRef = ref<InstanceType<typeof ElInput> | null>(null);

const handleRemoveOrigin = (tag: string) => {
  gatewayCors.allowed_origins = gatewayCors.allowed_origins.filter((o) => o !== tag);
};

const showInputOrigin = () => {
  inputOriginVisible.value = true;
  nextTick(() => {
    inputOriginRef.value?.focus();
  });
};

const handleInputOriginConfirm = () => {
  const value = inputOriginValue.value.trim();
  if (value) {
    if (!gatewayCors.allowed_origins.includes(value)) {
      gatewayCors.allowed_origins.push(value);
    }
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

watch(activeTab, (newTab) => {
  if (form.service_name === "mazbri-gateway") {
    if (newTab === "visual") {
      const parsed = parseGatewayCors(form.content);
      gatewayCors.allowed_origins = parsed.allowed_origins;
      gatewayCors.allow_credentials = parsed.allow_credentials;
    } else {
      syncToContent();
    }
  }
});

watch(
  () => props.config,
  (newConfig) => {
    if (newConfig) {
      form.service_name = newConfig.service_name;
      form.content = newConfig.content;
      if (newConfig.service_name === "mazbri-gateway") {
        const parsed = parseGatewayCors(newConfig.content);
        gatewayCors.allowed_origins = parsed.allowed_origins;
        gatewayCors.allow_credentials = parsed.allow_credentials;
      }
    } else {
      form.service_name = "";
      form.content = "";
      gatewayCors.allowed_origins = [];
      gatewayCors.allow_credentials = true;
    }
  },
  { immediate: true },
);

const handleServiceChange = (value: string) => {
  void value;
};

const handleContentChange = (content: string) => {
  form.content = content;
  validationError.value = null;
  if (form.service_name === "mazbri-gateway") {
    const parsed = parseGatewayCors(content);
    if (JSON.stringify(gatewayCors.allowed_origins) !== JSON.stringify(parsed.allowed_origins)) {
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
      saving.value = false;
      return;
    }
    if (isEdit.value) {
      await updateConfig(props.config!.service_name, {
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
    logger.error("Failed to save config:", error);
  } finally {
    saving.value = false;
  }
};
const handleClosed = () => {
  formRef.value?.resetFields();
  validationError.value = null;
};
</script>

<style scoped lang="scss">
.gateway-config-tabs {
  width: 100%;
}
.visual-config-container {
  padding: 16px 0;
}
.origins-tag-container {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
}
.form-tip {
  margin-top: 6px;
  color: var(--art-gray-500, #909399);
  font-size: 12px;
}
.w-40 {
  width: 160px;
}
.toml-editor-item {
  margin-top: 16px;
}
:deep(.el-form-item__content) {
  flex: 1;
}

:deep(.config-edit-dialog) {
  overflow: hidden;
}

:deep(.config-edit-dialog .el-dialog__header) {
  margin-right: 0;
  padding: 20px 24px 14px;
  border-bottom: 1px solid var(--art-card-border);
  background: var(--default-box-color);
}

:deep(.config-edit-dialog .el-dialog__title) {
  color: var(--art-gray-900);
  font-size: 18px;
  font-weight: 600;
  letter-spacing: 0.01em;
}

:deep(.config-edit-dialog .el-dialog__headerbtn) {
  top: 18px;
  right: 18px;
}

:deep(.config-edit-dialog .el-dialog__body) {
  padding: 24px 24px 20px;
  background: var(--default-box-color);
}

:deep(.config-edit-dialog .el-dialog__footer) {
  padding: 16px 24px 20px;
  border-top: 1px solid var(--art-card-border);
  background: var(--default-box-color);
}

:deep(.config-edit-dialog .el-form-item) {
  margin-bottom: 22px;
}

:deep(.config-edit-dialog .el-form-item:last-child) {
  margin-bottom: 0;
}

:deep(.config-edit-dialog .el-form-item__label) {
  color: var(--art-gray-800);
  font-weight: 600;
}
</style>
