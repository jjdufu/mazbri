<template>
  <section class="ops-section" :data-section="id">
    <header class="ops-section__head">
      <h3 class="ops-section__title">{{ title }}</h3>
      <span v-if="semantics" class="ops-section__semantics">{{ semantics }}</span>
      <div v-if="$slots.extra" class="ops-section__extra">
        <slot name="extra" />
      </div>
    </header>
    <div class="ops-section__body">
      <slot />
    </div>
  </section>
</template>

<script setup lang="ts">
/**
 * 运维页分区：标题 + 时间语义徽标（「跟随时间窗 …」/「此刻 …」），
 * 让每块数据吃哪段时间一眼可见，而不是靠页面副标题解释。
 */
defineOptions({ name: "OpsSection" });

defineProps<{
  /** 稳定 id（DOM `data-section`，便于验收与埋点） */
  id: string;
  title: string;
  semantics?: string;
}>();
</script>

<style scoped>
.ops-section {
  display: flex;
  flex-direction: column;
  gap: var(--ops-space-sm, 12px);
  min-width: 0;
}
.ops-section__head {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: var(--ops-space-xs, 8px);
}
.ops-section__title {
  margin: 0;
  font-size: 15px;
  font-weight: 600;
  color: var(--el-text-color-primary);
}
.ops-section__semantics {
  padding: 1px 8px;
  border-radius: 999px;
  background: var(--el-fill-color-light);
  color: var(--el-text-color-secondary);
  font-size: 12px;
}
.ops-section__extra {
  margin-left: auto;
}
.ops-section__body {
  display: flex;
  flex-direction: column;
  gap: var(--ops-page-gap, 16px);
  min-width: 0;
  overflow-x: hidden;
}
</style>
