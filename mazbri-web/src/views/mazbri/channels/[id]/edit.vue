<template>
  <div class="channel-edit-page">
    <ElPageHeader @back="goBack">
      <template #content>
        <span class="text-lg font-medium">{{
          $t("channels.dialog.editTitle")
        }}</span>
      </template>
    </ElPageHeader>

    <ElCard v-loading="loading" class="mt-5" shadow="never">
      <ElForm
        ref="formRef"
        :model="form"
        :rules="rules"
        label-width="120px"
        class="channel-edit-form"
      >
        <ElFormItem :label="$t('channels.fields.supplier')" prop="supplier_id">
          <ElSelect
            v-model="form.supplier_id"
            filterable
            :placeholder="$t('channels.placeholders.supplier')"
          >
            <ElOption
              v-for="supplier in suppliers"
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
          />
        </ElFormItem>
        <ElFormItem :label="$t('channels.fields.name')" prop="name">
          <ElInput
            v-model="form.name"
            :placeholder="$t('channels.placeholders.name')"
          />
        </ElFormItem>
        <ElFormItem
          :label="$t('channels.fields.pathPrefix')"
          prop="path_prefix"
        >
          <ElInput
            v-model="form.path_prefix"
            :placeholder="$t('channels.placeholders.pathPrefix')"
          />
        </ElFormItem>
        <ElFormItem :label="$t('channels.fields.costPrice')">
          <ElInput
            v-model="form.cost_price"
            :placeholder="$t('channels.placeholders.costPrice')"
          />
        </ElFormItem>
        <ElFormItem :label="$t('channels.fields.qpsLimit')">
          <ElInputNumber v-model="form.qps_limit" :min="0" />
        </ElFormItem>
        <ElFormItem :label="$t('common.fields.status')">
          <ElSelect v-model="form.status">
            <ElOption :label="$t('channels.status.active')" value="active" />
            <ElOption
              :label="$t('channels.status.inactive')"
              value="inactive"
            />
            <ElOption
              :label="$t('channels.status.suspended')"
              value="suspended"
            />
          </ElSelect>
        </ElFormItem>

        <ElFormItem class="form-actions">
          <ElButton @click="goBack">{{
            $t("channels.form.cancelBack")
          }}</ElButton>
          <ElButton type="primary" :loading="submitting" @click="submitForm">
            {{ $t("channels.form.saveAndBack") }}
          </ElButton>
        </ElFormItem>
      </ElForm>
    </ElCard>
  </div>
</template>

<script setup lang="ts">
import {
  ElMessage,
  type FormInstance,
  type FormRules,
} from "element-plus";
import { computed, onMounted, reactive, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import {
  fetchChannelById,
  updateChannel,
} from "@/api/route-management";
import { fetchSuppliers, type Supplier } from "@/api/supplier";
import { runWithLoading } from "@/composables/useAsyncState";
import { $t } from "@/locales";
import { logger } from "@/utils/sys";

defineOptions({ name: "ChannelEdit" });

const route = useRoute();
const router = useRouter();
const formRef = ref<FormInstance>();
const loading = ref(false);
const submitting = ref(false);
const suppliers = ref<Supplier[]>([]);
const channelId = computed(() => route.params.id as string);

const form = reactive({
  supplier_id: "",
  channel_code: "",
  name: "",
  path_prefix: "",
  cost_price: "0",
  qps_limit: 0,
  status: "active",
});

const rules: FormRules = {
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
};

const goBack = () => {
  router.push({
    name: "ChannelDetail",
    params: { id: channelId.value },
  });
};

const loadSuppliers = async () => {
  try {
    const res = await fetchSuppliers({ status: "active", page_size: 10000 });
    suppliers.value = res.suppliers;
  } catch {
    suppliers.value = [];
  }
};

const loadData = async () => {
  try {
    await runWithLoading(loading, async () => {
      await loadSuppliers();
      const data = await fetchChannelById(channelId.value);
      Object.assign(form, {
        supplier_id: data.supplier_id,
        channel_code: data.channel_code,
        name: data.name,
        path_prefix: data.path_prefix,
        cost_price: data.cost_price,
        qps_limit: data.qps_limit,
        status: data.status,
      });

      if (
        data.supplier_id &&
        !suppliers.value.some((s) => s.id === data.supplier_id)
      ) {
        suppliers.value = [
          {
            id: data.supplier_id,
            supplier_code: data.supplier_code,
            name: data.supplier_name,
            upstream_url: "",
            status: "active",
            created_at: "",
            updated_at: "",
          },
          ...suppliers.value,
        ];
      }
    });
  } catch (error) {
    logger.error("Failed to load channel for edit:", error);
    ElMessage.error($t("channels.detail.loadFailed"));
  }
};

const submitForm = async () => {
  const valid = await formRef.value?.validate().catch(() => false);
  if (!valid) return;

  await runWithLoading(submitting, async () => {
    await updateChannel(channelId.value, { ...form });
    ElMessage.success($t("common.feedback.saveSuccess"));
    goBack();
  });
};

onMounted(loadData);
</script>

<style scoped lang="scss">
.channel-edit-page {
  .channel-edit-form {
    max-width: 720px;
  }

  .form-actions {
    margin-top: 24px;

    :deep(.el-form-item__content) {
      justify-content: flex-end;
      gap: 12px;
    }
  }
}

.qps-limit-field {
  width: 100%;

  .field-hint {
    margin: 6px 0 0;
    font-size: 12px;
    line-height: 1.5;
    color: var(--el-text-color-secondary);
  }
}
</style>
