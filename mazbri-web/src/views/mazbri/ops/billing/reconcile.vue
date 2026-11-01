<template>
  <div class="ops-reconcile art-full-height">
    <div class="ops-traffic__toolbar">
      <h2>{{ t("menus.mazbri.billingReconcile") }}</h2>
      <div class="ops-traffic__actions">
        <ElSelect v-model="days" style="width: 120px" @change="loadCalendar">
          <ElOption :value="7" label="7d" />
          <ElOption :value="30" label="30d" />
          <ElOption :value="90" label="90d" />
        </ElSelect>
        <AutoRefresh v-model="refreshInterval" @tick="loadCalendar" />
      </div>
    </div>

    <div class="ops-traffic__card">
      <h3>{{ t("ops.reconcile.calendar") }}</h3>
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
    </div>

    <UsageReconciliationPanel />
  </div>
</template>

<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { ElMessage } from "element-plus";
import AutoRefresh from "@/components/ops/AutoRefresh.vue";
import {
  fetchReconcileCalendar,
  type ReconcileCalendarDay,
} from "@/api/billing-ops";
import UsageReconciliationPanel from "@/views/mazbri/billing/usage-reconciliation/index.vue";

defineOptions({ name: "OpsBillingReconcile" });

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
.ops-traffic__toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 12px;
}
.ops-traffic__actions {
  display: flex;
  gap: 8px;
  align-items: center;
}
.ops-traffic__card {
  margin-bottom: 12px;
  padding: 12px;
  border-radius: 8px;
  background: var(--el-bg-color);
}
.calendar-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(72px, 1fr));
  gap: 8px;
}
.calendar-cell {
  padding: 8px;
  border-radius: 6px;
  background: var(--el-fill-color-light);
  text-align: center;
  font-size: 12px;
}
.calendar-cell.is-ok {
  background: #e8f8ef;
}
.calendar-cell.is-open {
  background: #fff7e6;
}
.calendar-cell.is-error {
  background: #fdecea;
}
.calendar-cell .status {
  margin-top: 4px;
  color: var(--el-text-color-secondary);
}
</style>
