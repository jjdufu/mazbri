<template>
  <div class="tenant-detail-page">
    <ElPageHeader @back="router.back()">
      <template #content>
        <span class="text-lg font-medium">{{
          tenant?.name || $t("tenant.detail.pageTitle")
        }}</span>
      </template>
      <template #extra>
        <ElTag :type="getStatusType(tenantStatusKey)">
          {{ getStatusLabel(tenantStatusKey) }}
        </ElTag>
      </template>
    </ElPageHeader>

    <div class="tab-segmented mt-5">
      <ElSegmented
        :model-value="activeTab"
        :options="tabOptions"
        @change="handleTabChange"
      />
    </div>

    <div v-show="activeTab === 'basic'" class="mt-5">
      <BasicInfoTab :tenant="tenant" :loading="loading" @refresh="loadData" />
    </div>

    <div
      v-if="tenantCode && isTabLoaded('api-keys')"
      v-show="activeTab === 'api-keys'"
      class="mt-5"
    >
      <ApiKeysTab :tenant-code="tenantCode" />
    </div>

    <div
      v-if="tenantCode && isTabLoaded('whitelist')"
      v-show="activeTab === 'whitelist'"
      class="mt-5"
    >
      <WhitelistTab :tenant-code="tenantCode" />
    </div>

    <div
      v-if="tenantCode && isTabLoaded('access-strategy')"
      v-show="activeTab === 'access-strategy'"
      class="mt-5"
    >
      <AccessStrategyTab :tenant-code="tenantCode" />
    </div>

    <div
      v-if="tenantCode && isTabLoaded('products')"
      v-show="activeTab === 'products'"
      class="mt-5"
    >
      <ProductsTab :tenant-code="tenantCode" />
    </div>

    <div
      v-if="tenantCode && isTabLoaded('transport-encryption')"
      v-show="activeTab === 'transport-encryption'"
      class="mt-5"
    >
      <TransportEncryptionTab :tenant-code="tenantCode" />
    </div>

    <div
      v-if="tenantCode && isTabLoaded('account')"
      v-show="activeTab === 'account'"
      class="mt-5"
    >
      <AccountTab :tenant-code="tenantCode" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { $t } from "@/locales";
import { useTabRouteSync } from "@/composables/useTabRouteSync";
import type { Tenant } from "@/types/api/tenant";
import { fetchTenantById } from "@/api/tenant";
import BasicInfoTab from "./tabs/basic-info.vue";
import ApiKeysTab from "./tabs/api-keys.vue";
import WhitelistTab from "./tabs/whitelist.vue";
import AccessStrategyTab from "./tabs/access-strategy.vue";
import ProductsTab from "./tabs/products.vue";
import TransportEncryptionTab from "./tabs/transport-encryption.vue";
import AccountTab from "./tabs/account.vue";
import { runWithLoading } from "@/composables/useAsyncState";
import { logger } from "@/utils/sys";
import { getStatusType, getStatusLabel } from "@/utils/display-labels";
import { normalizeTenantCode } from "@/utils/tenant-code";
defineOptions({ name: "TenantDetail" });
const router = useRouter();
const route = useRoute();
const loading = ref(false);
const tenant = ref<Tenant | null>(null);
const tenantStatusKey = computed(() =>
  tenant.value?.enabled === false ? "disabled" : "enabled",
);
const { activeTab, handleTabChange } = useTabRouteSync({
  defaultValue: "basic",
  allowedValues: [
    "basic",
    "api-keys",
    "whitelist",
    "access-strategy",
    "products",
    "transport-encryption",
    "account",
  ],
});
const loadedTabs = ref(new Set(["basic"]));
const tabOptions = [
  { label: $t("tenant.detail.tabs.basic"), value: "basic" },
  { label: $t("tenant.detail.tabs.apiKeys"), value: "api-keys" },
  { label: $t("tenant.detail.tabs.whitelist"), value: "whitelist" },
  { label: $t("tenant.detail.tabs.accessStrategy"), value: "access-strategy" },
  { label: $t("tenant.detail.tabs.products"), value: "products" },
  {
    label: $t("tenant.detail.tabs.transportEncryption"),
    value: "transport-encryption",
  },
  { label: $t("tenant.detail.tabs.account"), value: "account" },
];

const tenantCode = computed(
  () => normalizeTenantCode(route.params.code) ?? "",
);
watch(
  activeTab,
  (tab) => {
    loadedTabs.value.add(tab);
  },
  { immediate: true },
);
const isTabLoaded = (tab: string) => loadedTabs.value.has(tab);
const loadData = async () => {
  const code = tenantCode.value;
  if (!code) {
    tenant.value = null;
    logger.warn("Tenant detail missing route param code; skip load");
    return;
  }
  try {
    await runWithLoading(loading, async () => {
      tenant.value = await fetchTenantById(code);
    });
  } catch (error) {
    logger.error("Failed to load tenant:", error);
  }
};
watch(tenantCode, (code, prev) => {
  if (code && code !== prev) {
    loadData();
  }
});
onMounted(() => {
  loadData();
});
onActivated(() => {
  loadData();
});
</script>

<style scoped lang="scss">
.tenant-detail-page {
  .tab-segmented {
    margin-bottom: 16px;

    .el-segmented {
      height: 40px;
      padding: 4px;
    }
  }
}
</style>
