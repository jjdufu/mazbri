<template>
  <div class="crypto-suite-select" :class="{ disabled }">
    <div class="field-block">
      <ElSelect
        :model-value="modelValue.suite ?? undefined"
        clearable
        filterable
        :disabled="disabled"
        :placeholder="$t('tenant.transportEncryption.placeholders.selectSuite')"
        @update:model-value="onSuiteChange"
      >
        <ElOption
          v-for="suite in CRYPTO_SUITES"
          :key="suite.id"
          :label="suiteOptionLabel(suite)"
          :value="suite.id"
        >
          <div class="option-row">
            <span>{{ $t(suite.labelKey) }}</span>
            <ElTag size="small" :type="securityTagType(suite.security)">
              {{ $t(securityClassLabelKey(suite.security)) }}
            </ElTag>
          </div>
        </ElOption>
      </ElSelect>
      <div v-if="selectedSuiteHint" class="field-hint">{{ selectedSuiteHint }}</div>
    </div>

    <div class="field-block">
      <CommonSwitchRow>
        <ElSwitch
          :model-value="modelValue.allowInsecure"
          :disabled="disabled || !canToggleInsecure"
          @update:model-value="onAllowInsecureChange"
        />
        <span class="switch-text ml-2">
          {{ $t("tenant.transportEncryption.fields.allowInsecure") }}
        </span>
      </CommonSwitchRow>
      <div class="field-hint warning">
        {{
          canToggleInsecure
            ? $t("tenant.transportEncryption.hints.allowInsecure")
            : $t("tenant.transportEncryption.hints.allowInsecureAdminOnly")
        }}
      </div>
    </div>

    <ElCollapse v-model="advancedOpen" class="advanced-collapse">
      <ElCollapseItem
        name="advanced"
        :title="$t('tenant.transportEncryption.fields.advancedAlgorithms')"
      >
        <div class="advanced-grid">
          <div class="field-block">
            <div class="field-label">
              {{ $t("tenant.transportEncryption.fields.dataAlg") }}
            </div>
            <ElSelect
              :model-value="modelValue.dataAlg ?? undefined"
              clearable
              filterable
              :disabled="disabled"
              :placeholder="
                $t('tenant.transportEncryption.placeholders.selectDataAlg')
              "
              @update:model-value="onDataAlgChange"
            >
              <ElOption
                v-for="alg in dataOptions"
                :key="alg.id"
                :label="algOptionLabel(alg)"
                :value="alg.id"
              >
                <div class="option-row">
                  <span>{{ alg.label }}</span>
                  <ElTag size="small" :type="securityTagType(alg.security)">
                    {{ $t(securityClassLabelKey(alg.security)) }}
                  </ElTag>
                </div>
              </ElOption>
            </ElSelect>
          </div>
          <div class="field-block">
            <div class="field-label">
              {{ $t("tenant.transportEncryption.fields.macAlg") }}
              <span v-if="macRequired" class="required-mark">*</span>
            </div>
            <ElSelect
              :model-value="modelValue.macAlg ?? undefined"
              clearable
              filterable
              :disabled="disabled"
              :placeholder="
                $t('tenant.transportEncryption.placeholders.selectMacAlg')
              "
              @update:model-value="onMacAlgChange"
            >
              <ElOption
                v-for="alg in macOptions"
                :key="alg.id"
                :label="algOptionLabel(alg)"
                :value="alg.id"
              >
                <div class="option-row">
                  <span>{{ alg.label }}</span>
                  <ElTag size="small" :type="securityTagType(alg.security)">
                    {{ $t(securityClassLabelKey(alg.security)) }}
                  </ElTag>
                </div>
              </ElOption>
            </ElSelect>
            <div v-if="macRequired" class="field-hint">
              {{ $t("tenant.transportEncryption.hints.macRequired") }}
            </div>
          </div>
        </div>
      </ElCollapseItem>
    </ElCollapse>
  </div>
</template>

<script setup lang="ts">
import { $t } from "@/locales";
import CommonSwitchRow from "@/components/common/switch-row/index.vue";
import {
  CRYPTO_SUITES,
  dataAlgorithmOptions,
  findAlgorithm,
  findSuite,
  macAlgorithmOptions,
  securityClassLabelKey,
  type CryptoAlgorithmMeta,
  type CryptoSuiteMeta,
  type SecurityClass,
} from "./cryptoCatalog";

export interface CryptoSuiteSelectValue {
  suite: string | null;
  dataAlg: string | null;
  macAlg: string | null;
  allowInsecure: boolean;
}

const props = withDefaults(
  defineProps<{
    modelValue: CryptoSuiteSelectValue;
    disabled?: boolean;
    /** 仅超级管理员可打开 allow_insecure */
    canToggleInsecure?: boolean;
  }>(),
  {
    disabled: false,
    canToggleInsecure: false,
  },
);

