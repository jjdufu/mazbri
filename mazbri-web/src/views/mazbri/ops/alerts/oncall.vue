<template>
  <div class="ops-alerts art-full-height">
    <div class="ops-alerts__toolbar">
      <h2>{{ t("menus.mazbri.alertOncall") }}</h2>
      <div class="ops-alerts__actions">
        <ElButton v-if="canEdit" type="primary" @click="openCreate">{{
          t("ops.alerts.oncall.create")
        }}</ElButton>
        <ElButton :loading="loading" @click="reload">{{ t("common.refresh") }}</ElButton>
      </div>
    </div>
    <div class="ops-alerts__card">
      <p class="ops-alerts__hint">{{ t("ops.alerts.oncall.hint") }}</p>
      <ElTable v-loading="loading" :data="rows" row-key="id" size="small">
        <ElTableColumn prop="name" :label="t('ops.alerts.oncall.name')" min-width="140" />
        <ElTableColumn prop="timezone" :label="t('ops.alerts.oncall.timezone')" width="140" />
        <ElTableColumn :label="t('ops.alerts.oncall.layers')" min-width="200">
          <template #default="{ row }">{{ summarize(row.layers) }}</template>
        </ElTableColumn>
        <ElTableColumn :label="t('ops.alerts.oncall.escalation')" min-width="220">
          <template #default="{ row }">{{ summarize(row.escalation) }}</template>
        </ElTableColumn>
        <ElTableColumn :label="t('ops.alerts.rules.columns.enabled')" width="90">
          <template #default="{ row }">
            <ElTag size="small">{{ row.enabled ? "ON" : "OFF" }}</ElTag>
          </template>
        </ElTableColumn>
        <ElTableColumn v-if="canEdit" :label="t('common.actions')" width="140">
          <template #default="{ row }">
            <ElButton link type="primary" @click="openEdit(row)">{{ t("common.edit") }}</ElButton>
            <ElButton link type="danger" @click="remove(row.id)">{{ t("common.delete") }}</ElButton>
          </template>
        </ElTableColumn>
      </ElTable>
    </div>

    <ElDialog v-model="dialog" :title="dialogTitle" width="640px">
      <ElForm label-width="100px">
        <ElFormItem :label="t('ops.alerts.oncall.name')">
          <ElInput v-model="form.name" />
        </ElFormItem>
        <ElFormItem :label="t('ops.alerts.oncall.timezone')">
          <ElInput v-model="form.timezone" />
        </ElFormItem>
        <ElFormItem :label="t('ops.alerts.oncall.layers')">
          <ElInput v-model="form.layersText" type="textarea" :rows="5" />
        </ElFormItem>
        <ElFormItem :label="t('ops.alerts.oncall.escalation')">
          <ElInput v-model="form.escalationText" type="textarea" :rows="5" />
        </ElFormItem>
        <ElFormItem :label="t('ops.alerts.rules.columns.enabled')">
          <ElSwitch v-model="form.enabled" />
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
  createOncallSchedule,
  deleteOncallSchedule,
  fetchOncallSchedules,
  updateOncallSchedule,
  type OncallSchedule,
} from "@/api/alerts";
import { useUserStore } from "@/store/modules/user";

defineOptions({ name: "OpsAlertOncall" });
const { t } = useI18n();
const userStore = useUserStore();
const roles = computed(() => (userStore.info?.roles as string[]) || []);
const canEdit = computed(() =>
  roles.value.some((role) => role === "R_ADMIN" || role === "R_FINANCE"),
);
const loading = ref(false);
const saving = ref(false);
const rows = ref<OncallSchedule[]>([]);
const dialog = ref(false);
const editingId = ref<string | null>(null);
const dialogTitle = computed(() =>
  editingId.value ? t("ops.alerts.oncall.edit") : t("ops.alerts.oncall.create"),
);
const form = reactive({
  name: "",
  timezone: "Asia/Shanghai",
  layersText: "[]",
  escalationText: "[]",
  enabled: true,
});

function summarize(v: unknown) {
  try {
    return JSON.stringify(v);
  } catch {
    return "—";
  }
}
function defaultEscalation() {
  return JSON.stringify(
    [
      { after_minutes: 0, users: ["jwczjzl"], channel_ids: [] },
      { after_minutes: 15, users: ["jwczjzl"], channel_ids: [] },
    ],
    null,
    2,
  );
}
function defaultLayers() {
  return JSON.stringify(
    [
      {
        name: "primary",
        starts_at: new Date().toISOString(),
        rotation_minutes: 10080,
        users: ["jwczjzl"],
      },
    ],
    null,
    2,
  );
}
async function reload() {
  loading.value = true;
  try {
    rows.value = (await fetchOncallSchedules()) || [];
  } finally {
    loading.value = false;
  }
}
function openCreate() {
  editingId.value = null;
  form.name = "default";
  form.timezone = "Asia/Shanghai";
  form.layersText = defaultLayers();
  form.escalationText = defaultEscalation();
  form.enabled = true;
  dialog.value = true;
}
function openEdit(row: OncallSchedule) {
  editingId.value = row.id;
  form.name = row.name;
  form.timezone = row.timezone;
  form.layersText = JSON.stringify(row.layers ?? [], null, 2);
  form.escalationText = JSON.stringify(row.escalation ?? [], null, 2);
  form.enabled = row.enabled;
  dialog.value = true;
}
async function save() {
  saving.value = true;
  try {
    const payload = {
      name: form.name,
      timezone: form.timezone,
      layers: JSON.parse(form.layersText || "[]"),
      escalation: JSON.parse(form.escalationText || "[]"),
      enabled: form.enabled,
    };
    if (editingId.value) await updateOncallSchedule(editingId.value, payload);
    else await createOncallSchedule(payload);
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
  await deleteOncallSchedule(id);
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
