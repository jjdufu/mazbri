<template>
  <TenantDetailShell>
    <template #identity>
      <EntityIdentityBar
        :name="tenant?.name || tenantCode"
        :code="tenantCode"
        :code-label="$t('tenant.fields.code')"
        :status-type="getStatusType(tenantStatusKey)"
        :status-text="getStatusLabel(tenantStatusKey)"
      />
    </template>
    <template #tabs>
      <ElTabs
        :model-value="activeTab"
        class="tenant-detail-tabs"
        @tab-change="handleTabChange"
      >
        <ElTabPane
          v-for="item in tabOptions"
          :key="item.value"
          :label="item.label"
          :name="item.value"
        />
      </ElTabs>
    </template>

    <div v-if="tenantCode && isTabLoaded('overview360')" v-show="activeTab === 'overview360'">
      <Overview360Tab :tenant-code="tenantCode" />
    </div>

    <div v-show="activeTab === 'basic'">
      <BasicInfoTab :tenant="tenant" :loading="loading" @refresh="loadData" />
    </div>

    <div
      v-if="canViewApiKeys && tenantCode && isTabLoaded('api-keys')"
      v-show="activeTab === 'api-keys'"
    >
      <ApiKeysTab :tenant-code="tenantCode" />
    </div>

    <div
      v-if="tenantCode && isTabLoaded('whitelist')"
      v-show="activeTab === 'whitelist'"
    >
      <WhitelistTab :tenant-code="tenantCode" />
    </div>

    <div
      v-if="tenantCode && isTabLoaded('access-strategy')"
      v-show="activeTab === 'access-strategy'"
    >
      <AccessStrategyTab :tenant-code="tenantCode" />
    </div>

    <div
      v-if="tenantCode && isTabLoaded('products')"
      v-show="activeTab === 'products'"
    >
      <ProductsTab :tenant-code="tenantCode" />
    </div>

    <div
      v-if="tenantCode && isTabLoaded('transport-encryption')"
      v-show="activeTab === 'transport-encryption'"
    >
      <TransportEncryptionTab :tenant-code="tenantCode" />
    </div>

    <div
      v-if="canViewApiKeys && tenantCode && isTabLoaded('account')"
      v-show="activeTab === 'account'"
    >
      <AccountTab :tenant-code="tenantCode" />
    </div>
  </TenantDetailShell>
</template>

<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { $t } from "@/locales";
import { useTabRouteSync } from "@/composables/useTabRouteSync";
import { useLatestRequest } from "@/composables/useLatestRequest";
import type { Tenant } from "@/types/api/tenant";
import { fetchTenantById } from "@/api/tenant";
import TenantDetailShell from "@/components/common/tenant-detail-shell/index.vue";
import EntityIdentityBar from "@/components/common/entity-identity-bar/index.vue";
import BasicInfoTab from "./tabs/basic-info.vue";
import ApiKeysTab from "./tabs/api-keys.vue";
import WhitelistTab from "./tabs/whitelist.vue";
import AccessStrategyTab from "./tabs/access-strategy.vue";
import ProductsTab from "./tabs/products.vue";
import TransportEncryptionTab from "./tabs/transport-encryption.vue";
import AccountTab from "./tabs/account.vue";
import Overview360Tab from "./tabs/overview360.vue";
import { logger } from "@/utils/sys";
import { getStatusType, getStatusLabel } from "@/utils/display-labels";
import { normalizeTenantCode } from "@/utils/tenant-code";
import { useUserStore } from "@/store/modules/user";
defineOptions({ name: "TenantDetail" });
const route = useRoute();
const { locale } = useI18n();
const userStore = useUserStore();
const tenant = ref<Tenant | null>(null);
const tenantStatusKey = computed(() =>
  tenant.value?.enabled === false ? "disabled" : "enabled",
);
// 与后端集中式访问控制表（route_access）一致：API 密钥与账户流水仅限超级管理员与管理员；
// 只读用户（R_VIEWER）不展示 api-keys 与 account 页签，也不允许通过 ?tab=... 进入。
const API_KEY_ROLES: readonly string[] = ["R_ADMIN", "R_FINANCE"];
const canViewApiKeys = computed(() =>
  (userStore.getUserInfo.roles ?? []).some((role) =>
    API_KEY_ROLES.includes(role),
  ),
);
const allowedTabValues = computed(() => [
  "overview360",
  "basic",
  ...(canViewApiKeys.value ? ["api-keys"] : []),
  "whitelist",
  "access-strategy",
  "products",
  "transport-encryption",
  ...(canViewApiKeys.value ? ["account"] : []),
]);
const { activeTab, handleTabChange } = useTabRouteSync({
  defaultValue: "overview360",
  allowedValues: allowedTabValues,
});
const loadedTabs = ref(new Set(["overview360"]));
const tabOptions = computed(() => {
  void locale.value;
  return [
    { label: $t("tenant.detail.tabs.overview360"), value: "overview360" },
    { label: $t("tenant.detail.tabs.basic"), value: "basic" },
    ...(canViewApiKeys.value
      ? [{ label: $t("tenant.detail.tabs.apiKeys"), value: "api-keys" }]
      : []),
    { label: $t("tenant.detail.tabs.whitelist"), value: "whitelist" },
    { label: $t("tenant.detail.tabs.accessStrategy"), value: "access-strategy" },
    { label: $t("tenant.detail.tabs.products"), value: "products" },
    {
      label: $t("tenant.detail.tabs.transportEncryption"),
      value: "transport-encryption",
    },
    ...(canViewApiKeys.value
      ? [{ label: $t("tenant.detail.tabs.account"), value: "account" }]
      : []),
  ];
});

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

const { loading, run: runLoadTenant, cancel: cancelLoadTenant } =
  useLatestRequest(async (ctx, code: string) => {
    const res = await fetchTenantById(code, ctx.signal);
    if (!ctx.isLatest()) return;
    tenant.value = res;
  });

const loadData = async () => {
  const code = tenantCode.value;
  if (!code) {
    cancelLoadTenant();
    tenant.value = null;
    logger.warn("Tenant detail missing route param code; skip load");
    return;
  }
  try {
    await runLoadTenant(code);
  } catch (error) {
    logger.error("Failed to load tenant:", error);
  }
};
watch(tenantCode, (code, prev) => {
  if (code && code !== prev) {
    loadedTabs.value = new Set([activeTab.value]);
    void loadData();
  }
});
onMounted(() => {
  void loadData();
});
</script>
