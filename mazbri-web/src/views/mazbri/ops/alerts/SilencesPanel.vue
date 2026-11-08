<template>
  <div class="ops-alerts-panel">
    <OpsPanelToolbar>
      <ElButton :loading="loading" @click="reload">{{
        t("common.refresh")
      }}</ElButton>
    </OpsPanelToolbar>

    <OpsCard
      :title="t('menus.mazbri.alertSilences')"
      :info="t('ops.alerts.silences.hint')"
    >
      <template #extra>
        <ElButton v-if="isAdmin" type="primary" @click="openCreate">{{
          t("ops.alerts.silences.create")
        }}</ElButton>
      </template>
      <ElTable v-loading="loading" :data="rows" row-key="id" size="small">
        <ElTableColumn
          :label="t('ops.alerts.silences.matchers')"
          min-width="200"
        >
          <template #default="{ row }">{{
            formatAlertMatchers(row.matchers)
          }}</template>
        </ElTableColumn>
        <ElTableColumn
          :label="t('ops.alerts.silences.window')"
          min-width="280"
        >
          <template #default="{ row }"
            >{{ formatAlertTime(row.starts_at) }} →
            {{ formatAlertTime(row.ends_at) }}</template
          >
        </ElTableColumn>
        <ElTableColumn
          prop="created_by"
          :label="t('ops.alerts.silences.createdBy')"
          width="120"
        />
        <ElTableColumn
          prop="comment"
          :label="t('ops.alerts.silences.comment')"
          min-width="140"
        />
        <ElTableColumn v-if="isAdmin" :label="t('common.actions')" width="100">
          <template #default="{ row }">
            <ElButton link type="danger" @click="remove(row.id)">{{
              t("common.delete")
            }}</ElButton>
          </template>
        </ElTableColumn>
      </ElTable>
    </OpsCard>

    <OpsCard :title="t('ops.alerts.inhibits.title')">
      <template #extra>
        <ElButton v-if="isAdmin" type="primary" @click="openInhibit">{{
          t("ops.alerts.inhibits.create")
        }}</ElButton>
      </template>
      <ElTable
        v-loading="loadingInhibits"
        :data="inhibits"
        row-key="id"
        size="small"
      >
        <ElTableColumn
          prop="name"
          :label="t('ops.alerts.inhibits.name')"
          min-width="140"
        />
        <ElTableColumn :label="t('ops.alerts.inhibits.source')" min-width="160">
          <template #default="{ row }">{{
            formatAlertMatchers(row.source_matchers)
          }}</template>
        </ElTableColumn>
        <ElTableColumn :label="t('ops.alerts.inhibits.target')" min-width="160">
          <template #default="{ row }">{{
            formatAlertMatchers(row.target_matchers)
          }}</template>
        </ElTableColumn>
        <ElTableColumn :label="t('ops.alerts.inhibits.equal')" min-width="120">
          <template #default="{ row }">{{
            row.equal_labels?.join(", ") || "—"
          }}</template>
        </ElTableColumn>
        <ElTableColumn
          :label="t('ops.alerts.rules.columns.enabled')"
          width="90"
        >
          <template #default="{ row }">
            <ElTag size="small" :type="row.enabled ? 'success' : 'info'">{{
              row.enabled
                ? t("common.status.enabled")
                : t("common.status.disabled")
            }}</ElTag>
          </template>
        </ElTableColumn>
        <ElTableColumn v-if="isAdmin" :label="t('common.actions')" width="100">
          <template #default="{ row }">
            <ElButton link type="danger" @click="removeInhibit(row.id)">{{
              t("common.delete")
            }}</ElButton>
          </template>
        </ElTableColumn>
      </ElTable>
    </OpsCard>

    <EditDialog
      v-model="dialog"
      :title="t('ops.alerts.silences.create')"
      width="480px"
      :saving="saving"
      @confirm="save"
    >
      <ElForm label-width="100px">
        <ElFormItem :label="t('ops.alerts.silences.matchers')">
          <ElInput
            v-model="form.matchersText"
            type="textarea"
            :rows="3"
            placeholder='{"product_id":"A"}'
          />
        </ElFormItem>
        <ElFormItem :label="t('ops.alerts.silences.startsAt')">
          <ElDatePicker
            v-model="form.starts_at"
            type="datetime"
            value-format="YYYY-MM-DDTHH:mm:ss[Z]"
          />
        </ElFormItem>
        <ElFormItem :label="t('ops.alerts.silences.endsAt')">
          <ElDatePicker
            v-model="form.ends_at"
            type="datetime"
            value-format="YYYY-MM-DDTHH:mm:ss[Z]"
          />
        </ElFormItem>
        <ElFormItem :label="t('ops.alerts.silences.comment')">
          <ElInput v-model="form.comment" />
        </ElFormItem>
      </ElForm>
    </EditDialog>

    <EditDialog
      v-model="inhibitDialog"
      :title="t('ops.alerts.inhibits.create')"
      width="520px"
      :saving="saving"
      @confirm="saveInhibit"
    >
      <ElForm label-width="110px">
        <ElFormItem :label="t('ops.alerts.inhibits.name')">
          <ElInput v-model="inhibitForm.name" />
        </ElFormItem>
        <ElFormItem :label="t('ops.alerts.inhibits.source')">
          <ElInput
            v-model="inhibitForm.sourceText"
            type="textarea"
            :rows="2"
            placeholder='{"category":"upstream"}'
          />
        </ElFormItem>
        <ElFormItem :label="t('ops.alerts.inhibits.target')">
          <ElInput
            v-model="inhibitForm.targetText"
            type="textarea"
            :rows="2"
            placeholder='{"category":"business"}'
          />
        </ElFormItem>
        <ElFormItem :label="t('ops.alerts.inhibits.equal')">
          <ElInput v-model="inhibitForm.equalText" placeholder="product_id" />
        </ElFormItem>
      </ElForm>
    </EditDialog>
  </div>
