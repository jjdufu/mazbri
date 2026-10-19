<template>
  <div class="common-detail-hero">
    <div class="common-detail-hero__main">
      <div v-if="eyebrow" class="common-detail-hero__eyebrow">
        {{ eyebrow }}
      </div>
      <div v-if="title" class="common-detail-hero__title">{{ title }}</div>
      <div v-if="subtitle" class="common-detail-hero__subtitle">
        {{ subtitle }}
      </div>
    </div>

    <div v-if="amount || meta" class="common-detail-hero__aside">
      <div v-if="amount" class="common-detail-hero__amount">
        <span>{{ amount }}</span>
      </div>
      <div v-if="meta" class="common-detail-hero__meta">{{ meta }}</div>
    </div>
  </div>
</template>

<script setup lang="ts">
defineOptions({ name: "CommonDetailHero" });

withDefaults(
  defineProps<{
    eyebrow?: string;
    title?: string;
    subtitle?: string;
    amount?: string;
    meta?: string;
  }>(),
  {
    eyebrow: "",
    title: "",
    subtitle: "",
    amount: "",
    meta: "",
  },
);
</script>

<style scoped lang="scss">
.common-detail-hero {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 20px;
  padding: 4px 0 18px;
}

.common-detail-hero__main {
  min-width: 0;
}

.common-detail-hero__eyebrow {
  font-size: 12px;
  font-weight: 600;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--el-color-primary);
}

.common-detail-hero__title {
  margin-top: 6px;
  font-size: 22px;
  font-weight: 700;
  line-height: 1.25;
  color: var(--el-text-color-primary);
  word-break: break-word;
}

.common-detail-hero__subtitle {
  margin-top: 6px;
  color: var(--el-text-color-secondary);
  font-size: 13px;
  word-break: break-word;
}

.common-detail-hero__aside {
  flex: none;
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 8px;
  text-align: right;
}

.common-detail-hero__amount {
  display: flex;
  align-items: baseline;
  gap: 6px;
  font-size: 28px;
  font-weight: 700;
  color: var(--el-text-color-primary);
}

.common-detail-hero__meta {
  font-size: 12px;
  color: var(--el-text-color-secondary);
}

@media (max-width: 768px) {
  .common-detail-hero {
    flex-direction: column;
  }

  .common-detail-hero__aside {
    align-items: flex-start;
    text-align: left;
  }
}
</style>
