<template>
  <div class="access-strategy-tab">
    <div class="strategy-grid">
      <CachePolicyCard class="strategy-card" :tenant-code="tenantCode" />
      <RateLimitCard class="strategy-card" :tenant-code="tenantCode" />
    </div>
  </div>
</template>

<script setup lang="ts">
import RateLimitCard from "./modules/rate-limit-card.vue";
import CachePolicyCard from "./modules/cache-policy-card.vue";

defineOptions({ name: "AccessStrategyTab" });
defineProps<{
  tenantCode: string;
}>();
</script>

<style scoped lang="scss">
.access-strategy-tab {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.strategy-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 16px;
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
