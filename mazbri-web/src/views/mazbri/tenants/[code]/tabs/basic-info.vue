<template>
  <div class="basic-info-tab">
    <ElCard v-loading="loading" shadow="never">
      <template #header>
        <SurfaceCardHeader :title="$t('tenant.detail.basicInfo.title')">
          <template #actions>
            <ElButton
              v-roles="['R_ADMIN', 'R_FINANCE']"
              type="primary"
              @click="handleEdit"
              >{{ $t("common.actions.modify") }}</ElButton
            >
          </template>
        </SurfaceCardHeader>
      </template>

      <ElDescriptions :column="2" border>
        <ElDescriptionsItem :label="$t('tenant.fields.code')">{{
          tenant?.tenant_code
        }}</ElDescriptionsItem>
        <ElDescriptionsItem :label="$t('tenant.fields.accountId')">{{
          tenant?.tenant_id
        }}</ElDescriptionsItem>
        <ElDescriptionsItem :label="$t('common.fields.name')">{{
          tenant?.name
        }}</ElDescriptionsItem>
        <ElDescriptionsItem :label="$t('tenant.fields.contactPerson')">{{
          tenant?.contact_person || "-"
        }}</ElDescriptionsItem>
        <ElDescriptionsItem :label="$t('tenant.fields.contactPhone')">{{
          tenant?.contact_phone || "-"
        }}</ElDescriptionsItem>
        <ElDescriptionsItem :label="$t('tenant.fields.contactEmail')">{{
          tenant?.contact_email || "-"
        }}</ElDescriptionsItem>
        <ElDescriptionsItem :label="$t('common.fields.status')">
          <ElSwitch
            :model-value="tenant?.enabled ?? false"
            :loading="statusLoading"
            :disabled="!userStore.canWriteBusiness"
            :active-text="$t('tenant.detail.status.active')"
            :inactive-text="$t('tenant.detail.status.suspended')"
            @change="handleStatusChange"
          />
        </ElDescriptionsItem>
        <ElDescriptionsItem :label="$t('common.fields.createdAt')">{{
          formatDate(tenant?.created_at || "")
        }}</ElDescriptionsItem>
        <ElDescriptionsItem :label="$t('common.fields.updatedAt')">{{
          formatDate(tenant?.updated_at || "")
        }}</ElDescriptionsItem>
      </ElDescriptions>

      <EditDialog
        v-model="editVisible"
        :title="$t('tenant.detail.basicInfo.editTitle')"
        width="560px"
        :saving="saving"
        @confirm="handleSave"
      >
        <ElForm
          ref="formRef"
          :model="editForm"
          :rules="rules"
          label-width="100px"
        >
          <ElFormItem :label="$t('common.fields.name')" prop="name">
            <ElInput v-model="editForm.name" />
          </ElFormItem>
          <ElFormItem :label="$t('tenant.fields.contactPerson')">
            <ElInput v-model="editForm.contact_person" />
          </ElFormItem>
          <ElFormItem :label="$t('tenant.fields.contactPhone')">
            <ElInput v-model="editForm.contact_phone" />
          </ElFormItem>
          <ElFormItem
            :label="$t('tenant.fields.contactEmail')"
            prop="contact_email"
          >
            <ElInput v-model="editForm.contact_email" />
          </ElFormItem>
        </ElForm>
      </EditDialog>
    </ElCard>


    <GateSummaryCard
      v-if="tenant?.tenant_code"
      :tenant-code="tenant.tenant_code"
    />
  </div>
</template>

<script setup lang="ts">
import { ElMessage, ElMessageBox, type FormInstance, type FormRules } from "element-plus";
import { $t } from "@/locales";
import type {
  Tenant,
  UpdateTenantRequest,
} from "@/types/api/tenant";
import { updateTenant, setTenantEnabled } from "@/api/tenant";
import EditDialog from "@/components/common/EditDialog.vue";
import SurfaceCardHeader from "@/components/common/surface-card-header/index.vue";
import GateSummaryCard from "./modules/gate-summary-card.vue";
import { runWithLoading } from "@/composables/useAsyncState";
import { useUserStore } from "@/store/modules/user";
import { logger } from "@/utils/sys";
import { formatDateTime as formatDate } from "@/utils/format";
defineOptions({ name: "BasicInfoTab" });
const props = defineProps<{
  tenant: Tenant | null;
  loading: boolean;
}>();
const emit = defineEmits<{
  refresh: [];
}>();
const editVisible = ref(false);
const saving = ref(false);
const statusLoading = ref(false);
const userStore = useUserStore();
const formRef = ref<FormInstance>();
const editForm = reactive<UpdateTenantRequest>({
  name: "",
  contact_person: "",
  contact_phone: "",
  contact_email: "",
});
const rules: FormRules = {
  name: [
    {
      required: true,
      message: $t("tenant.detail.validation.nameRequired"),
      trigger: "blur",
    },
    {
      min: 2,
      max: 100,
      message: $t("tenant.detail.validation.nameLength"),
      trigger: "blur",
    },
  ],
  contact_email: [
    {
      type: "email",
      message: $t("tenant.detail.validation.email"),
      trigger: "blur",
    },
  ],
};
const syncEditForm = (tenant: Tenant) => {
  editForm.name = tenant.name;
  editForm.contact_person = tenant.contact_person || "";
  editForm.contact_phone = tenant.contact_phone || "";
  editForm.contact_email = tenant.contact_email || "";
};
const handleEdit = () => {
  if (!props.tenant) return;
  syncEditForm(props.tenant);
  editVisible.value = true;
};
const handleSave = async () => {
  const valid = await formRef.value?.validate().catch(() => false);
  if (!valid || !props.tenant) return;
  try {
    await runWithLoading(saving, async () => {
      await updateTenant(props.tenant!.tenant_code, toRaw(editForm));
      ElMessage.success($t("common.feedback.saveSuccess"));
      editVisible.value = false;
      emit("refresh");
    });
  } catch (error) {
    logger.error("Failed to update tenant:", error);
  }
};
const handleStatusChange = async (active: string | number | boolean) => {
  const boolActive = Boolean(active);
  if (!props.tenant) return;
  try {
    await ElMessageBox.confirm(
      boolActive
        ? $t("tenant.messages.enableTenantConfirm", {
            name: props.tenant.name,
          })
        : $t("tenant.messages.disableTenantConfirm", {
            name: props.tenant.name,
          }),
      $t("common.tips"),
      {
        confirmButtonText: $t("common.confirm"),
        cancelButtonText: $t("common.cancel"),
        type: "warning",
      },
    );
    await runWithLoading(statusLoading, async () => {
      await setTenantEnabled(props.tenant!.tenant_code, {
        enabled: boolActive,
      });
      ElMessage.success(
        boolActive
          ? $t("common.feedback.enabled")
          : $t("common.feedback.disabled"),
      );
      emit("refresh");
    });
  } catch (error) {
    if (error !== "cancel") {
      logger.error("Failed to change status:", error);
    }

    emit("refresh");
  }
};
watch(
  () => props.tenant,
  (val) => {
    if (val && !editVisible.value) {
      syncEditForm(val);
    }
  },
);
</script>
