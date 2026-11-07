<template>
  <div class="service-hexagon-grid" :class="{ 'is-compact': compact }">
    <div v-if="!nodes.length" class="service-hexagon-grid__empty">
      <slot name="empty">—</slot>
    </div>
    <div v-else class="hexagon-grid">
      <ElTooltip
        v-for="node in nodes"
        :key="node.id"
        placement="top"
        :show-after="150"
        :disabled="!node.tooltipLines?.length && !node.label"
      >
        <template #content>
          <div class="hexagon-tooltip">
            <template v-if="node.tooltipLines?.length">
              <div
                v-for="(line, idx) in node.tooltipLines"
                :key="idx"
                class="hexagon-tooltip__line"
              >
                {{ line }}
              </div>
            </template>
            <div v-else class="hexagon-tooltip__line">{{ node.label }}</div>
          </div>
        </template>
        <div
          class="hexagon-item"
          :class="[
            `is-${node.status}`,
            { 'is-active': node.id === selectedId },
          ]"
          @click="onSelect(node.id)"
        >
          <div class="hexagon-border">
            <div class="hexagon-inner">
              <span
                class="status-dot"
                :class="{ pulsating: node.status === 'healthy' }"
              />
              <span class="service-abbreviation">{{
                node.shortLabel || shortName(node.label)
              }}</span>
              <span v-if="node.badge" class="hexagon-instance-badge">{{
                node.badge
              }}</span>
            </div>
          </div>
        </div>
      </ElTooltip>
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
  /** 角标文案，如 `v0.3.5` */
  badge?: string;
  /** 悬浮提示多行明细 */
  tooltipLines?: string[];
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
  gap: 10px 8px;
  justify-content: flex-start;
  padding: 6px 2px;
}

.service-hexagon-grid.is-compact .hexagon-grid {
  gap: 10px 8px;
  padding: 4px 0;
}

.hexagon-item {
  position: relative;
  width: 96px;
  height: 110px;
  cursor: pointer;
  transition:
    transform 0.22s cubic-bezier(0.4, 0, 0.2, 1),
    filter 0.22s ease;
  filter: drop-shadow(0 4px 6px rgba(0, 0, 0, 0.1));

  &:hover {
    transform: translateY(-3px) scale(1.04);
    filter: drop-shadow(
      0 6px 12px color-mix(in srgb, var(--theme-color) 24%, transparent)
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
    background: color-mix(
      in srgb,
      var(--art-danger, #f56c6c) 55%,
      var(--art-card-border)
    );
  }
  &.is-degraded .hexagon-border {
    background: color-mix(
      in srgb,
      var(--art-warning, #e6a23c) 45%,
      var(--art-card-border)
    );
  }
  &.is-unknown .hexagon-border {
    background: color-mix(
      in srgb,
      var(--art-gray-400, #909399) 45%,
      var(--art-card-border)
    );
  }

  &.is-down .hexagon-instance-badge {
    color: var(--art-danger, #f56c6c);
    background: color-mix(in srgb, var(--art-danger, #f56c6c) 12%, transparent);
  }
  &.is-degraded .hexagon-instance-badge {
    color: var(--art-warning, #e6a23c);
    background: color-mix(
      in srgb,
      var(--art-warning, #e6a23c) 14%,
      transparent
    );
  }
  &.is-unknown .hexagon-instance-badge {
    color: var(--el-text-color-secondary);
    background: var(--el-fill-color);
  }
}

.service-hexagon-grid.is-compact .hexagon-item {
  width: 92px;
  height: 106px;
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
  background: color-mix(
    in srgb,
    var(--default-box-color) 96%,
    var(--theme-color) 4%
  );
  clip-path: polygon(50% 0%, 100% 25%, 100% 75%, 50% 100%, 0% 75%, 0% 25%);
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 4px;
  padding: 20px 6px;
  box-sizing: border-box;
}

.service-hexagon-grid.is-compact .hexagon-inner {
  padding: 18px 5px;
  gap: 4px;
}

.service-abbreviation {
  font-weight: 600;
  color: var(--art-gray-900);
  font-size: 12px;
  line-height: 1.25;
  text-align: center;
  width: 100%;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  word-break: keep-all;
  overflow-wrap: break-word;
}

.hexagon-instance-badge {
  font-size: 10px;
  font-weight: 600;
  line-height: 1.3;
  color: var(--theme-color);
  background: color-mix(in srgb, var(--theme-color) 10%, transparent);
  padding: 1px 6px;
  border-radius: var(--art-form-control-radius-sm);
  max-width: 88%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.status-dot {
  flex-shrink: 0;
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--art-success);

  &.pulsating {
    box-shadow: 0 0 0 0 rgba(16, 185, 129, 0.6);
    animation: hex-dot-pulse 1.8s infinite;
  }
}

.hexagon-tooltip {
  display: flex;
  flex-direction: column;
  gap: 2px;
  font-size: 12px;
  line-height: 1.5;
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
