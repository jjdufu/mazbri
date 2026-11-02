<template>
  <OpsPage :title="t('menus.mazbri.alertRules')">
    <template #actions>
      <ElButton v-if="canEdit" type="primary" @click="openCreate">
        {{ t("ops.alerts.rules.create") }}
      </ElButton>
      <ElButton :loading="loading" @click="reload">{{
        t("ops.alerts.rules.refresh")
      }}</ElButton>
    </template>

    <OpsCard>
      <p class="ops-alerts__hint">{{ t("ops.alerts.rules.hint") }}</p>
      <ElTable v-loading="loading" :data="rules" row-key="id" size="small">
        <ElTableColumn
          :label="t('ops.alerts.rules.columns.enabled')"
          width="90"
        >
          <template #default="{ row }">
            <ElSwitch
              :model-value="row.enabled"
              :disabled="!canEdit"
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
          min-width="130"
        />
        <ElTableColumn
          :label="t('ops.alerts.rules.columns.severity')"
          width="100"
        >
          <template #default="{ row }">
            <ElTag :type="severityTag(row.severity)" size="small">
              {{ t(`ops.alerts.severity.${row.severity}`) }}
            </ElTag>
          </template>
        </ElTableColumn>
        <ElTableColumn
          :label="t('ops.alerts.rules.columns.expr')"
          min-width="320"
        >
          <template #default="{ row }">
            <code class="ops-alerts__expr"
              >{{ row.kind }} · {{ row.expr }}</code
            >
          </template>
        </ElTableColumn>
        <ElTableColumn
          :label="t('ops.alerts.rules.columns.condition')"
          width="150"
        >
          <template #default="{ row }">
            {{ row.op }} {{ row.threshold }} · {{ row.for_seconds }}s
          </template>
        </ElTableColumn>
        <ElTableColumn
          :label="t('ops.alerts.rules.columns.notify')"
          width="140"
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
        <ElTableColumn :label="t('ops.alerts.rules.columns.active')" width="90">
          <template #default="{ row }">
            <ElButton
              v-if="row.active_events"
              link
              type="danger"
              @click="router.push({ path: '/ops/alerts/events' })"
            >
              {{ row.active_events }}
            </ElButton>
            <span v-else>0</span>
          </template>
        </ElTableColumn>
        <ElTableColumn
          v-if="canEdit"
          :label="t('ops.alerts.columns.actions')"
          width="130"
          fixed="right"
        >
          <template #default="{ row }">
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

    <ElDialog
      v-model="dialogVisible"
      :title="
        editingId ? t('ops.alerts.rules.edit') : t('ops.alerts.rules.create')
      "
      width="640px"
    >
      <ElForm
        ref="formRef"
        :model="form"
        :rules="formRules"
        label-width="110px"
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
        <ElFormItem :label="t('ops.alerts.rules.columns.expr')" prop="expr">
          <ElInput
            v-model="form.expr"
            type="textarea"
            :rows="4"
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
            <span>s</span>
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
        <ElFormItem
          :label="t('ops.alerts.rules.form.labels')"
          prop="labelsText"
        >
          <ElInput
            v-model="form.labelsText"
            type="textarea"
            :rows="2"
            placeholder="category=upstream"
          />
        </ElFormItem>
        <ElFormItem :label="t('ops.alerts.rules.form.notifyEnabled')">
          <ElSwitch v-model="form.notifyEnabled" />
          <div class="ops-alerts__hint">
            {{ t("ops.alerts.rules.form.notifyEnabledHint") }}
          </div>
        </ElFormItem>
        <ElFormItem
          :label="t('ops.alerts.rules.form.emails')"
          prop="emailsText"
        >
          <ElInput
            v-model="form.emailsText"
            type="textarea"
            :rows="2"
            placeholder="oncall@example.com"
          />
          <div class="ops-alerts__hint">
            {{ t("ops.alerts.rules.form.emailsHint") }}
          </div>
        </ElFormItem>
        <ElFormItem
          :label="t('ops.alerts.rules.form.webhooks')"
          prop="webhooksText"
        >
          <ElInput
            v-model="form.webhooksText"
            type="textarea"
            :rows="2"
            placeholder="https://hooks.example.com/alert"
          />
          <div class="ops-alerts__hint">
            {{ t("ops.alerts.rules.form.webhooksHint") }}
          </div>
        </ElFormItem>
        <ElFormItem :label="t('ops.alerts.rules.form.evaluateEnabled')">
          <ElSwitch v-model="form.enabled" />
          <div class="ops-alerts__hint">
            {{ t("ops.alerts.rules.form.evaluateEnabledHint") }}
          </div>
        </ElFormItem>
      </ElForm>
      <template #footer>
        <ElButton @click="dialogVisible = false">{{
          t("ops.alerts.rules.cancel")
        }}</ElButton>
        <ElButton type="primary" :loading="saving" @click="save">
          {{ t("ops.alerts.rules.save") }}
        </ElButton>
      </template>
    </ElDialog>
  </OpsPage>
