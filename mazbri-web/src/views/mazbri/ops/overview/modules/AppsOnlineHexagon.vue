<template>
  <div class="ops-apps-online-hexagon">
    <ServiceHexagonGrid v-model:selected-id="selectedId" :nodes="nodes">
      <template #empty>
        {{ t("ops.components.empty") }}
      </template>
    </ServiceHexagonGrid>

    <div v-if="selectedComponent" class="ops-apps-online-hexagon__detail">
      <div class="ops-apps-online-hexagon__detail-header">
        <div class="ops-apps-online-hexagon__detail-title">
          <span
            class="ops-apps-online-hexagon__dot"
            :class="`is-${mapStatus(selectedComponent.status)}`"
          />
          <span class="ops-apps-online-hexagon__name">
            {{ selectedComponent.name || selectedComponent.key }}
          </span>
          <span class="ops-apps-online-hexagon__key">
            ({{ selectedComponent.key }})
          </span>
          <ElTag size="small" type="info" effect="plain">
            {{ t(`ops.components.group.${selectedComponent.group}`) }}
          </ElTag>
          <ElTag
            size="small"
            :type="statusTagType(selectedComponent.status)"
            effect="light"
          >
            {{ t(`ops.components.statusName.${selectedComponent.status}`) }}
          </ElTag>
        </div>
        <div class="ops-apps-online-hexagon__detail-actions">
          <ElButton
            link
            type="primary"
            @click="
              router.push(
                buildRuntimeRoute({
                  tab: 'apps',
                  service: selectedComponent.key,
                }),
              )
            "
          >
            {{ t("ops.overview.openRuntime") }}
          </ElButton>
          <ElButton link @click="selectedId = null">×</ElButton>
        </div>
      </div>

      <div class="ops-apps-online-hexagon__metrics">
        <div v-if="selectedComponent.reasons.length > 0" class="ops-apps-online-hexagon__metric is-reason">
          <span class="label">{{ t("ops.services.columns.reasons") }}：</span>
          <span class="value">
            {{
              selectedComponent.reasons
                .map((r) => t(`ops.components.reason.${r}`))
                .join("、")
            }}
          </span>
        </div>
        <div class="ops-apps-online-hexagon__metric">
          <span class="label">{{ t("ops.services.columns.container") }}：</span>
          <span class="value">{{ containerText(selectedComponent) }}</span>
        </div>
        <div class="ops-apps-online-hexagon__metric">
          <span class="label">{{ t("ops.services.columns.restarts1h") }}：</span>
          <span class="value">
            {{
              selectedComponent.restarts_1h !== null &&
              selectedComponent.restarts_1h !== undefined
                ? Math.round(selectedComponent.restarts_1h)
                : "—"
            }}
          </span>
        </div>
        <div class="ops-apps-online-hexagon__metric">
          <span class="label">CPU：</span>
          <span class="value">
            {{
              selectedComponent.cpu_cores !== null &&
              selectedComponent.cpu_cores !== undefined
                ? `${selectedComponent.cpu_cores.toFixed(3)} ${t("ops.components.cores")}`
                : "—"
            }}
          </span>
        </div>
        <div class="ops-apps-online-hexagon__metric">
          <span class="label">{{ t("ops.components.memory") }}：</span>
          <span class="value">
            {{
              selectedComponent.memory_bytes !== null &&
              selectedComponent.memory_bytes !== undefined
                ? formatBytes(selectedComponent.memory_bytes)
                : "—"
            }}
          </span>
        </div>
        <div v-if="selectedComponent.version" class="ops-apps-online-hexagon__metric">
          <span class="label">{{ t("ops.components.version") }}：</span>
          <span class="value">v{{ selectedComponent.version }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";
import ServiceHexagonGrid, {
  type HexagonNode,
  type HexagonStatus,
} from "@/components/mazbri/ServiceHexagonGrid.vue";
import type { ComponentHealth, ComponentStatus } from "@/types/api/monitoring";
import { formatBytes } from "@/utils/format";
import { formatShortVersion } from "@/utils/ops/components-health";
import { buildRuntimeRoute } from "@/utils/ops/runtime-link";

defineOptions({ name: "OpsOverviewAppsOnlineHexagon" });

const props = defineProps<{
  components?: ComponentHealth[] | null;
}>();

const { t } = useI18n();
const router = useRouter();
const selectedId = ref<string | null>(null);

const selectedComponent = computed(
  () => (props.components ?? []).find((c) => c.key === selectedId.value) ?? null,
);

watch(
  () => props.components,
  (list) => {
    if (selectedId.value && !(list ?? []).some((c) => c.key === selectedId.value)) {
      selectedId.value = null;
    }
  },
);

function mapStatus(status: ComponentStatus): HexagonStatus {
  switch (status) {
    case "green":
      return "healthy";
    case "yellow":
      return "degraded";
    case "red":
      return "down";
    default:
      return "unknown";
  }
}

function statusTagType(
  status: ComponentStatus,
): "success" | "warning" | "danger" | "info" {
  switch (status) {
    case "green":
      return "success";
    case "yellow":
      return "warning";
    case "red":
      return "danger";
    default:
      return "info";
  }
}

function containerText(c: ComponentHealth): string {
  if (c.container_running === true) return t("ops.services.containerRunning");
  if (c.container_running === false) return t("ops.services.containerStopped");
  if (c.group === "middleware" && c.status === "green") {
    return t("ops.services.containerExternal");
  }
  return "—";
}

function resolveBadge(c: ComponentHealth): string | undefined {
  if (c.reasons.length > 0) {
    return t(`ops.components.reason.${c.reasons[0]}`);
  }
  const shortVer = formatShortVersion(c.version);
  if (shortVer) return shortVer;
  return t(`ops.components.group.${c.group}`);
}

function buildTooltipLines(c: ComponentHealth): string[] {
  const lines: string[] = [
    `${c.name} (${c.key})`,
    `${t("ops.components.status")}：${t(`ops.components.statusName.${c.status}`)}`,
  ];
  if (c.reasons.length > 0) {
    const reasonsText = c.reasons
      .map((r) => t(`ops.components.reason.${r}`))
      .join("、");
    lines.push(reasonsText);
  }
  if (c.version) {
    lines.push(`${t("ops.components.version")}：v${c.version}`);
  }
  if (c.restarts_1h != null) {
    lines.push(
      `${t("ops.components.restarts1h")}：${Math.round(c.restarts_1h)}`,
    );
  }
  if (c.cpu_cores != null) {
    lines.push(
      `CPU：${c.cpu_cores.toFixed(2)} ${t("ops.components.cores")}`,
    );
  }
  if (c.memory_bytes != null) {
    lines.push(`${t("ops.components.memory")}：${formatBytes(c.memory_bytes)}`);
  }
  return lines;
}

const nodes = computed<HexagonNode[]>(() =>
  (props.components ?? []).map((c) => ({
    id: c.key,
    label: c.name || c.key,
    status: mapStatus(c.status),
    badge: resolveBadge(c),
    tooltipLines: buildTooltipLines(c),
  })),
);
</script>

<style scoped>
.ops-apps-online-hexagon {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.ops-apps-online-hexagon__detail {
  padding: 12px 16px;
  border-radius: 8px;
  background: var(--el-fill-color-lighter);
  border: 1px solid var(--el-border-color-lighter);
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.ops-apps-online-hexagon__detail-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  flex-wrap: wrap;
}

.ops-apps-online-hexagon__detail-title {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.ops-apps-online-hexagon__dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  display: inline-block;
}

.ops-apps-online-hexagon__dot.is-healthy {
  background: var(--el-color-success);
}

.ops-apps-online-hexagon__dot.is-degraded {
  background: var(--el-color-warning);
}

.ops-apps-online-hexagon__dot.is-down {
  background: var(--el-color-danger);
}

.ops-apps-online-hexagon__dot.is-unknown {
  background: var(--el-color-info);
}

.ops-apps-online-hexagon__name {
  font-size: 14px;
  font-weight: 600;
  color: var(--el-text-color-primary);
}

.ops-apps-online-hexagon__key {
  font-size: 12px;
  color: var(--el-text-color-secondary);
}

.ops-apps-online-hexagon__detail-actions {
  display: flex;
  align-items: center;
  gap: 8px;
}

.ops-apps-online-hexagon__metrics {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 16px;
  font-size: 12px;
}

.ops-apps-online-hexagon__metric {
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.ops-apps-online-hexagon__metric .label {
  color: var(--el-text-color-secondary);
}

.ops-apps-online-hexagon__metric .value {
  color: var(--el-text-color-primary);
  font-weight: 500;
  font-variant-numeric: tabular-nums;
}

.ops-apps-online-hexagon__metric.is-reason .value {
  color: var(--el-color-danger);
}
</style>
