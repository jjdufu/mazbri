<template>
  <OpsPage :title="t('menus.mazbri.upstreamSla')">
    <template #actions>
      <TimeRangePicker v-model="timeRange" @change="reload" />
        <AutoRefresh v-model="refreshInterval" @tick="reload" />
        <ElDropdown @command="exportCsv">
          <ElButton :loading="exporting">{{ t("ops.upstream.exportCsv") }}</ElButton>
          <template #dropdown>
            <ElDropdownMenu>
              <ElDropdownItem command="24h">{{ t("ops.upstream.report.day") }}</ElDropdownItem>
              <ElDropdownItem command="7d">{{ t("ops.upstream.report.week") }}</ElDropdownItem>
              <ElDropdownItem command="30d">{{ t("ops.upstream.report.month") }}</ElDropdownItem>
            </ElDropdownMenu>
          </template>
        </ElDropdown>
    </template>

    <OpsCard :title="t('ops.upstream.slaTable')">
      <p class="ops-sla__hint">{{ t("ops.upstream.thresholdHint") }}</p>
      <ArtTable
        :data="tableRows"
        :loading="loading"
        :columns="columns"
        class="surface-panel-table"
        stripe
      >
        <template #availability="{ row }">
          <span :class="['ops-sla__dot', `ops-sla__dot--${row.level}`]" />
          {{ formatPct(row.availability) }}
        </template>
      </ArtTable>
    </OpsCard>
  </OpsPage>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { ElMessage } from "element-plus";
import TimeRangePicker, {
  type TimeRangeValue,
} from "@/components/ops/TimeRangePicker.vue";
import AutoRefresh from "@/components/ops/AutoRefresh.vue";
import OpsPage from "@/components/ops/OpsPage.vue";
import OpsCard from "@/components/ops/OpsCard.vue";
import { toOlapRange } from "@/composables/ops/useOlapRange";
import { postOlapQuery } from "@/api/olap";
import { fetchSupplierCosts } from "@/api/billing-ops";
import { useUserStore } from "@/store/modules/user";
import type { OlapRange, SlaRow } from "@/types/api/olap";
import type { SupplierCostRow } from "@/types/api/billing-ops";
import type { ColumnOption } from "@/types/component";

defineOptions({ name: "OpsUpstreamSla" });

type Level = "green" | "yellow" | "red";

interface SlaTableRow {
  supplier_code: string;
  channel_code: string;
  calls: number;
  availability: number;
  level: Level;
  upstream_p95_ms: number;
  timeout_rate: number;
  retry_rate: number;
  fallback_rate: number;
  cost: string;
}

/** 可用率默认阈值：≥99.5% 绿，≥99% 黄，其余红。 */
const GREEN_AT = 0.995;
const YELLOW_AT = 0.99;

const { t } = useI18n();
const userStore = useUserStore();
const canSeeCost = computed(() => userStore.canWriteBusiness);

const timeRange = ref<TimeRangeValue>({
  preset: "24h",
  start: Math.floor(Date.now() / 1000) - 86400,
  end: Math.floor(Date.now() / 1000),
});
const refreshInterval = ref(60);
const loading = ref(false);
const exporting = ref(false);
const tableRows = ref<SlaTableRow[]>([]);

function levelOf(availability: number): Level {
  if (availability >= GREEN_AT) return "green";
  if (availability >= YELLOW_AT) return "yellow";
  return "red";
}

function ratio(part: number, whole: number) {
  return whole > 0 ? part / whole : 0;
}

function formatPct(value: number) {
  return `${(value * 100).toFixed(2)}%`;
}

function costKey(supplier: string, channel: string) {
  return `${supplier}\u0000${channel}`;
}

function toTableRows(rows: SlaRow[], costs: SupplierCostRow[]): SlaTableRow[] {
  const costMap = new Map(
    costs.map((c) => [costKey(c.supplier_code, c.channel_code), c.settlement_amount]),
  );
  return rows.map((r) => {
    const availability = 1 - ratio(r.failures, r.calls);
    return {
      supplier_code: r.supplier_code,
      channel_code: r.channel_code,
      calls: r.calls,
      availability,
      level: levelOf(availability),
      upstream_p95_ms: r.upstream_p95_ms,
      timeout_rate: ratio(r.timeouts, r.calls),
      retry_rate: ratio(r.retried, r.calls),
      fallback_rate: ratio(r.fallbacks, r.calls),
      cost: costMap.get(costKey(r.supplier_code, r.channel_code)) ?? "0",
    };
  });
}

