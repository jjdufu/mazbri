<template>
  <div class="node-status-card-wrapper">
    <div class="stats-cards">
      <ArtStatsCard
        icon="ri:wifi-line"
        :count="onlineCount"
        :description="$t('configCenter.nodeStatus.onlineNodes')"
        icon-style="bg-emerald-500/15 text-emerald-500"
        text-color="var(--art-success)"
        box-style="border border-solid border-[var(--art-card-border)]"
      />
      <ArtStatsCard
        icon="ri:server-line"
        :count="totalCount"
        :description="$t('configCenter.nodeStatus.totalNodes')"
        icon-style="bg-indigo-500/15 text-indigo-500"
        :text-color="'var(--theme-color)'"
        box-style="border border-solid border-[var(--art-card-border)]"
      />
      <ArtStatsCard
        :icon="statusIcon"
        :count="statusRatio"
        :decimals="0"
        :description="$t('configCenter.nodeStatus.onlineRate')"
        :icon-style="statusIconStyle"
        :text-color="statusTextColor"
        box-style="border border-solid border-[var(--art-card-border)]"
      />
    </div>

    <div class="node-detail-toggle">
      <ElButton
        link
        type="primary"
        class="toggle-button"
        @click="showDetails = !showDetails"
      >
        <ArtSvgIcon
          :icon="showDetails ? 'ri:arrow-up-s-line' : 'ri:arrow-down-s-line'"
          class="toggle-icon"
        />
        {{ $t("configCenter.nodeStatus.onlineNodes") }}
        <span class="toggle-count">{{ onlineCount }} / {{ totalCount }}</span>
      </ElButton>
      <CommonInlineHint>
        {{
          showDetails
            ? $t("configCenter.nodeStatus.collapseDetails")
            : $t("configCenter.nodeStatus.expandDetails")
        }}
      </CommonInlineHint>
    </div>

    <Transition name="fade-slide">
      <div v-if="showDetails" class="node-detail-panel">
        <div
          v-if="loading && onlineNodes.length === 0"
          :loading="true"
          class="loading-container"
        />

        <template v-else-if="hexagonNodes.length > 0">
          <ServiceHexagonGrid
            v-model:selected-id="activeServiceName"
            :nodes="hexagonNodes"
          />
        </template>

        <Transition name="fade-slide">
          <div v-if="activeService" class="active-service-detail-panel">
            <div class="panel-header">
              <div class="service-info-col">
                <span class="active-status-dot" />
                <span class="active-service-name">{{ activeService.service_name }}</span>
                <ElTag size="small" type="info" effect="light" class="instance-total-tag">
                  {{ activeService.instances.length }} {{ $t("configCenter.nodeStatus.instances") }}
                </ElTag>
              </div>
              <ElButton link type="primary" class="close-panel-btn" @click="activeServiceName = null">
                <ArtSvgIcon icon="ri:close-line" class="close-icon" />
              </ElButton>
            </div>

            <div class="instances-grid">
              <div
                v-for="instance in activeService.instances"
                :key="instance.id"
                class="compact-instance-card"
              >
                <div class="instance-ip-row">
                  <ArtSvgIcon icon="ri:global-line" class="meta-icon" />
                  <span class="ip-address">{{ instance.ip }}:{{ instance.port }}</span>
                </div>
                <div class="instance-meta-row">
                  <span class="version-tag">
                    <ArtSvgIcon icon="ri:git-commit-line" class="meta-icon" />
                    v{{ instance.config_version }}
                  </span>
                  <span class="heartbeat-tag">
                    <ArtSvgIcon icon="ri:time-line" class="meta-icon" />
                    {{ formatHeartbeat(instance.last_heartbeat_at) }}
                  </span>
                </div>
              </div>
            </div>
          </div>
        </Transition>
      </div>
    </Transition>
  </div>
</template>

