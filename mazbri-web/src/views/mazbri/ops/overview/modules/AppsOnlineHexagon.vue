<template>
  <ServiceHexagonGrid :nodes="nodes">
    <template #empty>
      {{ t("ops.components.empty") }}
    </template>
  </ServiceHexagonGrid>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import ServiceHexagonGrid, {
  type HexagonNode,
  type HexagonStatus,
} from "@/components/mazbri/ServiceHexagonGrid.vue";
import type { ComponentHealth, ComponentStatus } from "@/types/api/monitoring";
import { formatBytes } from "@/utils/format";
import { formatShortVersion } from "@/utils/ops/components-health";

defineOptions({ name: "OpsOverviewAppsOnlineHexagon" });

const props = defineProps<{
  components?: ComponentHealth[] | null;
}>();

const { t } = useI18n();

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