</template>

<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import OpsCard from "@/components/ops/OpsCard.vue";
import OpsPage from "@/components/ops/OpsPage.vue";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";
import {
  ElMessage,
  ElMessageBox,
  type FormInstance,
  type FormRules,
} from "element-plus";
import {
  createAlertRule,
  deleteAlertRule,
  fetchAlertRules,
  setAlertRuleEnabled,
  updateAlertRule,
} from "@/api/alerts";
import type {
  AlertOp,
  AlertRule,
  AlertRuleInput,
  AlertRuleKind,
  AlertSeverity,
} from "@/types/api/alerts";
import { useUserStore } from "@/store/modules/user";

defineOptions({ name: "OpsAlertRules" });

const { t } = useI18n();
const router = useRouter();
const userStore = useUserStore();

const ops: AlertOp[] = [">", ">=", "<", "<=", "==", "!="];
const severities: AlertSeverity[] = ["critical", "warning", "info"];
const olapPlaceholder =
  '{"query":{"template":"top_by_dim","params":{"dim":"product_id","metric":"p99","range":"15m"}},"value_field":"value"}';

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

const roles = computed(() => (userStore.info?.roles as string[]) || []);
const canEdit = computed(() =>
  roles.value.some((role) => role === "R_ADMIN" || role === "R_FINANCE"),
);

function parseLabels(text: string): Record<string, string> | null {
  const labels: Record<string, string> = {};
  for (const line of text.split(/[\n,]/)) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    const idx = trimmed.indexOf("=");
    if (idx <= 0) return null;
    labels[trimmed.slice(0, idx).trim()] = trimmed.slice(idx + 1).trim();
  }
  return labels;
}

const EMAIL_PATTERN = /^[^\s@<>,;"()[\]\\]+@[A-Za-z0-9-]+(\.[A-Za-z0-9-]+)+$/;

function parseList(text: string) {
  return text
    .split(/[\s,;]+/)
    .map((item) => item.trim())
    .filter(Boolean);
}

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
        const emails = parseList(value);
        if (!emails.every((email) => EMAIL_PATTERN.test(email))) {
          callback(new Error(t("ops.alerts.rules.form.emailsInvalid")));
          return;
        }
        if (
          form.notifyEnabled &&
          emails.length === 0 &&
          parseList(form.webhooksText).length === 0
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
        parseLabels(value)
          ? callback()
          : callback(new Error(t("ops.alerts.rules.form.labelsInvalid"))),
      trigger: "blur",
    },
  ],
};

function severityTag(severity: AlertSeverity) {
  if (severity === "critical") return "danger";
  if (severity === "warning") return "warning";
  return "info";
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
    labelsText: Object.entries(rule.labels)
      .map(([k, v]) => `${k}=${v}`)
      .join("\n"),
    emailsText: rule.notify.emails.join("\n"),
    webhooksText: rule.notify.webhooks.join("\n"),
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
    labels: parseLabels(form.labelsText) || {},
    notify: {
      enabled: form.notifyEnabled,
      emails: parseList(form.emailsText),
      webhooks: parseList(form.webhooksText),
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
.ops-alerts__hint {
  margin: 0 0 8px;
  color: var(--el-text-color-secondary);
  font-size: 12px;
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
