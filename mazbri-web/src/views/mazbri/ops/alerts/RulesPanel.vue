<template>
  <div class="ops-alerts-panel">
    <OpsPanelToolbar>
      <ElButton v-if="isAdmin" type="primary" @click="openCreate">
        {{ t("ops.alerts.rules.create") }}
      </ElButton>
      <ElButton :loading="loading" @click="reload">{{
        t("ops.alerts.rules.refresh")
      }}</ElButton>
    </OpsPanelToolbar>

    <OpsCard>
      <ElTable
        v-loading="loading"
        :data="rules"
        row-key="id"
        size="small"
        stripe
        class="surface-panel-table"
      >
        <ElTableColumn
          :label="t('ops.alerts.rules.columns.enabled')"
          width="68"
        >
          <template #default="{ row }">
            <ElSwitch
              :model-value="row.enabled"
              :disabled="!isAdmin"
              @change="
                (value: string | number | boolean) =>
                  toggle(row, Boolean(value))
              "
            />
          </template>
        </ElTableColumn>
        <ElTableColumn
          prop="name"
          :label="t('ops.alerts.rules.columns.name')"
          min-width="120"
          show-overflow-tooltip
        />
        <ElTableColumn
          :label="t('ops.alerts.rules.columns.severity')"
          width="76"
        >
          <template #default="{ row }">
            <ElTag :type="alertSeverityTag(row.severity)" size="small">
              {{ t(`ops.alerts.severity.${row.severity}`) }}
            </ElTag>
          </template>
        </ElTableColumn>
        <ElTableColumn
          :label="t('ops.alerts.rules.columns.expr')"
          min-width="180"
          show-overflow-tooltip
        >
          <template #default="{ row }">
            <code class="ops-alerts__expr"
              >{{ row.kind }} · {{ row.expr }}</code
            >
          </template>
        </ElTableColumn>
        <ElTableColumn
          :label="t('ops.alerts.rules.columns.condition')"
          width="120"
        >
          <template #default="{ row }">
            {{ row.op }} {{ row.threshold }} · {{ row.for_seconds
            }}{{ t("ops.units.seconds") }}
          </template>
        </ElTableColumn>
        <ElTableColumn
          :label="t('ops.alerts.rules.columns.notify')"
          width="120"
        >
          <template #default="{ row }">
            <span v-if="row.notify?.enabled">
              {{
                t("ops.alerts.rules.notifySummary", {
                  emails: row.notify.emails?.length || 0,
                  webhooks: row.notify.webhooks?.length || 0,
                })
              }}
            </span>
            <span v-else>{{ t("ops.alerts.rules.notifyOff") }}</span>
          </template>
        </ElTableColumn>
        <ElTableColumn :label="t('ops.alerts.rules.columns.active')" width="68">
          <template #default="{ row }">
            <ElButton
              v-if="row.active_events"
              link
              type="danger"
              @click="goEvents"
            >
              {{ row.active_events }}
            </ElButton>
            <span v-else>0</span>
          </template>
        </ElTableColumn>
        <ElTableColumn
          v-if="isAdmin"
          :label="t('ops.alerts.columns.actions')"
          width="180"
        >
          <template #default="{ row }">
            <ElButton
              v-if="drillRouteOf(row)"
              link
              type="primary"
              @click="goDrill(row)"
            >
              {{ t("ops.alerts.drillDown") }}
            </ElButton>
            <ElButton link type="primary" @click="openEdit(row)">
              {{ t("ops.alerts.rules.edit") }}
            </ElButton>
            <ElButton link type="danger" @click="remove(row)">
              {{ t("ops.alerts.rules.delete") }}
            </ElButton>
          </template>
        </ElTableColumn>
      </ElTable>
    </OpsCard>

    <EditDialog
      v-model="dialogVisible"
      :title="
        editingId ? t('ops.alerts.rules.edit') : t('ops.alerts.rules.create')
      "
      width="600px"
      :cancel-text="t('ops.alerts.rules.cancel')"
      :confirm-text="t('ops.alerts.rules.save')"
      :saving="saving"
      @confirm="save"
    >
      <ElForm
        ref="formRef"
        :model="form"
        :rules="formRules"
        label-width="116px"
      >
        <ElFormItem :label="t('ops.alerts.rules.columns.name')" prop="name">
          <ElInput v-model="form.name" maxlength="128" />
        </ElFormItem>
        <ElFormItem :label="t('ops.alerts.rules.form.kind')" prop="kind">
          <ElRadioGroup v-model="form.kind">
            <ElRadio value="promql">PromQL</ElRadio>
            <ElRadio value="olap">OLAP</ElRadio>
          </ElRadioGroup>
        </ElFormItem>
        <ElFormItem prop="expr">
          <template #label>
            <span class="ops-alerts__field-label">
              {{ t("ops.alerts.rules.columns.expr") }}
              <ElTooltip
                :content="t('ops.alerts.rules.form.exprHint')"
                placement="top"
              >
                <span class="ops-alerts__label-info" tabindex="-1">
                  <ArtSvgIcon icon="ri:information-line" />
                </span>
              </ElTooltip>
            </span>
          </template>
          <ElInput
            v-model="form.expr"
            type="textarea"
            :rows="3"
            maxlength="4000"
            :placeholder="
              form.kind === 'olap'
                ? olapPlaceholder
                : 'max by (name) (circuit_breaker_state)'
            "
          />
        </ElFormItem>
        <ElFormItem
          :label="t('ops.alerts.rules.columns.condition')"
          prop="threshold"
        >
          <div class="ops-alerts__inline">
            <ElSelect v-model="form.op" style="width: 90px">
              <ElOption v-for="op in ops" :key="op" :value="op" :label="op" />
            </ElSelect>
            <ElInputNumber
              v-model="form.threshold"
              :controls="false"
              style="width: 140px"
            />
            <span>{{ t("ops.alerts.rules.form.for") }}</span>
            <ElInputNumber
              v-model="form.for_seconds"
              :min="0"
              :max="86400"
              :step="30"
            />
            <span>{{ t("ops.units.seconds") }}</span>
          </div>
        </ElFormItem>
        <ElFormItem
          :label="t('ops.alerts.rules.columns.severity')"
          prop="severity"
        >
          <ElSelect v-model="form.severity" style="width: 160px">
            <ElOption
              v-for="s in severities"
              :key="s"
              :value="s"
              :label="t(`ops.alerts.severity.${s}`)"
            />
          </ElSelect>
        </ElFormItem>
        <ElFormItem prop="labelsText">
          <template #label>
            <span class="ops-alerts__field-label">
              {{ t("ops.alerts.rules.form.labels") }}
              <ElTooltip
                :content="t('ops.alerts.rules.form.labelsInvalid')"
                placement="top"
              >
                <span class="ops-alerts__label-info" tabindex="-1">
                  <ArtSvgIcon icon="ri:information-line" />
                </span>
              </ElTooltip>
            </span>
          </template>
          <ElInput
            v-model="form.labelsText"
            type="textarea"
            :rows="2"
            placeholder="category=upstream"
          />
        </ElFormItem>
        <ElFormItem prop="emailsText">
          <template #label>
            <span class="ops-alerts__field-label">
              {{ t("ops.alerts.rules.form.emails") }}
              <ElTooltip
                :content="t('ops.alerts.rules.form.emailsHint')"
                placement="top"
              >
                <span class="ops-alerts__label-info" tabindex="-1">
                  <ArtSvgIcon icon="ri:information-line" />
                </span>
              </ElTooltip>
            </span>
          </template>
          <ElInput
            v-model="form.emailsText"
            type="textarea"
            :rows="2"
            placeholder="oncall@example.com"
          />
        </ElFormItem>
        <ElFormItem prop="webhooksText">
          <template #label>
            <span class="ops-alerts__field-label">
              {{ t("ops.alerts.rules.form.webhooks") }}
              <ElTooltip
                :content="t('ops.alerts.rules.form.webhooksHint')"
                placement="top"
              >
                <span class="ops-alerts__label-info" tabindex="-1">
                  <ArtSvgIcon icon="ri:information-line" />
                </span>
              </ElTooltip>
            </span>
          </template>
          <ElInput
            v-model="form.webhooksText"
            type="textarea"
            :rows="2"
            placeholder="https://hooks.example.com/alert"
          />
        </ElFormItem>
        <ElFormItem>
          <template #label>
            <span class="ops-alerts__field-label">
              {{ t("ops.alerts.rules.form.notifyEnabled") }}
              <ElTooltip
                :content="t('ops.alerts.rules.form.notifyEnabledHint')"
                placement="top"
              >
                <span class="ops-alerts__label-info" tabindex="-1">
                  <ArtSvgIcon icon="ri:information-line" />
                </span>
              </ElTooltip>
            </span>
          </template>
          <ElSwitch v-model="form.notifyEnabled" />
        </ElFormItem>
        <ElFormItem>
          <template #label>
            <span class="ops-alerts__field-label">
              {{ t("ops.alerts.rules.form.evaluateEnabled") }}
              <ElTooltip
                :content="t('ops.alerts.rules.form.evaluateEnabledHint')"
                placement="top"
              >
                <span class="ops-alerts__label-info" tabindex="-1">
                  <ArtSvgIcon icon="ri:information-line" />
                </span>
              </ElTooltip>
            </span>
          </template>
          <ElSwitch v-model="form.enabled" />
        </ElFormItem>
      </ElForm>
    </EditDialog>
  </div>
