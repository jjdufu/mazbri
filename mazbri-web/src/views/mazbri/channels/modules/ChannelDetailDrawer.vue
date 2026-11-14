<template>
  <AdaptiveDrawer
    v-model="visible"
    size="640px"
    :destroy-on-close="false"
  >
    <template #header>
      <CommonDrawerHeader
        :title="channel?.name || $t('channels.detail.pageTitle')"
        :subtitle="channel?.channel_code || ''"
        align="center"
      >
        <template v-if="canWriteBusiness && channel" #actions>
          <ElButton
            v-roles="ROLES_BIZ"
            type="primary"
            @click="emit('edit', channel)"
          >
            {{ $t("channels.actions.edit") }}
          </ElButton>
        </template>
      </CommonDrawerHeader>
    </template>

    <div v-loading="loading" class="channel-drawer-body">
      <template v-if="channel">
        <section class="drawer-section">
          <div class="drawer-section__title">
            {{ $t("channels.detail.basicInfoTitle") }}
          </div>
          <ElDescriptions :column="2" border>
            <ElDescriptionsItem :label="$t('channels.fields.channelCode')">
              <CopyableText
                :value="channel.channel_code"
                :label="$t('channels.fields.channelCode')"
              />
            </ElDescriptionsItem>
            <ElDescriptionsItem :label="$t('common.fields.status')">
              <StatusIndicator :type="statusIndicatorType(channel.status)">
                {{ $t(`channels.status.${channel.status}`) }}
              </StatusIndicator>
            </ElDescriptionsItem>
            <ElDescriptionsItem :label="$t('channels.fields.name')">
              {{ channel.name || "—" }}
            </ElDescriptionsItem>
            <ElDescriptionsItem :label="$t('channels.fields.pathPrefix')">
              {{ channel.path_prefix || "—" }}
            </ElDescriptionsItem>
            <ElDescriptionsItem :label="$t('channels.fields.costPrice')">
              {{ formatCostPrice(channel.cost_price) }}
            </ElDescriptionsItem>
            <ElDescriptionsItem :label="$t('channels.fields.id')">
              <CopyableText
                :value="channel.id"
                :label="$t('channels.fields.id')"
                compact
              />
            </ElDescriptionsItem>
            <ElDescriptionsItem :label="$t('common.fields.createdAt')">
              {{ formatDateTime(channel.created_at) }}
            </ElDescriptionsItem>
            <ElDescriptionsItem :label="$t('common.fields.updatedAt')">
              {{ formatDateTime(channel.updated_at) }}
            </ElDescriptionsItem>
          </ElDescriptions>
        </section>

        <section class="drawer-section">
          <div class="drawer-section__title">
            {{ $t("channels.detail.supplierSectionTitle") }}
          </div>
          <ElDescriptions :column="2" border>
            <ElDescriptionsItem :label="$t('channels.fields.supplier')">
              <div class="supplier-cell">
                <span>{{ channel.supplier_name || "—" }}</span>
                <ElButton
                  v-if="channel.supplier_id"
                  link
                  type="primary"
                  @click="goSupplierDetail(channel.supplier_id)"
                >
                  {{ $t("channels.detail.viewSupplier") }}
                </ElButton>
              </div>
            </ElDescriptionsItem>
            <ElDescriptionsItem :label="$t('supplier.fields.code')">
              <CopyableText
                :value="channel.supplier_code"
                :label="$t('supplier.fields.code')"
              />
            </ElDescriptionsItem>
          </ElDescriptions>
        </section>

        <section class="drawer-section">
          <div class="drawer-section__title">
            {{ $t("channels.detail.healthSectionTitle") }}
          </div>
          <ElDescriptions :column="2" border>
            <ElDescriptionsItem :label="$t('channels.fields.qpsLimit')">
              <ElTag
                v-if="channel.qps_limit === 0"
                type="info"
                effect="plain"
                size="small"
              >
                {{ $t("channels.fields.qpsUnlimited") }}
              </ElTag>
              <span v-else>
                {{ channel.qps_limit }} {{ $t("channels.fields.qpsUnit") }}
              </span>
            </ElDescriptionsItem>
            <ElDescriptionsItem :label="$t('channels.detail.egressMode')">
              {{
                hasCustomConfig
                  ? $t("channels.detail.egressConfigured")
                  : $t("channels.detail.egressDefault")
              }}
            </ElDescriptionsItem>
          </ElDescriptions>
        </section>

        <section class="drawer-section">
          <div class="drawer-section__title">
            {{ $t("channels.detail.routesSectionTitle") }}
          </div>
          <ElTable
            v-if="boundRoutes.length > 0"
            :data="boundRoutes"
            border
            size="small"
            class="routes-table"
          >
            <ElTableColumn
              :label="$t('channels.detail.routeProduct')"
              min-width="150"
              show-overflow-tooltip
            >
              <template #default="{ row }">
                {{ formatRouteProduct(row) }}
              </template>
            </ElTableColumn>
            <ElTableColumn
              :label="$t('channels.detail.routeTenant')"
              min-width="130"
              show-overflow-tooltip
            >
              <template #default="{ row }">
                <ElTag
                  v-if="!row.tenant_id"
                  type="info"
                  effect="plain"
                  size="small"
                >
                  {{ $t("channels.detail.routeGlobal") }}
                </ElTag>
                <span v-else>
                  {{ formatRouteTenant(row) }}
                </span>
              </template>
            </ElTableColumn>
            <ElTableColumn
              prop="priority"
              :label="$t('channels.detail.routePriority')"
              width="78"
            />
            <ElTableColumn
              :label="$t('channels.detail.routeOrder')"
              width="82"
            >
              <template #default="{ row }">#{{ row.channel_order }}</template>
            </ElTableColumn>
            <ElTableColumn
              :label="$t('channels.detail.routeTimeout')"
              width="126"
            >
              <template #default="{ row }">
                {{ formatSwitchCondition(row.switch_condition) }}
              </template>
            </ElTableColumn>
          </ElTable>
          <ElEmpty
            v-else
            :image-size="60"
            :description="$t('channels.detail.routesEmpty')"
          />
        </section>
      </template>
    </div>
  </AdaptiveDrawer>
