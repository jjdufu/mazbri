<template>
  <EditDialog
    v-model="visible"
    :title="
      isEdit
        ? $t('channels.dialog.editTitle')
        : $t('channels.dialog.createTitle')
    "
    width="640px"
    :saving="submitting"
    @confirm="submitForm"
    @closed="handleClosed"
  >
    <ElForm
      ref="formRef"
      :model="form"
      :rules="rules"
      label-width="136px"
      class="channel-form"
    >
      <ElFormItem :label="$t('channels.fields.supplier')" prop="supplier_id">
        <ElSelect
          v-model="form.supplier_id"
          filterable
          remote
          reserve-keyword
          :remote-method="searchSuppliers"
          :loading="supplierLoading"
          :placeholder="$t('channels.placeholders.supplierRemote')"
          class="control-full"
          @focus="ensureSuppliersLoaded"
        >
          <ElOption
            v-for="supplier in supplierOptions"
            :key="supplier.id"
            :label="`${supplier.name} (${supplier.supplier_code})`"
            :value="supplier.id"
          />
        </ElSelect>
      </ElFormItem>

      <ElFormItem
        :label="$t('channels.fields.channelCode')"
        prop="channel_code"
      >
        <ElInput
          v-model="form.channel_code"
          :placeholder="$t('channels.placeholders.channelCode')"
          class="control-full"
        />
      </ElFormItem>

      <ElFormItem :label="$t('channels.fields.name')" prop="name">
        <ElInput
          v-model="form.name"
          :placeholder="$t('channels.placeholders.name')"
          class="control-full"
        />
      </ElFormItem>

      <ElFormItem :label="$t('channels.fields.pathPrefix')" prop="path_prefix">
        <ElInput
          v-model="form.path_prefix"
          :placeholder="$t('channels.placeholders.pathPrefix')"
          class="control-full"
        />
      </ElFormItem>

      <ElFormItem :label="$t('channels.fields.costPrice')" prop="cost_price">
        <div class="cost-price-field">
          <ElInput
            v-model="form.cost_price"
            inputmode="decimal"
            :placeholder="$t('channels.placeholders.costPrice')"
            class="control-full"
            @blur="handleCostPriceBlur"
          />
          <p class="field-hint">{{ $t("channels.fields.costPriceHint") }}</p>
        </div>
      </ElFormItem>

      <ElFormItem :label="$t('channels.fields.qpsLimit')" prop="qps_limit">
        <div class="qps-limit-field">
          <div class="qps-limit-input-row">
            <ElInputNumber
              v-model="form.qps_limit"
              :min="0"
              :precision="0"
              controls-position="right"
              class="qps-limit-number"
            />
            <span class="qps-unit">{{ $t("channels.fields.qpsUnit") }}</span>
          </div>
          <p class="field-hint">{{ $t("channels.fields.qpsLimitHint") }}</p>
        </div>
      </ElFormItem>

      <ElFormItem
        v-if="isEdit"
        :label="$t('common.fields.status')"
        prop="status"
      >
        <ElSelect v-model="form.status" class="control-full">
          <ElOption :label="$t('channels.status.active')" value="active" />
          <ElOption :label="$t('channels.status.inactive')" value="inactive" />
          <ElOption
            :label="$t('channels.status.suspended')"
            value="suspended"
          />
        </ElSelect>
      </ElFormItem>
    </ElForm>
  </EditDialog>
</template>

<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import {
  ElMessage,
  type FormInstance,
  type FormRules,
} from "element-plus";
import {
  createChannel,
  updateChannel,
  type Channel,
} from "@/api/route-management";
import { fetchSuppliers, type Supplier } from "@/api/supplier";
import EditDialog from "@/components/common/EditDialog.vue";
import { runWithLoading } from "@/composables/useAsyncState";
import { $t } from "@/locales";
import { getUserFacingErrorMessage } from "@/utils/http/error";
import {
  formatCostPrice,
  isValidCostPrice,
  normalizeCostPriceInput,
} from "../costPrice";

defineOptions({ name: "ChannelFormDialog" });

const props = withDefaults(
  defineProps<{
    channel?: Channel | null;
  }>(),
  {
    channel: null,
  },
);

const visible = defineModel<boolean>({ required: true });
const emit = defineEmits<{
  success: [channel: Channel];
}>();

const formRef = ref<FormInstance>();
const submitting = ref(false);
const supplierLoading = ref(false);
const supplierOptions = ref<Supplier[]>([]);

const isEdit = computed(() => Boolean(props.channel?.id));

const form = reactive({
  supplier_id: "",
  channel_code: "",
  name: "",
  path_prefix: "/",
  cost_price: "0.00",
  qps_limit: 0,
  status: "active",
});

