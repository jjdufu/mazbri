<template>
  <div class="ops-traffic art-full-height">
    <div class="ops-traffic__toolbar">
      <h2>{{ t("menus.mazbri.customDashboard") }}</h2>
      <div class="ops-traffic__actions">
        <ElButton type="primary" @click="save">{{ t("ops.dashboards.save") }}</ElButton>
      </div>
    </div>
    <div class="ops-traffic__card">
      <ElForm label-width="100px">
        <ElFormItem :label="t('ops.dashboards.home')">
          <ElInput v-model="homePath" placeholder="/dashboard" />
        </ElFormItem>
        <ElFormItem :label="t('ops.dashboards.json')">
          <ElInput v-model="dashboardsJson" type="textarea" :rows="14" />
        </ElFormItem>
      </ElForm>
      <p class="hint">{{ t("ops.dashboards.hint") }}</p>
    </div>
  </div>
</template>

<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { ElMessage } from "element-plus";
import { getPreferences, putPreferences } from "@/api/preferences";

defineOptions({ name: "OpsCustomDashboard" });

const { t } = useI18n();
const homePath = ref("");
const dashboardsJson = ref("[]");

const ALLOWED = new Set([
  "product_rank", "security_top", "security_events", "quota_top",
  "cost_trend", "latency_quantiles", "error_matrix", "top_by_dim",
]);

async function load() {
  try {
    const pref = (await getPreferences()) as {
      home_path?: string;
      dashboards?: unknown;
    };
    homePath.value = pref.home_path || "";
    dashboardsJson.value = JSON.stringify(pref.dashboards ?? [], null, 2);
  } catch (e) {
    ElMessage.error(String(e));
  }
}

async function save() {
  try {
    const parsed = JSON.parse(dashboardsJson.value || "[]");
    if (!Array.isArray(parsed)) throw new Error("dashboards must be array");
    for (const d of parsed) {
      for (const w of d.widgets || []) {
        if (!ALLOWED.has(w.template)) {
          throw new Error(`template not allowed: ${w.template}`);
        }
      }
    }
    await putPreferences({ home_path: homePath.value, dashboards: parsed });
    ElMessage.success(t("ops.dashboards.saved"));
  } catch (e) {
    ElMessage.error(String(e));
  }
}

onMounted(load);
</script>

<style scoped>
.ops-traffic__toolbar { display:flex; justify-content:space-between; align-items:center; margin-bottom:12px; }
.ops-traffic__card { background:var(--el-bg-color); border-radius:8px; padding:12px; }
.hint { color: var(--el-text-color-secondary); font-size: 12px; }
</style>
