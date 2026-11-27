<template>
  <ControlPanelPageShell
    :title="t('menus.mazbri.productAuthorization')"
    icon="ri:shield-user-line"
  >
    <div
      class="product-authorization-page"
      style="display: flex; flex-direction: column; flex: 1; min-height: 0"
    >
      <div class="page-header-tabs-bar">
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
        <div ref="headerActionsEl" class="page-header-tabs-actions" />
      </div>

      <div
        v-if="isTabLoaded('products')"
        v-show="activeTab === 'products'"
        class="tab-panel"
      >
        <ProductList />
      </div>
      <div
        v-if="isTabLoaded('authorizations')"
        v-show="activeTab === 'authorizations'"
        class="tab-panel"
      >
        <AuthorizationList />
      </div>
    </div>
  </ControlPanelPageShell>
</template>

<script setup lang="ts">
import { computed, provide, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useTabRouteSync } from "@/composables/useTabRouteSync";
import ControlPanelPageShell from "@/components/common/control-panel-page-shell/index.vue";
import AuthorizationList from "@/views/mazbri/operations/authorizations/index.vue";
import ProductList from "@/views/mazbri/products/index.vue";

defineOptions({ name: "ProductGovernance" });

type ProductAuthorizationTab = "products" | "authorizations";

const { t } = useI18n();
const headerActionsEl = ref<HTMLElement | null>(null);
provide("resourceTabHeaderActionsEl", headerActionsEl);

const tabOptions = computed(() => [
  { label: t("menus.mazbri.productList"), value: "products" },
  { label: t("menus.mazbri.authorizationList"), value: "authorizations" },
]);

const { activeTab, handleTabChange } = useTabRouteSync<ProductAuthorizationTab>(
  {
    defaultValue: "products",
    allowedValues: ["products", "authorizations"],
  },
);

const loadedTabs = ref<Set<ProductAuthorizationTab>>(new Set([activeTab.value]));
watch(
  activeTab,
  (tab) => {
    loadedTabs.value.add(tab);
  },
  { immediate: true },
);
const isTabLoaded = (tab: ProductAuthorizationTab) => loadedTabs.value.has(tab);
</script>

<style scoped lang="scss">
.product-authorization-page {
  min-width: 0;
  display: flex;
  flex-direction: column;
  flex: 1;
  height: 100%;

  .page-header-tabs-bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px 16px;
    margin-bottom: 12px;
    flex-shrink: 0;
  }

  .page-header-tabs-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-left: auto;
  }

  .page-header-tabs {
    margin-bottom: 0;
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
