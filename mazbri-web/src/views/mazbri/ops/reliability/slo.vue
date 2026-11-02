<template>
  <OpsPage :title="t('menus.mazbri.slo')">
    <template #actions>
      <AutoRefresh v-model="refreshInterval" @tick="reload" />
    </template>
    <OpsCard>
      <ArtTable :data="rows" :loading="loading" :columns="columns" stripe />
    </OpsCard>
  </OpsPage>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { ElMessage } from "element-plus";
import AutoRefresh from "@/components/ops/AutoRefresh.vue";
import OpsPage from "@/components/ops/OpsPage.vue";
import OpsCard from "@/components/ops/OpsCard.vue";
import {
  listSloStatus,
  type SloDefinition,
  type SloStatusRow,
} from "@/api/opsReliability";
import type { ColumnOption } from "@/types/component";

defineOptions({ name: "OpsReliabilitySlo" });

const { t } = useI18n();
const refreshInterval = ref(60);
const loading = ref(false);

interface Row {
  name: string;
  sli_type: string;
  objective: number;
  window_days: number;
  sli: number | null;
  remaining: number | null;
  burn_5m: number | null;
  burn_page: number | null;
  burn_ticket: number | null;
  ts: string | null;
}

const rows = ref<Row[]>([]);
const columns = computed<ColumnOption<Row>[]>(() => [
  { prop: "name", label: t("ops.slo.colName"), minWidth: 160 },
  { prop: "sli_type", label: t("ops.slo.colType"), width: 110 },
  { prop: "objective", label: t("ops.slo.colObjective"), width: 100 },
  { prop: "window_days", label: t("ops.slo.colWindow"), width: 90 },
  { prop: "sli", label: t("ops.slo.colSli"), width: 100 },
  { prop: "remaining", label: t("ops.slo.colBudget"), width: 120 },
  { prop: "burn_5m", label: t("ops.slo.colBurn5m"), width: 100 },
  { prop: "burn_page", label: t("ops.slo.colBurnPage"), width: 90 },
  { prop: "burn_ticket", label: t("ops.slo.colBurnTicket"), width: 90 },
  { prop: "ts", label: t("ops.slo.colTs"), width: 180 },
]);

function merge(defs: SloDefinition[], status: SloStatusRow[]): Row[] {
  const byName = new Map(status.map((s) => [s.name, s]));
  return defs.map((d) => {
    const s = byName.get(d.name);
    return {
      name: d.name,
      sli_type: d.sli_type,
      objective: d.objective,
      window_days: d.window_days,
      sli: s?.sli ?? null,
      remaining: s?.error_budget_remaining ?? null,
      burn_5m: s?.burn_5m ?? null,
      burn_page: s?.burn_page ?? null,
      burn_ticket: s?.burn_ticket ?? null,
      ts: s?.ts ?? null,
    };
  });
}

async function reload() {
  loading.value = true;
  try {
    const data = (await listSloStatus()) as {
      definitions?: SloDefinition[];
      status?: SloStatusRow[];
    };
    rows.value = merge(data?.definitions || [], data?.status || []);
  } catch (e) {
    ElMessage.error(String(e));
  } finally {
    loading.value = false;
  }
}
onMounted(reload);
</script>

<style scoped>
</style>
