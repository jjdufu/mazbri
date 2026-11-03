<template>
  <OpsCard v-if="showPanel" class="slo-budget-panel">
    <template #title>
      {{ t("ops.overview.sloBudgetTitle") }}
    </template>

    <!-- 观察者 / 管理员：仅极简剩余预算，无 burn 详表 -->
    <div v-if="!isAdmin" class="slo-budget-panel__slim">
      <ElTag type="success">
        {{
          t("ops.overview.sloBudgetSlim", {
            name: slimName,
            remaining: slimRemainingLabel,
          })
        }}
      </ElTag>
    </div>

    <!-- 超管：详表（含 burn / Top 违规优先排序） -->
    <template v-else>
      <OpsTableWrap>
        <ArtTable
          :data="detailRows"
          :loading="loading"
          :columns="columns"
          stripe
        />
      </OpsTableWrap>
    </template>
  </OpsCard>
</template>

<script setup lang="ts">
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import { ElMessage } from "element-plus";
import OpsCard from "@/components/ops/OpsCard.vue";
import OpsTableWrap from "@/components/ops/OpsTableWrap.vue";
import { useOpsRole } from "@/composables/ops/useOpsRole";
import { listSloStatus } from "@/api/opsReliability";
import {
  buildSloTableColumns,
  formatBudgetRemaining,
  hasSlimErrorBudget,
  hasSloBudgetData,
  mergeSloBudgetRows,
  takeSloTopViolators,
  type SloBudgetRow,
} from "@/utils/ops/slo";

defineOptions({ name: "OpsOverviewSloBudgetPanel" });

const props = defineProps<{
  /** overview.error_budget_connected —— 非超管极简块门控 */
  connected?: boolean;
  remaining?: number | null;
  sloName?: string | null;
}>();

const DETAIL_LIMIT = 8;

const { t } = useI18n();
const { isAdmin } = useOpsRole();
const loading = ref(false);
const adminRows = ref<SloBudgetRow[]>([]);

const slimVisible = computed(() =>
  hasSlimErrorBudget(props.connected, props.remaining),
);

const adminVisible = computed(() => hasSloBudgetData(adminRows.value));

const showPanel = computed(() =>
  isAdmin.value ? adminVisible.value : slimVisible.value,
);

const slimName = computed(
  () => props.sloName?.trim() || t("ops.overview.sloBudgetDefaultName"),
);

const slimRemainingLabel = computed(() =>
  formatBudgetRemaining(props.remaining),
);

const detailRows = computed(() =>
  takeSloTopViolators(adminRows.value, DETAIL_LIMIT),
);

const columns = computed(() =>
  buildSloTableColumns(t, { adminDetail: true }),
);

async function reload() {
  if (!isAdmin.value) {
    adminRows.value = [];
    return;
  }
  loading.value = true;
  try {
    const data = await listSloStatus();
    adminRows.value = mergeSloBudgetRows(data?.definitions, data?.status);
  } catch (error) {
    adminRows.value = [];
    ElMessage.error(
      error instanceof Error
        ? error.message
        : t("ops.overview.sloBudgetLoadFailed"),
    );
  } finally {
    loading.value = false;
  }
}

defineExpose({ reload });
</script>

<style scoped>
.slo-budget-panel__slim {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}
</style>
