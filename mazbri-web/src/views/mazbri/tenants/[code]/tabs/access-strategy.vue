<template>
  <TenantTabShell>
    <div class="strategy-grid">
      <CachePolicyCard class="strategy-card" :tenant-code="tenantCode" />
      <RateLimitCard class="strategy-card" :tenant-code="tenantCode" />
    </div>
  </TenantTabShell>
</template>

<script setup lang="ts">
import RateLimitCard from "./modules/rate-limit-card.vue";
import CachePolicyCard from "./modules/cache-policy-card.vue";
import TenantTabShell from "@/components/common/tenant-tab-shell/index.vue";

defineOptions({ name: "AccessStrategyTab" });
defineProps<{
  tenantCode: string;
}>();
</script>

<style scoped lang="scss">
.strategy-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: var(--ops-section-gap, 16px);
  align-items: start;
}

.strategy-card {
  min-width: 0;
  height: 100%;
}

@media (max-width: 768px) {
  .strategy-grid {
    grid-template-columns: 1fr;
  }
}
</style>
