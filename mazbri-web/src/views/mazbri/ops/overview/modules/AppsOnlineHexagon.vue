<template>
  <ServiceHexagonGrid
    :nodes="nodes"
    compact
  >
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

const nodes = computed<HexagonNode[]>(() =>
  (props.components ?? []).map((c) => ({
    id: c.key,
    label: c.name || c.key,
    status: mapStatus(c.status),
    badge: c.version ? `v${c.version}` : undefined,
  })),
);
</script>