</template>

<script setup lang="ts">
import { onMounted, reactive, ref } from "vue";
import { useI18n } from "vue-i18n";
import { ElMessage, ElMessageBox } from "element-plus";
import EditDialog from "@/components/common/EditDialog.vue";
import OpsCard from "@/components/ops/OpsCard.vue";
import OpsPanelToolbar from "@/components/ops/OpsPanelToolbar.vue";
import {
  createAlertInhibit,
  createAlertSilence,
  deleteAlertInhibit,
  deleteAlertSilence,
  fetchAlertInhibits,
  fetchAlertSilences,
  type AlertInhibit,
  type AlertSilence,
} from "@/api/alerts";
import { useOpsRole } from "@/composables/ops/useOpsRole";
import {
  formatAlertMatchers,
  formatAlertTime,
} from "@/utils/ops/alerts-format";

defineOptions({ name: "OpsAlertSilencesPanel" });

const { t } = useI18n();
const { isAdmin } = useOpsRole();

const loading = ref(false);
const loadingInhibits = ref(false);
const saving = ref(false);
const rows = ref<AlertSilence[]>([]);
const inhibits = ref<AlertInhibit[]>([]);
const dialog = ref(false);
const inhibitDialog = ref(false);
const form = reactive({
  matchersText: '{"product_id":"A"}',
  starts_at: "",
  ends_at: "",
  comment: "",
});
const inhibitForm = reactive({
  name: "熔断抑制错误率",
  sourceText: '{"category":"upstream"}',
  targetText: '{"category":"business"}',
  equalText: "product_id",
});

function parseJson(text: string): Record<string, string> {
  const v = JSON.parse(text || "{}");
  if (!v || typeof v !== "object" || Array.isArray(v)) {
    throw new Error("invalid");
  }
  return v as Record<string, string>;
}

async function reload() {
  loading.value = true;
  loadingInhibits.value = true;
  try {
    rows.value = (await fetchAlertSilences()) || [];
    inhibits.value = (await fetchAlertInhibits()) || [];
  } finally {
    loading.value = false;
    loadingInhibits.value = false;
  }
}

function openCreate() {
  const now = new Date();
  const end = new Date(now.getTime() + 10 * 60 * 1000);
  form.starts_at = now.toISOString();
  form.ends_at = end.toISOString();
  form.comment = "";
  dialog.value = true;
}

function openInhibit() {
  inhibitDialog.value = true;
}

async function save() {
  saving.value = true;
  try {
    await createAlertSilence({
      matchers: parseJson(form.matchersText),
      starts_at: new Date(form.starts_at).toISOString(),
      ends_at: new Date(form.ends_at).toISOString(),
      comment: form.comment,
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

async function saveInhibit() {
  saving.value = true;
  try {
    await createAlertInhibit({
      name: inhibitForm.name,
      source_matchers: parseJson(inhibitForm.sourceText),
      target_matchers: parseJson(inhibitForm.targetText),
      equal_labels: inhibitForm.equalText
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean),
      enabled: true,
    });
    inhibitDialog.value = false;
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
  await deleteAlertSilence(id);
  await reload();
}

async function removeInhibit(id: string) {
  await ElMessageBox.confirm(t("common.confirmDelete"), { type: "warning" });
  await deleteAlertInhibit(id);
  await reload();
}

onMounted(reload);
</script>

<style scoped>
.ops-alerts-panel {
  display: flex;
  flex-direction: column;
  gap: var(--ops-section-gap, 16px);
}
</style>
