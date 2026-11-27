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
      <ArtTable
        :loading="loading"
        :data="rows"
        :columns="silenceColumns"
        row-key="id"
        size="small"
        stripe
        class="surface-panel-table"
      >
        <template #matchers="{ row }">
          {{ formatAlertMatchers(row.matchers, t("common.all")) }}
        </template>
        <template #window="{ row }">
          {{ formatAlertTime(row.starts_at) }} → {{ formatAlertTime(row.ends_at) }}
        </template>
        <template #operations="{ row }">
          <ElButton link type="danger" @click="remove(row.id)">{{
            t("common.delete")
          }}</ElButton>
        </template>
      </ArtTable>
    </OpsCard>

    <OpsCard :title="t('ops.alerts.inhibits.title')">
      <template #extra>
        <ElButton v-if="isAdmin" type="primary" @click="openInhibit">{{
          t("ops.alerts.inhibits.create")
        }}</ElButton>
      </template>
      <ArtTable
        :loading="loadingInhibits"
        :data="inhibits"
        :columns="inhibitColumns"
        row-key="id"
        size="small"
        stripe
        class="surface-panel-table"
      >
        <template #source="{ row }">
          {{ formatAlertMatchers(row.source_matchers, t("common.all")) }}
        </template>
        <template #target="{ row }">
          {{ formatAlertMatchers(row.target_matchers, t("common.all")) }}
        </template>
        <template #equal="{ row }">
          {{ row.equal_labels?.join(", ") || "—" }}
        </template>
        <template #enabled="{ row }">
          <StatusIndicator :type="row.enabled ? 'success' : 'info'">{{
            row.enabled
              ? t("common.status.enabled")
              : t("common.status.disabled")
          }}</StatusIndicator>
        </template>
        <template #operations="{ row }">
          <ElButton link type="danger" @click="removeInhibit(row.id)">{{
            t("common.delete")
          }}</ElButton>
        </template>
      </ArtTable>
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
            value-format="YYYY-MM-DD HH:mm:ss"
          />
        </ElFormItem>
        <ElFormItem :label="t('ops.alerts.silences.endsAt')">
          <ElDatePicker
            v-model="form.ends_at"
            type="datetime"
            value-format="YYYY-MM-DD HH:mm:ss"
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
import { computed, onMounted, reactive, ref } from "vue";
import { useI18n } from "vue-i18n";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-dialog-service";
import { formatDateTime, parseDisplayWallClockMs } from "@/utils/format";
import { ElMessage, ElMessageBox } from "element-plus";
import ArtTable from "@/components/core/tables/art-table/index.vue";
import StatusIndicator from "@/components/common/StatusIndicator.vue";
import EditDialog from "@/components/common/EditDialog.vue";
import OpsCard from "@/components/ops/OpsCard.vue";
import OpsPanelToolbar from "@/components/ops/OpsPanelToolbar.vue";
import type { ColumnOption } from "@/types/component";
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

const silenceColumns = computed<ColumnOption<AlertSilence>[]>(() => {
  const cols: ColumnOption<AlertSilence>[] = [
    {
      prop: "matchers",
      label: t("ops.alerts.silences.matchers"),
      minWidth: 200,
      showOverflowTooltip: true,
      useSlot: true,
    },
    {
      prop: "window",
      label: t("ops.alerts.silences.window"),
      minWidth: 280,
      useSlot: true,
    },
    {
      prop: "created_by",
      label: t("ops.alerts.silences.createdBy"),
      width: 120,
    },
    {
      prop: "comment",
      label: t("ops.alerts.silences.comment"),
      minWidth: 140,
      showOverflowTooltip: true,
    },
  ];
  if (isAdmin.value) {
    cols.push({
      prop: "operations",
      label: t("common.fields.operations"),
      width: 100,
      useSlot: true,
    });
  }
  return cols;
});

const inhibitColumns = computed<ColumnOption<AlertInhibit>[]>(() => {
  const cols: ColumnOption<AlertInhibit>[] = [
    {
      prop: "name",
      label: t("ops.alerts.inhibits.name"),
      minWidth: 140,
      showOverflowTooltip: true,
    },
    {
      prop: "source",
      label: t("ops.alerts.inhibits.source"),
      minWidth: 160,
      showOverflowTooltip: true,
      useSlot: true,
    },
    {
      prop: "target",
      label: t("ops.alerts.inhibits.target"),
      minWidth: 160,
      showOverflowTooltip: true,
      useSlot: true,
    },
    {
      prop: "equal",
      label: t("ops.alerts.inhibits.equal"),
      minWidth: 120,
      useSlot: true,
    },
    {
      prop: "enabled",
      label: t("ops.alerts.rules.columns.enabled"),
      width: 90,
      useSlot: true,
    },
  ];
  if (isAdmin.value) {
    cols.push({
      prop: "operations",
      label: t("common.fields.operations"),
      width: 100,
      useSlot: true,
    });
  }
  return cols;
});

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
  name: "",
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
  const now = Date.now();
  const end = now + 10 * 60 * 1000;
  form.starts_at = formatDateTime(now);
  form.ends_at = formatDateTime(end);
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
      starts_at: new Date(parseDisplayWallClockMs(form.starts_at)).toISOString(),
      ends_at: new Date(parseDisplayWallClockMs(form.ends_at)).toISOString(),
      comment: form.comment,
    });
    dialog.value = false;
    ElMessage.success(t("common.success"));
    await reload();
  } catch (e) {
    if (isMfaStepUpCancelled(e)) return;
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
    if (isMfaStepUpCancelled(e)) return;
    ElMessage.error((e as Error).message || t("common.failed"));
  } finally {
    saving.value = false;
  }
}

async function remove(id: string) {
  try {
    await ElMessageBox.confirm(t("common.confirmDelete"), { type: "warning" });
  } catch {
    return;
  }
  try {
    await deleteAlertSilence(id);
    await reload();
  } catch (e) {
    if (isMfaStepUpCancelled(e)) return;
  }
}

async function removeInhibit(id: string) {
  try {
    await ElMessageBox.confirm(t("common.confirmDelete"), { type: "warning" });
  } catch {
    return;
  }
  try {
    await deleteAlertInhibit(id);
    await reload();
  } catch (e) {
    if (isMfaStepUpCancelled(e)) return;
  }
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