const emit = defineEmits<{
  "update:modelValue": [CryptoSuiteSelectValue];
}>();

const advancedOpen = ref<string[]>([]);

const dataOptions = computed(() =>
  dataAlgorithmOptions(props.modelValue.allowInsecure),
);
const macOptions = computed(() =>
  macAlgorithmOptions(props.modelValue.allowInsecure),
);

const selectedSuite = computed(() => findSuite(props.modelValue.suite));
const selectedSuiteHint = computed(() => {
  const suite = selectedSuite.value;
  if (!suite) return "";
  const mac = suite.macAlg ?? $t("tenant.transportEncryption.hints.noMac");
  return $t("tenant.transportEncryption.hints.suiteFollows", {
    data: suite.dataAlg,
    mac,
  });
});

const macRequired = computed(() => {
  const alg =
    findAlgorithm(props.modelValue.dataAlg) ??
    (selectedSuite.value
      ? findAlgorithm(selectedSuite.value.dataAlg)
      : undefined);
  return Boolean(alg?.requireMac);
});

watch(
  () => [props.modelValue.dataAlg, props.modelValue.macAlg],
  ([dataAlg, macAlg]) => {
    if (dataAlg || macAlg) {
      if (!advancedOpen.value.includes("advanced")) {
        advancedOpen.value = ["advanced"];
      }
    }
  },
  { immediate: true },
);

const patch = (partial: Partial<CryptoSuiteSelectValue>) => {
  emit("update:modelValue", { ...props.modelValue, ...partial });
};

const onSuiteChange = (value: string | null | undefined) => {
  const suiteId = value || null;
  if (!suiteId) {
    patch({ suite: null });
    return;
  }
  // 选套件时清空高级覆盖，表示跟随 suite
  patch({
    suite: suiteId,
    dataAlg: null,
    macAlg: null,
  });
};

const onAllowInsecureChange = (value: boolean | string | number) => {
  const allow = Boolean(value);
  if (!allow) {
    const data = findAlgorithm(props.modelValue.dataAlg);
    const mac = findAlgorithm(props.modelValue.macAlg);
    patch({
      allowInsecure: false,
      dataAlg: data?.legacy ? null : props.modelValue.dataAlg,
      macAlg: mac?.legacy ? null : props.modelValue.macAlg,
    });
    return;
  }
  patch({ allowInsecure: true });
};

const onDataAlgChange = (value: string | null | undefined) => {
  const dataAlg = value || null;
  const meta = findAlgorithm(dataAlg);
  patch({
    suite: null,
    dataAlg,
    macAlg: meta?.requireMac ? props.modelValue.macAlg : null,
  });
};

const onMacAlgChange = (value: string | null | undefined) => {
  patch({
    suite: null,
    macAlg: value || null,
  });
};

const suiteOptionLabel = (suite: CryptoSuiteMeta) =>
  `${$t(suite.labelKey)} (${suite.dataAlg}${suite.macAlg ? ` + ${suite.macAlg}` : ""})`;

const algOptionLabel = (alg: CryptoAlgorithmMeta) =>
  `${alg.label} · ${$t(securityClassLabelKey(alg.security))}`;

const securityTagType = (
  security: SecurityClass,
): "success" | "warning" | "info" | "danger" | "primary" => {
  if (security === "recommended") return "success";
  if (security === "gm") return "primary";
  if (security === "legacy") return "danger";
  return "info";
};
</script>

<style scoped lang="scss">
.crypto-suite-select {
  display: flex;
  flex-direction: column;
  gap: 14px;
  width: 100%;

  &.disabled {
    opacity: 0.75;
  }
}

.field-block {
  display: flex;
  flex-direction: column;
  gap: 6px;
  width: 100%;
}

.field-label {
  font-size: 13px;
  font-weight: 600;
  color: var(--el-text-color-regular);
}

.field-hint {
  font-size: 12px;
  color: var(--el-text-color-secondary);
  line-height: 1.4;

  &.warning {
    color: var(--el-color-warning);
  }
}

.required-mark {
  color: var(--el-color-danger);
  margin-left: 2px;
}

.option-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  width: 100%;
}

.advanced-collapse {
  border: none;

  :deep(.el-collapse-item__header) {
    font-size: 13px;
    font-weight: 600;
    height: 36px;
    line-height: 36px;
  }

  :deep(.el-collapse-item__wrap),
  :deep(.el-collapse-item__header) {
    border: none;
    background: transparent;
  }
}

.advanced-grid {
  display: grid;
  grid-template-columns: 1fr;
  gap: 12px;
}

.switch-text {
  font-size: 13px;
}
</style>
