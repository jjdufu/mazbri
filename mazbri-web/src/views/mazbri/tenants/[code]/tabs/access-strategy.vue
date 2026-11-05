<template>
  <TenantTabShell>
    <SettingsCardGrid>
      <template #left>
        <CachePolicyCard :tenant-code="tenantCode" />
      </template>
      <template #right>
        <RateLimitCard :tenant-code="tenantCode" />
      </template>
    </SettingsCardGrid>
  </TenantTabShell>
</template>

<script setup lang="ts">
import RateLimitCard from "./modules/rate-limit-card.vue";
import CachePolicyCard from "./modules/cache-policy-card.vue";
import TenantTabShell from "@/components/common/tenant-tab-shell/index.vue";
import SettingsCardGrid from "@/components/common/settings-card-grid/index.vue";

defineOptions({ name: "AccessStrategyTab" });
defineProps<{
  tenantCode: string;
}>();
</script>
