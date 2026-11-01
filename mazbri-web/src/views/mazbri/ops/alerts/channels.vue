<template>
  <div class="ops-alerts art-full-height">
    <div class="ops-alerts__toolbar">
      <h2>{{ t("menus.mazbri.alertChannels") }}</h2>
      <div class="ops-alerts__actions">
        <ElButton v-if="canEdit" type="primary" @click="openCreate">{{
          t("ops.alerts.channels.create")
        }}</ElButton>
        <ElButton :loading="loading" @click="reload">{{ t("common.refresh") }}</ElButton>
      </div>
    </div>
    <div class="ops-alerts__card">
      <p class="ops-alerts__hint">{{ t("ops.alerts.channels.hint") }}</p>
      <ElTable v-loading="loading" :data="rows" row-key="id" size="small">
        <ElTableColumn prop="name" :label="t('ops.alerts.channels.name')" min-width="140" />
        <ElTableColumn prop="type" :label="t('ops.alerts.channels.type')" width="120" />
        <ElTableColumn :label="t('ops.alerts.channels.config')" min-width="220">
          <template #default="{ row }">{{ JSON.stringify(row.config || {}) }}</template>
        </ElTableColumn>
        <ElTableColumn :label="t('ops.alerts.channels.hasSecret')" width="100">
          <template #default="{ row }">
            <ElTag size="small" :type="row.has_secret ? 'warning' : 'info'">
              {{ row.has_secret ? "yes" : "no" }}
            </ElTag>
          </template>
        </ElTableColumn>
        <ElTableColumn :label="t('ops.alerts.rules.columns.enabled')" width="90">
          <template #default="{ row }">
            <ElTag size="small">{{ row.enabled ? "ON" : "OFF" }}</ElTag>
          </template>
        </ElTableColumn>
        <ElTableColumn v-if="canEdit" :label="t('common.actions')" width="100">
          <template #default="{ row }">
            <ElButton link type="danger" @click="remove(row.id)">{{ t("common.delete") }}</ElButton>
          </template>
        </ElTableColumn>
      </ElTable>
    </div>

    <ElDialog v-model="dialog" :title="t('ops.alerts.channels.create')" width="520px">
      <ElForm label-width="100px">
        <ElFormItem :label="t('ops.alerts.channels.name')">
          <ElInput v-model="form.name" />
        </ElFormItem>
        <ElFormItem :label="t('ops.alerts.channels.type')">
          <ElSelect v-model="form.type" style="width: 100%">
            <ElOption value="email" label="email" />
            <ElOption value="webhook" label="webhook" />
            <ElOption value="dingtalk" label="dingtalk (stub)" />
            <ElOption value="feishu" label="feishu (stub)" />
            <ElOption value="wecom" label="wecom (stub)" />
          </ElSelect>
        </ElFormItem>
        <ElFormItem :label="t('ops.alerts.channels.config')">
          <ElInput v-model="form.configText" type="textarea" :rows="5" />
        </ElFormItem>
      </ElForm>
      <template #footer>
        <ElButton @click="dialog = false">{{ t("common.cancel") }}</ElButton>
        <ElButton type="primary" :loading="saving" @click="save">{{ t("common.confirm") }}</ElButton>
      </template>
    </ElDialog>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import { useI18n } from "vue-i18n";
import { ElMessage, ElMessageBox } from "element-plus";
import {
  createNotifyChannel,
  deleteNotifyChannel,
  fetchNotifyChannels,
  type NotifyChannel,
} from "@/api/alerts";
import { useUserStore } from "@/store/modules/user";

defineOptions({ name: "OpsAlertChannels" });
const { t } = useI18n();
const userStore = useUserStore();
const roles = computed(() => (userStore.info?.roles as string[]) || []);
const canEdit = computed(() =>
  roles.value.some((role) => role === "R_ADMIN" || role === "R_FINANCE"),
);
const loading = ref(false);
const saving = ref(false);
const rows = ref<NotifyChannel[]>([]);
const dialog = ref(false);
const form = reactive({
  name: "",
  type: "email" as NotifyChannel["type"],
  configText: '{"emails":["alerts@example.com"]}',
});

async function reload() {
  loading.value = true;
  try {
    rows.value = (await fetchNotifyChannels()) || [];
  } finally {
    loading.value = false;
  }
}
function openCreate() {
  form.name = "dev19-mock-email";
  form.type = "email";
  form.configText = '{"emails":["alerts@example.com"]}';
  dialog.value = true;
}
async function save() {
  saving.value = true;
  try {
    await createNotifyChannel({
      name: form.name,
      type: form.type,
      config: JSON.parse(form.configText || "{}"),
      enabled: true,
    });
    dialog.value = false;
    ElMessage.success(t("common.success"));
    await reload();
  } catch (e) {
    ElMessage.error((e as Error).message || t("common.failed"));
  } finally {
    saving.value = false;
  }
}
async function remove(id: string) {
  await ElMessageBox.confirm(t("common.confirmDelete"), { type: "warning" });
  await deleteNotifyChannel(id);
  await reload();
}
onMounted(reload);
</script>

<style scoped>
.ops-alerts__toolbar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 12px;
}
.ops-alerts__card {
  background: var(--el-bg-color);
  border-radius: 8px;
  padding: 16px;
}
.ops-alerts__hint {
  color: var(--el-text-color-secondary);
  margin-bottom: 12px;
}
</style>