</template>

<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { ElMessage } from "element-plus";
import { useRouter } from "vue-router";
import {
  fetchChannelById,
  type Channel,
  type ChannelBoundRoute,
} from "@/api/route-management";
import AdaptiveDrawer from "@/components/common/AdaptiveDrawer.vue";
import CopyableText from "@/components/common/copyable-text/index.vue";
import CommonDrawerHeader from "@/components/common/drawer-header/index.vue";
import StatusIndicator from "@/components/common/StatusIndicator.vue";
import { runWithLoading } from "@/composables/useAsyncState";
import { $t } from "@/locales";
import { useUserStore } from "@/store/modules/user";
import { formatDateTime } from "@/utils/format";
import { ROLES_BIZ } from "@/utils/ops/roles";
import { logger } from "@/utils/sys";
import { formatCostPrice } from "../costPrice";

defineOptions({ name: "ChannelDetailDrawer" });

const props = defineProps<{
  channelId: string;
}>();

const visible = defineModel<boolean>({ required: true });
const emit = defineEmits<{
  edit: [channel: Channel];
}>();

const router = useRouter();
const userStore = useUserStore();
const loading = ref(false);
const channel = ref<Channel | null>(null);

const canWriteBusiness = computed(() => userStore.canWriteBusiness);
const boundRoutes = computed<ChannelBoundRoute[]>(
  () => channel.value?.routes ?? [],
);
const hasCustomConfig = computed(() => {
  const cfg = channel.value?.config;
  return Boolean(cfg && Object.keys(cfg).length > 0);
});

function statusIndicatorType(status: string) {
  if (status === "active") return "success";
  if (status === "suspended") return "warning";
  return "info";
}

function formatRouteProduct(row: ChannelBoundRoute) {
  if (row.product_name && row.product_code) {
    return `${row.product_name} (${row.product_code})`;
  }
  return row.product_code || row.product_name || row.product_uuid;
}

function formatRouteTenant(row: ChannelBoundRoute) {
  if (row.tenant_name && row.tenant_code) {
    return `${row.tenant_name} (${row.tenant_code})`;
  }
  return row.tenant_code || row.tenant_name || row.tenant_id || "—";
}

function formatSwitchCondition(cond: Record<string, unknown> | undefined) {
  const timeout =
    typeof cond?.timeout_ms === "number" ? `${cond.timeout_ms}ms` : "—";
  const retries =
    typeof cond?.max_retries === "number" ? `${cond.max_retries}` : "—";
  return `${timeout} / ${retries}`;
}

function goSupplierDetail(supplierId: string) {
  visible.value = false;
  void router.push({ name: "SupplierDetail", params: { id: supplierId } });
}

async function reload() {
  if (!props.channelId) {
    channel.value = null;
    return;
  }
  try {
    await runWithLoading(loading, async () => {
      channel.value = await fetchChannelById(props.channelId);
    });
  } catch (error) {
    logger.error("Failed to load channel detail:", error);
    ElMessage.error($t("channels.detail.loadFailed"));
  }
}

watch(
  [visible, () => props.channelId],
  ([open, id]) => {
    if (open && id) {
      void reload();
    }
  },
  { immediate: false },
);

defineExpose({ reload });
</script>

<style scoped lang="scss">
.channel-drawer-body {
  display: flex;
  flex-direction: column;
  gap: 20px;
  min-height: 200px;
}

.drawer-section {
  display: flex;
  flex-direction: column;
  gap: 10px;

  &__title {
    font-size: 14px;
    font-weight: 600;
    color: var(--el-text-color-primary);
  }
}

.supplier-cell {
  display: inline-flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 8px;
}

.routes-table {
  width: 100%;
}
</style>