</template>

<script setup lang="ts">
import { onMounted, reactive, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";
import {
  ElMessage,
  ElMessageBox,
  type FormInstance,
  type FormRules,
} from "element-plus";
import EditDialog from "@/components/common/EditDialog.vue";
import OpsCard from "@/components/ops/OpsCard.vue";
import OpsPanelToolbar from "@/components/ops/OpsPanelToolbar.vue";
import {
  createAlertRule,
  deleteAlertRule,
  fetchAlertRules,
  setAlertRuleEnabled,
  updateAlertRule,
} from "@/api/alerts";
import { useOpsRole } from "@/composables/ops/useOpsRole";
import type {
  AlertOp,
  AlertRule,
  AlertRuleInput,
  AlertRuleKind,
  AlertSeverity,
} from "@/types/api/alerts";
import {
  alertSeverityTag,
  formatAlertLabelsText,
  isValidAlertEmail,
  parseAlertLabelsText,
  parseAlertListText,
} from "@/utils/ops/alerts-format";
import {
  buildAlertsRoute,
  resolveAlertDrillRoute,
} from "@/utils/ops/alerts-link";

defineOptions({ name: "OpsAlertRulesPanel" });

const { t } = useI18n();
const router = useRouter();
const { isAdmin, isBiz, isViewer } = useOpsRole();

const ops: AlertOp[] = [">", ">=", "<", "<=", "==", "!="];
const severities: AlertSeverity[] = ["critical", "warning", "info"];
const olapPlaceholder =
  '{"query":{"template":"top_by_dim","params":{"dim":"product_id","metric":"p99"}},"value_field":"value"}';

const loading = ref(false);
const saving = ref(false);
const rules = ref<AlertRule[]>([]);
const dialogVisible = ref(false);
const editingId = ref<string | null>(null);
const formRef = ref<FormInstance>();

interface RuleForm {
  name: string;
  kind: AlertRuleKind;
  expr: string;
  threshold: number;
  op: AlertOp;
  for_seconds: number;
  severity: AlertSeverity;
  labelsText: string;
  notifyEnabled: boolean;
  emailsText: string;
  webhooksText: string;
  enabled: boolean;
}

const emptyForm = (): RuleForm => ({
  name: "",
  kind: "promql",
  expr: "",
  threshold: 0,
  op: ">",
  for_seconds: 0,
  severity: "warning",
  labelsText: "",
  notifyEnabled: false,
  emailsText: "",
  webhooksText: "",
  enabled: true,
});

const form = reactive<RuleForm>(emptyForm());

const formRules: FormRules<RuleForm> = {
  name: [
    {
      required: true,
      message: t("ops.alerts.rules.form.required"),
      trigger: "blur",
    },
  ],
  expr: [
    {
      required: true,
      message: t("ops.alerts.rules.form.required"),
      trigger: "blur",
    },
  ],
  emailsText: [
    {
      validator: (_rule, value: string, callback) => {
        const emails = parseAlertListText(value);
        if (!emails.every((email) => isValidAlertEmail(email))) {
          callback(new Error(t("ops.alerts.rules.form.emailsInvalid")));
          return;
        }
        if (
          form.notifyEnabled &&
          emails.length === 0 &&
          parseAlertListText(form.webhooksText).length === 0
        ) {
          callback(new Error(t("ops.alerts.rules.form.notifyRequired")));
          return;
        }
        callback();
      },
      trigger: "blur",
    },
  ],
  labelsText: [
    {
      validator: (_rule, value: string, callback) =>
        parseAlertLabelsText(value)
          ? callback()
          : callback(new Error(t("ops.alerts.rules.form.labelsInvalid"))),
      trigger: "blur",
    },
  ],
};

function goEvents() {
  void router.push(buildAlertsRoute({ tab: "events" }));
}

function drillRouteOf(rule: AlertRule) {
  return resolveAlertDrillRoute(
    { labels: rule.labels, expr: rule.expr },
    { isAdmin: isAdmin.value, isBiz: isBiz.value, isViewer: isViewer.value },
  );
}

function goDrill(rule: AlertRule) {
  const target = drillRouteOf(rule);
  if (target) {
    void router.push(target);
  }
}

async function reload() {
  loading.value = true;
  try {
    rules.value = await fetchAlertRules();
  } finally {
    loading.value = false;
  }
}

function openCreate() {
  editingId.value = null;
  Object.assign(form, emptyForm());
  dialogVisible.value = true;
}

function openEdit(rule: AlertRule) {
  editingId.value = rule.id;
  Object.assign(form, {
    name: rule.name,
    kind: rule.kind,
    expr: rule.expr,
    threshold: rule.threshold,
    op: rule.op,
    for_seconds: rule.for_seconds,
    severity: rule.severity,
    labelsText: formatAlertLabelsText(rule.labels),
    notifyEnabled: rule.notify?.enabled ?? false,
    emailsText: (rule.notify?.emails || []).join("\n"),
    webhooksText: (rule.notify?.webhooks || []).join("\n"),
    enabled: rule.enabled,
  });
  dialogVisible.value = true;
}

async function save() {
  if (!formRef.value) return;
  const valid = await formRef.value.validate().catch(() => false);
  if (!valid) return;
  const payload: AlertRuleInput = {
    name: form.name,
    kind: form.kind,
    expr: form.expr,
    threshold: form.threshold,
    op: form.op,
    for_seconds: form.for_seconds,
    severity: form.severity,
    labels: parseAlertLabelsText(form.labelsText) || {},
    notify: {
      enabled: form.notifyEnabled,
      emails: parseAlertListText(form.emailsText),
      webhooks: parseAlertListText(form.webhooksText),
    },
    enabled: form.enabled,
  };
  saving.value = true;
  try {
    if (editingId.value) {
      await updateAlertRule(editingId.value, payload);
    } else {
      await createAlertRule(payload);
    }
    ElMessage.success(t("ops.alerts.rules.saved"));
    dialogVisible.value = false;
    await reload();
  } finally {
    saving.value = false;
  }
}

async function toggle(rule: AlertRule, enabled: boolean) {
  await setAlertRuleEnabled(rule.id, enabled);
  ElMessage.success(t("ops.alerts.rules.saved"));
  await reload();
}

async function remove(rule: AlertRule) {
  try {
    await ElMessageBox.confirm(
      t("ops.alerts.rules.deleteConfirm", { name: rule.name }),
      t("ops.alerts.rules.delete"),
      { type: "warning" },
    );
  } catch {
    return;
  }
  await deleteAlertRule(rule.id);
  ElMessage.success(t("ops.alerts.rules.deleted"));
  await reload();
}

onMounted(reload);
</script>

<style scoped>
.ops-alerts__field-label {
  display: inline-flex;
  align-items: flex-start;
  gap: 2px;
}
.ops-alerts__label-info {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  margin-top: -2px;
  color: var(--el-text-color-secondary);
  font-size: 13px;
  line-height: 1;
  cursor: help;
  transition: color 0.15s ease;
}
.ops-alerts__label-info:hover {
  color: var(--el-color-primary);
}
.ops-alerts__expr {
  font-size: 12px;
  word-break: break-all;
}
.ops-alerts__inline {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
}
</style>