<script setup lang="ts">
import ArtStatsCard from "@/components/core/cards/art-stats-card/index.vue";
import { $t } from "@/locales";
import { fetchNodeList } from "@/api/config-center";
import type { NodeInfo } from "@/types/api/config-center";
import { logger } from "@/utils/sys";
import CommonInlineHint from "@/components/common/inline-hint/index.vue";
import ServiceHexagonGrid, {
  type HexagonNode,
} from "@/components/mazbri/ServiceHexagonGrid.vue";
const loading = ref(false);
const nodes = ref<NodeInfo[]>([]);
const showDetails = ref(false);
const onlineCount = computed(
  () => nodes.value.filter((n) => !isOffline(n)).length,
);
const totalCount = computed(() => nodes.value.length);
const statusRatio = computed(() => {
  if (totalCount.value === 0) return 0;
  return Math.round((onlineCount.value / totalCount.value) * 100);
});
const statusIcon = computed(() => {
  const ratio = statusRatio.value;
  if (ratio === 100) return "ri:checkbox-circle-line";
  if (ratio >= 80) return "ri:error-warning-line";
  return "ri:close-circle-line";
});
const statusIconStyle = computed(() => {
  const ratio = statusRatio.value;
  if (ratio === 100) return "bg-emerald-500/15 text-emerald-500";
  if (ratio >= 80) return "bg-amber-500/15 text-amber-500";
  return "bg-rose-500/15 text-rose-500";
});
const statusTextColor = computed(() => {
  const ratio = statusRatio.value;
  if (ratio === 100) return "var(--art-success)";
  if (ratio >= 80) return "var(--art-warning)";
  return "var(--art-danger)";
});
const isOffline = (node: NodeInfo) => {
  const now = Date.now();
  const heartbeat = new Date(node.last_heartbeat_at).getTime();
  return now - heartbeat > 2 * 60 * 1000;
};
const onlineNodes = computed(() => {
  return nodes.value.filter((n) => !isOffline(n));
});

interface GroupedService {
  service_name: string;
  instances: NodeInfo[];
}

const groupedOnlineServices = computed<GroupedService[]>(() => {
  const groups: Record<string, GroupedService> = {};
  for (const node of onlineNodes.value) {
    if (!groups[node.service_name]) {
      groups[node.service_name] = {
        service_name: node.service_name,
        instances: [],
      };
    }
    groups[node.service_name].instances.push(node);
  }
  return Object.values(groups).sort((a, b) => a.service_name.localeCompare(b.service_name));
});

const activeServiceName = ref<string | null>(null);

const hexagonNodes = computed<HexagonNode[]>(() =>
  groupedOnlineServices.value.map((service) => ({
    id: service.service_name,
    label: service.service_name,
    status: "healthy" as const,
    badge: `${service.instances.length} Ins`,
  })),
);


const activeService = computed(() => {
  if (!activeServiceName.value) return null;
  return groupedOnlineServices.value.find(s => s.service_name === activeServiceName.value) || null;
});


const formatHeartbeat = (date: string) => {
  const now = Date.now();
  const heartbeat = new Date(date).getTime();
  const diff = Math.floor((now - heartbeat) / 1000);
  if (diff < 60)
    return $t("configCenter.nodeStatus.time.secondsAgo", { count: diff });
  if (diff < 3600)
    return $t("configCenter.nodeStatus.time.minutesAgo", {
      count: Math.floor(diff / 60),
    });
  if (diff < 86400)
    return $t("configCenter.nodeStatus.time.hoursAgo", {
      count: Math.floor(diff / 3600),
    });
  return $t("configCenter.nodeStatus.time.daysAgo", {
    count: Math.floor(diff / 86400),
  });
};
const loadNodes = async (silent = false) => {
  if (!silent) loading.value = true;
  try {
    const res = await fetchNodeList();
    nodes.value = res.nodes;
  } catch (error) {
    logger.error("Failed to load node list:", error);
    if (!silent) nodes.value = [];
  } finally {
    loading.value = false;
  }
};
onMounted(() => {
  void loadNodes(false);
});
</script>

