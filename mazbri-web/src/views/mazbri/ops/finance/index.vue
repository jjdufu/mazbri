<template>
  <div class="ops-finance-placeholder art-full-height">
    <ElEmpty :description="t('ops.finance.placeholder')">
      <template #image>
        <ArtSvgIcon icon="ri:line-chart-line" style="font-size: 64px" />
      </template>
    </ElEmpty>
  </div>
</template>

<script setup lang="ts">
import { useI18n } from "vue-i18n";

defineOptions({ name: "FinanceOverviewPlaceholder" });

const { t } = useI18n();
</script>

<style scoped>
.ops-finance-placeholder {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 360px;
  padding: 24px;
}
</style>
