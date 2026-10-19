<template>
  <ControlPanelPageShell
    :title="$t('menus.mazbri.productAuthorization')"
    icon="ri:shield-user-line"
  >
    <div class="product-authorization-page" style="display: flex; flex-direction: column; flex: 1; min-height: 0;">
      <div class="tab-segmented">
        <ElSegmented
          v-model="activeTab"
          :options="tabOptions"
          @change="handleTabChange"
        />
      </div>

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

  .tab-segmented {
    margin-bottom: 16px;
    flex-shrink: 0;

    .el-segmented {
      height: 40px;
      padding: 4px;
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
