<template>
  <div class="component-health" :class="{ 'is-dark': dark }">
    <div v-if="groups.length === 0" class="component-health__empty">
      {{ t("ops.components.empty") }}
    </div>
    <section
      v-for="g in groups"
      :key="g.group"
      class="component-health__group"
      :data-group="g.group"
    >
      <header class="component-health__head">
        <span class="component-health__group-name">{{ t(`ops.components.group.${g.group}`) }}</span>
        <span
          class="component-health__count"
          :class="{ 'is-bad': g.abnormal > 0 }"
        >
          {{
            g.abnormal > 0
              ? t("ops.components.abnormalOf", { n: g.abnormal, total: g.items.length })
              : t("ops.components.allHealthy", { total: g.items.length })
          }}
        </span>
      </header>
      <ul class="component-health__items">
        <li
          v-for="c in g.items"
          :key="c.key"
          class="component-health__item"
          :class="`is-${c.status}`"
          :data-key="c.key"
          :data-status="c.status"
        >
          <ElTooltip placement="top" :show-after="200">
            <template #content>
              <div class="component-health__tip">
                <div>{{ c.key }}</div>
                <div>{{ t("ops.components.status") }}：{{ t(`ops.components.statusName.${c.status}`) }}</div>
                <div v-if="c.version">{{ t("ops.components.version") }}：{{ c.version }}</div>
                <div v-if="c.restarts_1h != null">
                  {{ t("ops.components.restarts1h") }}：{{ Math.round(c.restarts_1h) }}
                </div>
                <div v-if="c.cpu_cores != null">CPU：{{ c.cpu_cores.toFixed(2) }} {{ t("ops.components.cores") }}</div>
                <div v-if="c.memory_bytes != null">
                  {{ t("ops.components.memory") }}：{{ formatBytes(c.memory_bytes) }}
                </div>
              </div>
            </template>
            <span class="component-health__label">
              <i class="component-health__dot" />
              <span class="component-health__name">{{ c.name }}</span>
            </span>
          </ElTooltip>
          <span
            v-for="r in c.reasons"
            :key="r"
            class="component-health__reason"
          >
            {{ t(`ops.components.reason.${r}`) }}
          </span>
        </li>
      </ul>
    </section>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import type { ComponentHealth } from "@/types/api/monitoring";
import { formatBytes } from "@/utils/format";
import { groupComponents } from "@/utils/ops/components-health";

/**
 * 组件健康分组清单：Dashboard「实时状态」与大屏资源页共用。
 * 名称、分组、状态均来自后端 `components`（唯一数据源），前端只做分组渲染与 i18n。
 */
defineOptions({ name: "ComponentHealthList" });

const props = withDefaults(
  defineProps<{
    components?: ComponentHealth[] | null;
    /** 大屏深色主题 */
    dark?: boolean;
  }>(),
  { components: null, dark: false },
);

const { t } = useI18n();
const groups = computed(() => groupComponents(props.components));
</script>

<style scoped>
.component-health {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.component-health__head {
  display: flex;
  gap: 8px;
  align-items: baseline;
  margin-bottom: 6px;
  font-size: 13px;
}
.component-health__group-name {
  font-weight: 600;
}
.component-health__count {
  color: var(--el-text-color-secondary);
  font-size: 12px;
}
.component-health__count.is-bad {
  color: var(--el-color-danger);
}
.component-health__items {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
  gap: 6px 12px;
  margin: 0;
  padding: 0;
  list-style: none;
}
.component-health__item {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  align-items: center;
  min-width: 0;
  padding: 4px 8px;
  border-radius: 6px;
  background: var(--el-fill-color-light);
  font-size: 13px;
}
.component-health__label {
  display: inline-flex;
  gap: 6px;
  align-items: center;
  min-width: 0;
}
.component-health__name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.component-health__dot {
  flex-shrink: 0;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--el-color-info);
}
.is-green .component-health__dot { background: var(--el-color-success); }
.is-yellow .component-health__dot { background: var(--el-color-warning); }
.is-red .component-health__dot { background: var(--el-color-danger); }
.is-red { background: var(--el-color-danger-light-9); }
.is-yellow { background: var(--el-color-warning-light-9); }
.component-health__reason {
  padding: 0 4px;
  border-radius: 4px;
  color: var(--el-text-color-secondary);
  font-size: 12px;
}
.is-red .component-health__reason { color: var(--el-color-danger); }
.is-yellow .component-health__reason { color: var(--el-color-warning); }
.component-health__empty {
  color: var(--el-text-color-secondary);
  font-size: 13px;
}
.component-health__tip {
  font-size: 12px;
  line-height: 1.6;
}
.is-dark .component-health__item { background: rgba(120, 160, 220, 0.08); color: #e8eef7; }
.is-dark .component-health__item.is-red { background: rgba(245, 108, 108, 0.18); }
.is-dark .component-health__item.is-yellow { background: rgba(230, 162, 60, 0.18); }
.is-dark .component-health__head { color: #9fb3cd; }
.is-dark .component-health__count { color: #9fb3cd; }
.is-dark .component-health__count.is-bad { color: #f56c6c; }
</style>
