<template>
  <div class="ops-traffic art-full-height">
    <div class="ops-traffic__toolbar">
      <h2>{{ t("menus.mazbri.securityEvents") }}</h2>
      <div class="ops-traffic__actions">
        <TimeRangePicker v-model="timeRange" @change="reload" />
        <AutoRefresh v-model="refreshInterval" @tick="reload" />
      </div>
    </div>

    <ElRow :gutter="12">
      <ElCol :xs="24" :lg="8">
        <div class="ops-traffic__card">
          <h3>{{ t("ops.security.topIp") }}</h3>
          <ArtTable :data="topIp" :loading="loading" :columns="topColumns" stripe />
        </div>
      </ElCol>
      <ElCol :xs="24" :lg="8">
        <div class="ops-traffic__card">
          <h3>{{ t("ops.security.topKey") }}</h3>
          <ArtTable :data="topKey" :loading="loading" :columns="topColumns" stripe />
        </div>
      </ElCol>
      <ElCol :xs="24" :lg="8">
        <div class="ops-traffic__card">
          <h3>{{ t("ops.security.topCountry") }}</h3>
          <ArtTable :data="topCountry" :loading="loading" :columns="topColumns" stripe />
        </div>
      </ElCol>
    </ElRow>

    <div class="ops-traffic__card">
      <h3>{{ t("ops.security.events") }}</h3>
      <ArtTable :data="events" :loading="loading" :columns="eventColumns" stripe />
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { ElMessage } from "element-plus";
import TimeRangePicker, {
  type TimeRangeValue,
} from "@/components/ops/TimeRangePicker.vue";
import AutoRefresh from "@/components/ops/AutoRefresh.vue";
import { toOlapRange } from "@/composables/ops/useOlapRange";
import { postOlapQuery } from "@/api/olap";
import type { ColumnOption } from "@/types/component";

defineOptions({ name: "OpsSecurityEvents" });

interface TopRow {
  dim_value: string;
  fail_count: number;
  req_count: number;
}
interface EventRow {
  ts: string;
  event_type: string;
  tenant: string;
  key_id: string;
  ip: string;
  detail: string;
  geo_country: string;
  geo_city: string;
  asn: string;
}

const { t } = useI18n();
const timeRange = ref<TimeRangeValue>({
  preset: "24h",
  start: Math.floor(Date.now() / 1000) - 86400,
  end: Math.floor(Date.now() / 1000),
});
const refreshInterval = ref(60);
const loading = ref(false);
const topIp = ref<TopRow[]>([]);
const topKey = ref<TopRow[]>([]);
const topCountry = ref<TopRow[]>([]);
const events = ref<EventRow[]>([]);

const topColumns = computed<ColumnOption[]>(() => [
  { prop: "dim_value", label: t("ops.security.colDim") },
  { prop: "fail_count", label: t("ops.security.colFails"), width: 100 },
  { prop: "req_count", label: t("ops.security.colReqs"), width: 100 },
]);
const eventColumns = computed<ColumnOption[]>(() => [
  { prop: "ts", label: t("ops.security.colTime"), width: 180 },
  { prop: "event_type", label: t("ops.security.colType"), width: 120 },
  { prop: "tenant", label: t("ops.security.colTenant"), width: 120 },
  { prop: "key_id", label: t("ops.security.colKey"), width: 140 },
  { prop: "ip", label: t("ops.security.colIp"), width: 140 },
  { prop: "geo_country", label: t("ops.security.colCountry"), width: 90 },
  { prop: "geo_city", label: t("ops.security.colCity"), width: 100 },
  { prop: "asn", label: "ASN", width: 80 },
  { prop: "detail", label: t("ops.security.colDetail") },
]);

async function reload() {
  loading.value = true;
  try {
    const range = toOlapRange(timeRange.value);
    const [ip, key, country, ev] = await Promise.all([
      postOlapQuery({ template: "security_top", params: { range, dim: "ip", limit: 10 } }),
      postOlapQuery({ template: "security_top", params: { range, dim: "key", limit: 10 } }),
      postOlapQuery({ template: "security_top", params: { range, dim: "country", limit: 10 } }),
      postOlapQuery({ template: "security_events", params: { range, limit: 50 } }),
    ]);
    topIp.value = ((ip as unknown as { rows?: TopRow[] }).rows) || [];
    topKey.value = ((key as unknown as { rows?: TopRow[] }).rows) || [];
    topCountry.value = ((country as unknown as { rows?: TopRow[] }).rows) || [];
    events.value = ((ev as unknown as { rows?: EventRow[] }).rows) || [];
  } catch (e) {
    ElMessage.error(String(e));
  } finally {
    loading.value = false;
  }
}

onMounted(reload);
</script>

<style scoped>
.ops-traffic__toolbar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 12px;
}
.ops-traffic__actions {
  display: flex;
  gap: 8px;
  align-items: center;
}
.ops-traffic__card {
  background: var(--el-bg-color);
  border-radius: 8px;
  padding: 12px;
  margin-bottom: 12px;
}
</style>
