<template>
  <TenantDetailShell>
    <template #identity>
      <span class="tenant-identity__name" :title="tenant?.name || tenantCode">
        {{ tenant?.name || tenantCode || "—" }}
      </span>
      <span v-if="tenantCode" class="tenant-identity__code">
        <CopyableText
          :value="tenantCode"
          :label="$t('tenant.fields.code')"
        />
      </span>
      <ElTag
        :type="getStatusType(tenantStatusKey)"
        size="small"
        class="tenant-identity__status"
      >
        {{ getStatusLabel(tenantStatusKey) }}
      </ElTag>
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
      v-if="tenantCode && isTabLoaded('account')"
      v-show="activeTab === 'account'"
    >
      <AccountTab :tenant-code="tenantCode" />
    </div>
  </TenantDetailShell>
</template>

<script setup lang="ts">
import { $t } from "@/locales";
import { useTabRouteSync } from "@/composables/useTabRouteSync";
import type { Tenant } from "@/types/api/tenant";
import { fetchTenantById } from "@/api/tenant";
import TenantDetailShell from "@/components/common/tenant-detail-shell/index.vue";
import CopyableText from "@/components/common/copyable-text/index.vue";
import BasicInfoTab from "./tabs/basic-info.vue";
import ApiKeysTab from "./tabs/api-keys.vue";
import WhitelistTab from "./tabs/whitelist.vue";
import AccessStrategyTab from "./tabs/access-strategy.vue";
import ProductsTab from "./tabs/products.vue";
import TransportEncryptionTab from "./tabs/transport-encryption.vue";
import AccountTab from "./tabs/account.vue";
import Overview360Tab from "./tabs/overview360.vue";
import { runWithLoading } from "@/composables/useAsyncState";
import { logger } from "@/utils/sys";
import { getStatusType, getStatusLabel } from "@/utils/display-labels";
import { normalizeTenantCode } from "@/utils/tenant-code";
import { useUserStore } from "@/store/modules/user";
defineOptions({ name: "TenantDetail" });
const route = useRoute();
const loading = ref(false);
const tenant = ref<Tenant | null>(null);
const tenantStatusKey = computed(() =>
  tenant.value?.enabled === false ? "disabled" : "enabled",
);
// 与后端集中式访问控制表（route_access）一致：API 密钥的查看 / 签发 / 吊销
// 仅限超级管理员与管理员；只读用户不展示该标签页，也不允许通过 ?tab=api-keys 进入。
const API_KEY_ROLES: readonly string[] = ["R_ADMIN", "R_FINANCE"];
const userRoles: readonly string[] = useUserStore().getUserInfo.roles ?? [];
const canViewApiKeys = userRoles.some((role) => API_KEY_ROLES.includes(role));
const { activeTab, handleTabChange } = useTabRouteSync({
  defaultValue: "overview360",
  allowedValues: [
    "overview360",
    "basic",
    ...(canViewApiKeys ? ["api-keys"] : []),
    "whitelist",
    "access-strategy",
    "products",
    "transport-encryption",
    "account",
  ],
});
const loadedTabs = ref(new Set(["overview360"]));
const tabOptions = [
  { label: $t("tenant.detail.tabs.overview360"), value: "overview360" },
  { label: $t("tenant.detail.tabs.basic"), value: "basic" },
  ...(canViewApiKeys
    ? [{ label: $t("tenant.detail.tabs.apiKeys"), value: "api-keys" }]
    : []),
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
</script>

<style scoped lang="scss">
.tenant-identity__name {
  font-size: 15px;
  font-weight: 600;
  line-height: 22px;
  color: var(--el-text-color-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 360px;
}

.tenant-identity__code {
  display: inline-flex;
  align-items: center;
  flex-shrink: 0;
  padding: 1px 8px;
  border-radius: var(--art-form-control-radius-sm, 4px);
  background: var(--el-fill-color-light);
  color: var(--el-text-color-regular);
  line-height: 20px;
}

.tenant-identity__status {
  flex-shrink: 0;
}
</style>