const rules = computed<FormRules>(() => ({
  supplier_id: [
    {
      required: true,
      message: $t("channels.validation.supplierRequired"),
      trigger: "change",
    },
  ],
  channel_code: [
    {
      required: true,
      message: $t("channels.validation.channelCodeRequired"),
      trigger: "blur",
    },
    {
      pattern: /^[A-Z][A-Z0-9-]*$/,
      message: $t("channels.validation.channelCodePattern"),
      trigger: "blur",
    },
  ],
  name: [
    {
      required: true,
      message: $t("channels.validation.nameRequired"),
      trigger: "blur",
    },
  ],
  path_prefix: [
    {
      required: true,
      message: $t("channels.validation.pathPrefixRequired"),
      trigger: "blur",
    },
  ],
  cost_price: [
    {
      required: true,
      validator: (_rule, value: unknown, callback) => {
        const raw = typeof value === "string" ? value.trim() : "";
        if (!raw) {
          callback(new Error($t("channels.validation.costPriceRequired")));
          return;
        }
        if (!isValidCostPrice(raw)) {
          callback(new Error($t("channels.validation.costPricePattern")));
          return;
        }
        callback();
      },
      trigger: ["blur", "change"],
    },
  ],
}));

function prependCurrentSupplierIfMissing(list: Supplier[]): Supplier[] {
  const current = props.channel;
  if (!current?.supplier_id) {
    return list;
  }
  if (list.some((item) => item.id === current.supplier_id)) {
    return list;
  }
  return [
    {
      id: current.supplier_id,
      supplier_code: current.supplier_code,
      name: current.supplier_name,
      upstream_url: "",
      status: "active",
      created_at: "",
      updated_at: "",
    },
    ...list,
  ];
}

async function searchSuppliers(keyword = "") {
  supplierLoading.value = true;
  try {
    const res = await fetchSuppliers({
      status: "active",
      page: 1,
      page_size: 20,
      q: keyword.trim() || undefined,
    });
    supplierOptions.value = prependCurrentSupplierIfMissing(
      res.suppliers ?? [],
    );
  } catch {
    supplierOptions.value = prependCurrentSupplierIfMissing([]);
  } finally {
    supplierLoading.value = false;
  }
}

function ensureSuppliersLoaded() {
  if (supplierOptions.value.length === 0 && !supplierLoading.value) {
    void searchSuppliers("");
  }
}

function populateForm() {
  if (props.channel) {
    form.supplier_id = props.channel.supplier_id;
    form.channel_code = props.channel.channel_code;
    form.name = props.channel.name;
    form.path_prefix = props.channel.path_prefix;
    form.cost_price = formatCostPrice(props.channel.cost_price);
    form.qps_limit = props.channel.qps_limit ?? 0;
    form.status = props.channel.status || "active";
  } else {
    form.supplier_id = "";
    form.channel_code = "";
    form.name = "";
    form.path_prefix = "/";
    form.cost_price = "0.00";
    form.qps_limit = 0;
    form.status = "active";
  }
  formRef.value?.clearValidate();
}

watch(
  [visible, () => props.channel],
  ([open]) => {
    if (open) {
      populateForm();
      void searchSuppliers("");
    }
  },
  { immediate: false },
);

function handleCostPriceBlur() {
  form.cost_price = normalizeCostPriceInput(form.cost_price);
}

function handleClosed() {
  formRef.value?.clearValidate();
}

async function submitForm() {
  handleCostPriceBlur();
  const valid = await formRef.value?.validate().catch(() => false);
  if (!valid) {
    return;
  }
  await runWithLoading(submitting, async () => {
    try {
      const normalizedPrice = normalizeCostPriceInput(form.cost_price);
      let saved: Channel;
      if (isEdit.value && props.channel) {
        saved = await updateChannel(props.channel.id, {
          supplier_id: form.supplier_id,
          channel_code: form.channel_code.trim(),
          name: form.name.trim(),
          path_prefix: form.path_prefix.trim(),
          cost_price: normalizedPrice,
          qps_limit: form.qps_limit ?? 0,
          status: form.status,
        });
      } else {
        saved = await createChannel({
          supplier_id: form.supplier_id,
          channel_code: form.channel_code.trim(),
          name: form.name.trim(),
          path_prefix: form.path_prefix.trim(),
          cost_price: normalizedPrice,
          qps_limit: form.qps_limit ?? 0,
        });
      }
      ElMessage.success($t("common.feedback.saveSuccess"));
      visible.value = false;
      emit("success", saved);
    } catch (error: unknown) {
      ElMessage.error(
        getUserFacingErrorMessage(error, $t("common.feedback.saveFailed")),
      );
    }
  });
}
</script>

<style scoped lang="scss">
.channel-form {
  .control-full {
    width: 100%;
  }
}

.cost-price-field,
.qps-limit-field {
  width: 100%;
}

.qps-limit-input-row {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;

  .qps-limit-number {
    flex: 1;
    min-width: 0;
  }

  .qps-unit {
    flex-shrink: 0;
    font-size: 13px;
    font-weight: 500;
    color: var(--el-text-color-regular);
  }
}

.field-hint {
  margin: 6px 0 0;
  font-size: 12px;
  line-height: 1.5;
  color: var(--el-text-color-secondary);
}
</style>
