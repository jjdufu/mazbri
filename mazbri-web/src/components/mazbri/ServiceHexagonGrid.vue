<template>
  <div class="service-hexagon-grid" :class="{ 'is-compact': compact }">
    <div v-if="!nodes.length" class="service-hexagon-grid__empty">
      <slot name="empty">—</slot>
    </div>
    <div v-else class="hexagon-grid">
      <div
        v-for="node in nodes"
        :key="node.id"
        class="hexagon-item"
        :class="[
          `is-${node.status}`,
          { 'is-active': node.id === selectedId },
        ]"
        :title="node.label"
        @click="onSelect(node.id)"
      >
        <div class="hexagon-border">
          <div class="hexagon-inner">
            <span class="status-dot" :class="{ pulsating: node.status === 'healthy' }" />
            <span class="service-abbreviation">{{
              node.shortLabel || shortName(node.label)
            }}</span>
            <span v-if="node.badge" class="hexagon-instance-badge">{{
              node.badge
            }}</span>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
export type HexagonStatus = "healthy" | "degraded" | "down" | "unknown";

export interface HexagonNode {
  id: string;
  /** 完整显示名 / 服务名 */
  label: string;
  /** 六边形内短名；缺省由 label 推导 */
  shortLabel?: string;
  status: HexagonStatus;
  /** 角标文案，如 `2 Ins` */
  badge?: string;
}

defineOptions({ name: "ServiceHexagonGrid" });

const props = withDefaults(
  defineProps<{
    nodes: HexagonNode[];
    selectedId?: string | null;
    compact?: boolean;
  }>(),
  {
    selectedId: null,
    compact: false,
  },
);

const emit = defineEmits<{
  select: [id: string];
  "update:selectedId": [id: string | null];
}>();

function shortName(name: string): string {
  return name
    .replace(/^mazbri-/, "")
    .replace(/-service$/, "")
    .replace(/-api$/, "")
    .replace(/-v\d+$/, "");
}

function onSelect(id: string) {
  const next = props.selectedId === id ? null : id;
  emit("update:selectedId", next);
  emit("select", id);
}
</script>

<style scoped lang="scss">
.service-hexagon-grid__empty {
  padding: 16px 4px;
  color: var(--el-text-color-secondary);
  font-size: 13px;
}

.hexagon-grid {
  display: flex;
  flex-wrap: wrap;
  gap: 16px 14px;
  justify-content: flex-start;
  padding: 10px 4px;
}

.service-hexagon-grid.is-compact .hexagon-grid {
  gap: 12px 10px;
  padding: 4px 0;
}

.hexagon-item {
  position: relative;
  width: 90px;
  height: 104px;
  cursor: pointer;
  transition:
    transform 0.22s cubic-bezier(0.4, 0, 0.2, 1),
    filter 0.22s ease;
  filter: drop-shadow(0 4px 6px rgba(0, 0, 0, 0.12));

  &:hover {
    transform: translateY(-4px) scale(1.05);
    filter: drop-shadow(
      0 6px 12px color-mix(in srgb, var(--theme-color) 25%, transparent)
    );
  }

  &.is-active {
    transform: translateY(-2px) scale(1.02);
    filter: drop-shadow(0 0 10px var(--theme-color));

    .hexagon-border {
      background: var(--theme-color);
    }
  }

  &.is-healthy .status-dot {
    background: var(--art-success, #67c23a);
  }
  &.is-degraded .status-dot {
    background: var(--art-warning, #e6a23c);
  }
  &.is-down .status-dot {
    background: var(--art-danger, #f56c6c);
  }
  &.is-unknown .status-dot {
    background: var(--art-gray-400, #909399);
  }

  &.is-down .hexagon-border {
    background: color-mix(in srgb, var(--art-danger, #f56c6c) 55%, var(--art-card-border));
  }
  &.is-degraded .hexagon-border {
    background: color-mix(in srgb, var(--art-warning, #e6a23c) 45%, var(--art-card-border));
  }
}

.service-hexagon-grid.is-compact .hexagon-item {
  width: 76px;
  height: 88px;
}

.hexagon-border {
  width: 100%;
  height: 100%;
  background: var(--art-card-border);
  clip-path: polygon(50% 0%, 100% 25%, 100% 75%, 50% 100%, 0% 75%, 0% 25%);
  display: flex;
  align-items: center;
  justify-content: center;
  transition: background-color 0.22s ease;
}

.hexagon-inner {
  width: calc(100% - 4px);
  height: calc(100% - 4px);
  background: color-mix(in srgb, var(--default-box-color) 96%, var(--theme-color) 4%);
  clip-path: polygon(50% 0%, 100% 25%, 100% 75%, 50% 100%, 0% 75%, 0% 25%);
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: space-between;
  padding: 12px 6px;
  box-sizing: border-box;
}

.service-hexagon-grid.is-compact .hexagon-inner {
  padding: 10px 4px;
}

.service-abbreviation {
  font-weight: 600;
  color: var(--art-gray-900);
  font-size: 12px;
  text-align: center;
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.hexagon-instance-badge {
  font-size: 10px;
  font-weight: 700;
  color: var(--theme-color);
  background: color-mix(in srgb, var(--theme-color) 10%, transparent);
  padding: 1px 5px;
  border-radius: 8px;
  white-space: nowrap;
}

.status-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--art-success);

  &.pulsating {
    box-shadow: 0 0 0 0 rgba(16, 185, 129, 0.6);
    animation: hex-dot-pulse 1.8s infinite;
  }
}

@keyframes hex-dot-pulse {
  0% {
    transform: scale(0.95);
    box-shadow: 0 0 0 0 rgba(16, 185, 129, 0.6);
  }
  70% {
    transform: scale(1);
    box-shadow: 0 0 0 5px rgba(16, 185, 129, 0);
  }
  100% {
    transform: scale(0.95);
    box-shadow: 0 0 0 0 rgba(16, 185, 129, 0);
  }
}

@media (max-width: 768px) {
  .hexagon-grid {
    justify-content: center;
  }
}
</style>
