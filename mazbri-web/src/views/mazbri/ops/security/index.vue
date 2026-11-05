<template>
  <OpsPage :title="t('menus.mazbri.securityEvents')">
    <template #actions>
      <TimeRangePicker v-model="timeRange" @change="reload" />
        <AutoRefresh v-model="refreshInterval" @tick="reload" />
    </template>

    <ElRow :gutter="16" class="ops-equal-row">
      <ElCol :xs="24" :lg="8">
        <OpsCard :title="t('ops.security.topIp')">
          <ArtTable :data="topIp" :loading="loading" :columns="topColumns" stripe />
        </OpsCard>
      </ElCol>
      <ElCol :xs="24" :lg="8">
        <OpsCard :title="t('ops.security.topKey')">
          <ArtTable :data="topKey" :loading="loading" :columns="topColumns" stripe />
        </OpsCard>
      </ElCol>
      <ElCol :xs="24" :lg="8">
        <OpsCard :title="t('ops.security.topCountry')">
          <ArtTable :data="topCountry" :loading="loading" :columns="topColumns" stripe />
        </OpsCard>
      </ElCol>
    </ElRow>

    <OpsCard :title="t('ops.security.events')">
      <ArtTable :data="events" :loading="loading" :columns="eventColumns" stripe />
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
import { toOlapWindow } from "@/composables/ops/useOlapRange";
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
  preset: "6h",
  start: Math.floor(Date.now() / 1000) - 6 * 3600,
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
    const window = toOlapWindow(timeRange.value);
    const [ip, key, country, ev] = await Promise.all([
      postOlapQuery({ template: "security_top", params: { ...window, dim: "ip", limit: 10 } }),
      postOlapQuery({ template: "security_top", params: { ...window, dim: "key", limit: 10 } }),
      postOlapQuery({ template: "security_top", params: { ...window, dim: "country", limit: 10 } }),
      postOlapQuery({ template: "security_events", params: { ...window, limit: 50 } }),
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
</style>