<style scoped lang="scss">
.node-status-card-wrapper {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.stats-cards {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
  gap: 10px;

  :deep(.art-card) {
    height: auto;
    min-height: 90px;
    padding: 12px 14px;
    align-items: flex-start;
  }

  :deep(.art-card .size-11) {
    width: 34px;
    height: 34px;
    margin-right: 10px;
  }

  :deep(.art-card .text-2xl) {
    font-size: 1.35rem;
    line-height: 1.1;
  }

  :deep(.art-card .text-sm) {
    font-size: 12px;
    line-height: 1.3;
    margin-top: 2px;
  }
}

.node-detail-toggle {
  display: flex;
  align-items: center;
  gap: 10px;
  justify-content: flex-start;
  padding: 2px 2px 0;
}

.toggle-button {
  padding: 0;
  font-size: 13px;
  font-weight: 600;
}

.toggle-icon {
  margin-right: 4px;
  font-size: 14px;
}

.toggle-count {
  margin-left: 6px;
  color: var(--theme-color);
  font-weight: 700;
}

.node-detail-panel {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.node-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
  max-height: 480px;
  overflow-y: auto;
  padding-right: 6px;

  &::-webkit-scrollbar {
    width: 6px;
  }

  &::-webkit-scrollbar-track {
    background: var(--art-gray-100);
    border-radius: 3px;
  }

  &::-webkit-scrollbar-thumb {
    background: var(--art-gray-300);
    border-radius: 3px;

    &:hover {
      background: var(--art-gray-400);
    }
  }
}

.active-service-detail-panel {
  display: flex;
  flex-direction: column;
  background: var(--art-gray-100);
  border-radius: calc(var(--custom-radius) - 2px);
  border: 1px solid var(--art-card-border);
  padding: 14px;
  gap: 12px;
  margin-top: 4px;
}

.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  border-bottom: 1px solid var(--art-card-border);
  padding-bottom: 8px;
}

.service-info-col {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.active-status-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--art-success);
}

.active-service-name {
  font-weight: 600;
  font-size: 15px;
  color: var(--art-gray-900);
}

.instance-total-tag {
  font-size: 11px;
}

.close-panel-btn {
  padding: 4px;
}

.close-icon {
  font-size: 18px;
  color: var(--art-gray-600);
}

.instances-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
  gap: 10px;
  max-height: 240px;
  overflow-y: auto;
  padding-right: 4px;

  &::-webkit-scrollbar {
    width: 6px;
  }

  &::-webkit-scrollbar-track {
    background: var(--art-gray-100);
    border-radius: 3px;
  }

  &::-webkit-scrollbar-thumb {
    background: var(--art-gray-300);
    border-radius: 3px;

    &:hover {
      background: var(--art-gray-400);
    }
  }
}

.compact-instance-card {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 10px 12px;
  background: var(--default-box-color);
  border-radius: 4px;
  border: 1px solid var(--art-card-border);
  transition: all 0.2s ease;

  &:hover {
    background: color-mix(
      in srgb,
      var(--default-box-color) 94%,
      var(--theme-color)
    );
    border-color: color-mix(
      in srgb,
      var(--theme-color) 15%,
      var(--art-card-border)
    );
  }
}

.instance-ip-row {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  font-weight: 500;
  color: var(--art-gray-800);

  .meta-icon {
    font-size: 14px;
    color: var(--theme-color);
  }
}

.instance-meta-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 11px;
  color: var(--art-gray-600);

  .version-tag,
  .heartbeat-tag {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }

  .meta-icon {
    font-size: 12px;
  }
}

.loading-container {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 200px;
}

.fade-slide-enter-active,
.fade-slide-leave-active {
  transition:
    opacity 0.18s ease,
    transform 0.18s ease;
}

.fade-slide-enter-from,
.fade-slide-leave-to {
  opacity: 0;
  transform: translateY(-6px);
}

@media (max-width: 768px) {
  .stats-cards {
    grid-template-columns: 1fr;
  }

  .node-detail-toggle {
    flex-wrap: wrap;
  }

  .instances-grid {
    grid-template-columns: 1fr;
  }
}
</style>
