<template>
  <div class="ops-billing-panel">
    <OpsPanelToolbar>
      <ElSelect v-model="days" style="width: 120px" @change="loadCalendar">
        <ElOption :value="7" label="7d" />
        <ElOption :value="30" label="30d" />
        <ElOption :value="90" label="90d" />
      </ElSelect>
      <AutoRefresh v-model="refreshInterval" @tick="loadCalendar" />
    </OpsPanelToolbar>

    <OpsCard :title="t('ops.reconcile.calendar')">
      <div class="calendar-grid">
        <div
          v-for="d in calendar"
          :key="d.day"
          class="calendar-cell"
          :class="`is-${d.status}`"
          :title="`${d.day}: open=${d.open_count} resolved=${d.resolved_count} error=${d.error_count}`"
        >
          <div class="day">{{ d.day.slice(5) }}</div>
          <div class="status">{{ d.status }}</div>
        </div>
      </div>
    </OpsCard>

    <UsageReconciliationPanel />
  </div>
</template>

<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { ElMessage } from "element-plus";
import AutoRefresh from "@/components/ops/AutoRefresh.vue";
import OpsPanelToolbar from "@/components/ops/OpsPanelToolbar.vue";
import OpsCard from "@/components/ops/OpsCard.vue";
import {
  fetchReconcileCalendar,
  type ReconcileCalendarDay,
} from "@/api/billing-ops";
import UsageReconciliationPanel from "@/views/mazbri/billing/usage-reconciliation/index.vue";

defineOptions({ name: "OpsBillingReconcilePanel" });

const { t } = useI18n();
const days = ref(30);
const refreshInterval = ref(120);
const calendar = ref<ReconcileCalendarDay[]>([]);

async function loadCalendar() {
  try {
    calendar.value = (await fetchReconcileCalendar(days.value)) ?? [];
  } catch {
    ElMessage.error(t("ops.reconcile.loadFailed"));
  }
}

onMounted(loadCalendar);
</script>

<style scoped>
.ops-billing-panel {
  display: flex;
  flex-direction: column;
  gap: var(--ops-section-gap, 16px);
  min-width: 0;
}
.calendar-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(72px, 1fr));
  gap: 8px;
}
.calendar-cell {
  padding: 8px;
  border-radius: 6px;
  border: 1px solid var(--el-border-color-lighter);
  background: var(--el-fill-color-light);
  text-align: center;
  font-size: 12px;
}
.calendar-cell.is-ok {
  background: var(--el-color-success-light-9);
  border-color: var(--el-color-success-light-7);
}
.calendar-cell.is-open {
  background: var(--el-color-warning-light-9);
  border-color: var(--el-color-warning-light-7);
}
.calendar-cell.is-error {
  background: var(--el-color-danger-light-9);
  border-color: var(--el-color-danger-light-7);
}
.calendar-cell .status {
  margin-top: 4px;
  color: var(--el-text-color-secondary);
}
</style>
