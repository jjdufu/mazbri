<template>
  <ControlPanelPageShell
    :title="$t('menus.mazbri.productAuthorization')"
    icon="ri:shield-user-line"
  >
    <div class="product-authorization-page" style="display: flex; flex-direction: column; flex: 1; min-height: 0;">
      <ElTabs
        :model-value="activeTab"
        class="page-header-tabs"
        @tab-change="handleTabChange"
      >
        <ElTabPane
          v-for="item in tabOptions"
          :key="item.value"
          :label="item.label"
          :name="item.value"
        />
      </ElTabs>

      <div v-show="activeTab === 'products'" class="tab-panel">
        <ProductList />
      </div>
      <div v-show="activeTab === 'authorizations'" class="tab-panel">
        <AuthorizationList />
      </div>
    </div>
  </ControlPanelPageShell>
</template>

<script setup lang="ts">
import { $t } from "@/locales";
import { useTabRouteSync } from "@/composables/useTabRouteSync";
import ControlPanelPageShell from "@/components/common/control-panel-page-shell/index.vue";
import AuthorizationList from "@/views/mazbri/operations/authorizations/index.vue";
import ProductList from "@/views/mazbri/products/index.vue";

defineOptions({ name: "ProductGovernance" });

type ProductAuthorizationTab = "products" | "authorizations";

const tabOptions = [
  { label: $t("menus.mazbri.productList"), value: "products" },
  { label: $t("menus.mazbri.authorizationList"), value: "authorizations" },
];

const { activeTab, handleTabChange } = useTabRouteSync<ProductAuthorizationTab>(
  {
    defaultValue: "products",
    allowedValues: ["products", "authorizations"],
  },
);
</script>

<style scoped lang="scss">
.product-authorization-page {
  min-width: 0;
  display: flex;
  flex-direction: column;
  flex: 1;
  height: 100%;

  .page-header-tabs {
    margin-bottom: 12px;
    flex-shrink: 0;

    :deep(.el-tabs__header) {
      margin: 0 !important;
    }

    :deep(.el-tabs__content) {
      display: none !important;
    }
  }

  .tab-panel {
    min-width: 0;
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
  }
}

.product-authorization-page :deep(.product-list-page),
.product-authorization-page :deep(.authorization-list-page) {
  min-width: 0;
}
</style>