async function load(range: OlapRange) {
  const [sla, costs] = await Promise.all([
    postOlapQuery<SlaRow>({ template: "supplier_sla", params: { range } }),
    canSeeCost.value ? fetchSupplierCosts(range) : Promise.resolve([]),
  ]);
  return toTableRows(sla.rows ?? [], costs ?? []);
}

const columns = computed<ColumnOption<SlaTableRow>[]>(() => {
  const cols: ColumnOption<SlaTableRow>[] = [
    { prop: "supplier_code", label: t("ops.upstream.columns.supplier") },
    { prop: "channel_code", label: t("ops.upstream.columns.channel") },
    {
      prop: "availability",
      label: t("ops.upstream.columns.availability"),
      useSlot: true,
    },
    {
      prop: "upstream_p95_ms",
      label: "P95 (ms)",
      formatter: (row: SlaTableRow) => row.upstream_p95_ms.toFixed(1),
    },
    {
      prop: "timeout_rate",
      label: t("ops.upstream.columns.timeoutRate"),
      formatter: (row: SlaTableRow) => formatPct(row.timeout_rate),
    },
    {
      prop: "retry_rate",
      label: t("ops.upstream.columns.retryRate"),
      formatter: (row: SlaTableRow) => formatPct(row.retry_rate),
    },
    {
      prop: "fallback_rate",
      label: t("ops.upstream.columns.fallbackRate"),
      formatter: (row: SlaTableRow) => formatPct(row.fallback_rate),
    },
    { prop: "calls", label: t("ops.upstream.columns.calls") },
  ];
  if (canSeeCost.value) {
    cols.push({ prop: "cost", label: t("ops.upstream.columns.cost") });
  }
  return cols;
});

function csvCell(value: string | number) {
  const text = String(value);
  return /[",\n]/.test(text) ? `"${text.replace(/"/g, '""')}"` : text;
}

async function exportCsv(range: OlapRange) {
  exporting.value = true;
  try {
    const rows = await load(range);
    const header = [
      "supplier_code",
      "channel_code",
      "availability",
      "level",
      "upstream_p95_ms",
      "timeout_rate",
      "retry_rate",
      "fallback_rate",
      "calls",
      ...(canSeeCost.value ? ["cost"] : []),
    ];
    const lines = rows.map((r) =>
      [
        r.supplier_code,
        r.channel_code,
        r.availability.toFixed(6),
        r.level,
        r.upstream_p95_ms.toFixed(1),
        r.timeout_rate.toFixed(6),
        r.retry_rate.toFixed(6),
        r.fallback_rate.toFixed(6),
        r.calls,
        ...(canSeeCost.value ? [r.cost] : []),
      ]
        .map(csvCell)
        .join(","),
    );
    const blob = new Blob([`\uFEFF${[header.join(","), ...lines].join("\n")}\n`], {
      type: "text/csv;charset=utf-8",
    });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = `supplier-sla-${range}-${new Date().toISOString().slice(0, 10)}.csv`;
    link.click();
    URL.revokeObjectURL(url);
  } catch {
    ElMessage.error(t("ops.upstream.loadFailed"));
  } finally {
    exporting.value = false;
  }
}

async function reload() {
  loading.value = true;
  try {
    tableRows.value = await load(toOlapRange(timeRange.value));
  } catch {
    ElMessage.error(t("ops.upstream.loadFailed"));
  } finally {
    loading.value = false;
  }
}

onMounted(reload);
</script>

<style scoped>

.ops-sla__hint {
  margin: 0 0 8px;
  color: var(--el-text-color-secondary);
  font-size: 12px;
}
.ops-sla__dot {
  display: inline-block;
  width: 8px;
  height: 8px;
  margin-right: 6px;
  border-radius: 50%;
}
.ops-sla__dot--green {
  background: var(--el-color-success);
}
.ops-sla__dot--yellow {
  background: var(--el-color-warning);
}
.ops-sla__dot--red {
  background: var(--el-color-danger);
}
</style>
