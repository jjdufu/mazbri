<template>
  <EditDialog
    v-model="visible"
    :title="$t('callbacks.fields.addTitle')"
    width="560px"
    :saving="loading"
    :confirm-text="$t('common.confirm')"
    :cancel-text="$t('common.cancel')"
    @confirm="handleSubmit"
    @close="handleClose"
  >
    <ElForm
      ref="formRef"
      :model="form"
      :rules="rules"
      label-width="124px"
      class="callback-create-form"
    >
      <ElFormItem :label="$t('callbacks.fields.tenant')" prop="tenant_code">
        <ElSelect
          v-model="form.tenant_code"
          filterable
          remote
          reserve-keyword
          clearable
          :remote-method="searchTenants"
          :loading="tenantLoading"
          :placeholder="$t('callbacks.placeholders.tenantCode')"
          class="control-full"
          @focus="ensureTenantsLoaded"
          @change="handleTenantChange"
        >
          <ElOption
            v-for="item in tenantOptions"
            :key="item.tenant_code"
            :label="`${item.name} (${item.tenant_code})`"
            :value="item.tenant_code"
          />
        </ElSelect>
      </ElFormItem>

      <ElFormItem :label="$t('callbacks.fields.product')" prop="product_code">
        <ElSelect
          v-model="form.product_code"
          filterable
          clearable
          :disabled="!form.tenant_code"
          :loading="productLoading"
          :no-data-text="$t('callbacks.messages.noAuthorizedProducts')"
          :placeholder="
            form.tenant_code
              ? $t('callbacks.placeholders.productCode')
              : $t('callbacks.placeholders.selectTenantFirst')
          "
          class="control-full"
        >
          <ElOption
            v-for="item in productOptions"
            :key="item.product_id"
            :label="`${item.name} (${item.product_id})`"
            :value="item.product_id"
          />
        </ElSelect>
      </ElFormItem>

      <ElFormItem :label="$t('callbacks.fields.url')" prop="callback_url">
        <ElInput
          v-model="form.callback_url"
          :placeholder="$t('callbacks.placeholders.callbackUrl')"
          class="control-full"
        />
      </ElFormItem>

      <ElFormItem :label="$t('callbacks.fields.secret')" prop="secret_key">
        <ElInput
          v-model="form.secret_key"
          type="password"
          show-password
          :placeholder="$t('callbacks.placeholders.secretKey')"
          class="control-full"
        >
          <template #append>
            <ElButton @click="generateRandomSecret">
              {{ $t("callbacks.actions.generateSecret") }}
            </ElButton>
          </template>
        </ElInput>
      </ElFormItem>
    </ElForm>
  </EditDialog>
</template>

<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { ElMessage, type FormInstance, type FormRules } from "element-plus";
import { createCallbackSubscription } from "@/api/callback";
import { fetchTenantList, fetchTenantProducts } from "@/api/tenant";
import EditDialog from "@/components/common/EditDialog.vue";
import { runWithLoading } from "@/composables/useAsyncState";
import { $t } from "@/locales";
import type { Tenant, TenantProduct } from "@/types/api/tenant";
import { getUserFacingErrorMessage } from "@/utils/http/error";

const SECRET_ALPHABET =
  "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const SECRET_LENGTH = 32;

const props = defineProps<{
  modelValue: boolean;
}>();

const emits = defineEmits<{
  (e: "update:modelValue", value: boolean): void;
  (e: "success"): void;
}>();

const visible = ref(false);
const loading = ref(false);
const tenantLoading = ref(false);
const productLoading = ref(false);
const tenantOptions = ref<Tenant[]>([]);
const productOptions = ref<TenantProduct[]>([]);
const formRef = ref<FormInstance>();

const form = ref({
  tenant_code: "",
  product_code: "",
  callback_url: "",
  secret_key: "",
});

const rules = computed<FormRules>(() => ({
  tenant_code: [
    {
      required: true,
      message: $t("callbacks.validation.tenantCodeRequired"),
      trigger: "change",
    },
  ],
  product_code: [
    {
      required: true,
      message: $t("callbacks.validation.productCodeRequired"),
      trigger: "change",
    },
  ],
  callback_url: [
    {
      required: true,
      message: $t("callbacks.validation.callbackUrlRequired"),
      trigger: "blur",
    },
  ],
  secret_key: [
    {
      required: true,
      message: $t("callbacks.validation.secretKeyRequired"),
      trigger: ["blur", "change"],
    },
  ],
}));

async function searchTenants(keyword = "") {
  tenantLoading.value = true;
  try {
    const res = await fetchTenantList({
      page: 1,
      page_size: 20,
      q: keyword.trim() || undefined,
    });
    tenantOptions.value = res.tenants ?? [];
  } catch {
    tenantOptions.value = [];
  } finally {
    tenantLoading.value = false;
  }
}

function ensureTenantsLoaded() {
  if (tenantOptions.value.length === 0 && !tenantLoading.value) {
    void searchTenants("");
  }
}

async function handleTenantChange(tenantCode: string) {
  form.value.product_code = "";
  productOptions.value = [];
  formRef.value?.clearValidate("product_code");
  if (!tenantCode) {
    return;
  }
  productLoading.value = true;
  try {
    const res = await fetchTenantProducts(tenantCode, {
      page: 1,
      page_size: 100,
    });
    productOptions.value = res.products ?? [];
  } catch {
    productOptions.value = [];
  } finally {
    productLoading.value = false;
  }
}

function generateRandomSecret() {
  const chars: string[] = [];
  const maxMultiple =
    Math.floor(256 / SECRET_ALPHABET.length) * SECRET_ALPHABET.length;
  const buf = new Uint8Array(64);
  while (chars.length < SECRET_LENGTH) {
    crypto.getRandomValues(buf);
    for (const byte of buf) {
      if (byte < maxMultiple) {
        chars.push(SECRET_ALPHABET[byte % SECRET_ALPHABET.length]);
        if (chars.length === SECRET_LENGTH) {
          break;
        }
      }
    }
  }
  form.value.secret_key = chars.join("");
  formRef.value?.clearValidate("secret_key");
}

watch(
  () => props.modelValue,
  (val) => {
    visible.value = val;
    if (val) {
      void searchTenants("");
    }
  },
);

watch(visible, (val) => {
  emits("update:modelValue", val);
});

const handleClose = () => {
  formRef.value?.resetFields();
  form.value.tenant_code = "";
  form.value.product_code = "";
  form.value.callback_url = "";
  form.value.secret_key = "";
  productOptions.value = [];
};

const handleSubmit = async () => {
  if (!formRef.value) return;
  const valid = await formRef.value.validate().catch(() => false);
  if (!valid) return;
  await runWithLoading(loading, async () => {
    try {
      await createCallbackSubscription({
        tenant_code: form.value.tenant_code.trim(),
        product_code: form.value.product_code.trim(),
        callback_url: form.value.callback_url.trim(),
        secret_key: form.value.secret_key.trim(),
      });
      ElMessage.success($t("common.feedback.createSuccess"));
      emits("success");
      visible.value = false;
    } catch (err: unknown) {
      ElMessage.error(
        getUserFacingErrorMessage(err, $t("callbacks.messages.createFailed")),
      );
    }
  });
};
</script>

<style scoped lang="scss">
.callback-create-form {
  .control-full {
    width: 100%;
  }
}
</style>
