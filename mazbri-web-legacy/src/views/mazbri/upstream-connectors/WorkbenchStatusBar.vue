<template>
  <footer class="wb-statusbar">
    <div class="wb-statusbar__left">
      <button
        type="button"
        class="wb-statusbar__btn"
        :title="
          sidebarCollapsed
            ? $t('upstreamConnector.expandSidebar')
            : $t('upstreamConnector.collapseSidebar')
        "
        :aria-label="
          sidebarCollapsed
            ? $t('upstreamConnector.expandSidebar')
            : $t('upstreamConnector.collapseSidebar')
        "
        @click="emit('toggle-sidebar')"
      >
        <svg
          width="13"
          height="13"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <rect x="3" y="3" width="18" height="18" rx="2" />
          <line x1="9" y1="3" x2="9" y2="21" />
        </svg>
      </button>

      <span class="wb-statusbar__sep" aria-hidden="true" />

      <button
        type="button"
        class="wb-statusbar__btn"
        @click="emit('open-env')"
      >
        <span class="wb-statusbar__dot" :class="{ on: Boolean(activeEnvName) }" />
        <span>{{ activeEnvName || $t("upstreamConnector.noEnvSelected") }}</span>
        <span class="tabular-nums">({{ envVarCount }})</span>
      </button>

      <button
        v-if="capturedCount > 0"
        type="button"
        class="wb-statusbar__btn is-accent tabular-nums"
        @click="emit('open-captures')"
      >
        <ArtSvgIcon icon="ri:scan-2-line" />
        <span>captures · {{ capturedCount }}</span>
      </button>
    </div>

    <div class="wb-statusbar__right">
      <button
        type="button"
        class="wb-statusbar__btn tabular-nums"
        @click="emit('toggle-response')"
      >
        <template v-if="running">
          <ArtSvgIcon icon="ri:refresh-line" class="wb-statusbar__spin" />
          <span>{{ $t("upstreamConnector.requesting") }}</span>
        </template>
        <template v-else-if="lastRun">
          <span
            class="wb-statusbar__dot"
            :class="lastRun.ok ? 'on' : 'err'"
          />
          <span>
            {{ lastRun.response_preview?.status ?? (lastRun.ok ? "OK" : "ERR") }}
            · {{ lastRun.latency_ms ?? 0 }} ms
          </span>
        </template>
        <template v-else>
          <span>{{ $t("upstreamConnector.statusReady") }}</span>
        </template>
      </button>

      <span class="wb-statusbar__sep" aria-hidden="true" />

      <button
        type="button"
        class="wb-statusbar__btn"
        :title="
          splitDir === 'vertical'
            ? $t('upstreamConnector.splitTopBottom')
            : $t('upstreamConnector.splitSideBySide')
        "
        :aria-label="
          splitDir === 'vertical'
            ? $t('upstreamConnector.splitTopBottom')
            : $t('upstreamConnector.splitSideBySide')
        "
        @click="emit('toggle-split-dir')"
      >
        <svg
          v-if="splitDir === 'vertical'"
          width="13"
          height="13"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <rect x="3" y="3" width="18" height="18" rx="2" />
          <line x1="12" y1="3" x2="12" y2="21" />
        </svg>
        <svg
          v-else
          width="13"
          height="13"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <rect x="3" y="3" width="18" height="18" rx="2" />
          <line x1="3" y1="12" x2="21" y2="12" />
        </svg>
      </button>

      <button
        type="button"
        class="wb-statusbar__btn"
        :title="
          responseCollapsed
            ? $t('upstreamConnector.expandResponse')
            : $t('upstreamConnector.collapseResponse')
        "
        :aria-label="
          responseCollapsed
            ? $t('upstreamConnector.expandResponse')
            : $t('upstreamConnector.collapseResponse')
        "
        @click="emit('toggle-response')"
      >
        <svg
          width="13"
          height="13"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <rect x="3" y="3" width="18" height="18" rx="2" />
          <line x1="3" y1="15" x2="21" y2="15" />
        </svg>
      </button>

      <button
        type="button"
        class="wb-statusbar__btn"
        :title="$t('upstreamConnector.shortcutsTitle')"
        @click="emit('open-shortcuts')"
      >
        <ArtSvgIcon icon="ri:command-fill" />
        <span>{{ $t("upstreamConnector.shortcutsTitle") }}</span>
      </button>
    </div>
  </footer>
</template>

<script setup lang="ts">
import type { ConnectorRun } from "@/api/upstream-connectors";
import type { SplitDirection } from "./workbenchHelpers";

defineProps<{
  sidebarCollapsed: boolean;
  responseCollapsed: boolean;
  splitDir: SplitDirection;
  activeEnvName: string;
  envVarCount: number;
  capturedCount: number;
  running: boolean;
  lastRun: ConnectorRun | null;
}>();

const emit = defineEmits<{
  "toggle-sidebar": [];
  "toggle-response": [];
  "toggle-split-dir": [];
  "open-env": [];
  "open-captures": [];
  "open-shortcuts": [];
}>();
</script>

<style scoped lang="scss">
.wb-statusbar {
  display: flex;
  flex-shrink: 0;
  align-items: center;
  justify-content: space-between;
  height: 30px;
  min-height: 30px;
  padding: 0 8px;
  font-size: 11.5px;
  color: var(--wb-text-muted);
  background: var(--wb-surface-subtle);
  border-top: 1px solid var(--wb-divider);
}

.wb-statusbar__left,
.wb-statusbar__right {
  display: flex;
  gap: 4px;
  align-items: center;
}

.wb-statusbar__sep {
  width: 1px;
  height: 12px;
  margin: 0 2px;
  background: var(--wb-divider);
}

.wb-statusbar__btn {
  display: inline-flex;
  gap: 5px;
  align-items: center;
  height: 22px;
  padding: 0 6px;
  font-size: 11.5px;
  color: var(--wb-text-muted);
  cursor: pointer;
  background: transparent;
  border: none;
  border-radius: 3px;

  &:hover {
    color: var(--wb-text);
    background: var(--wb-surface-hover);
  }

  &.is-accent {
    color: var(--wb-success);
  }
}

.wb-statusbar__dot {
  width: 6px;
  height: 6px;
  background: var(--wb-text-faint);
  border-radius: 50%;

  &.on {
    background: var(--wb-success);
  }

  &.err {
    background: var(--wb-danger);
  }
}

.wb-statusbar__spin {
  animation: wb-status-spin 0.8s linear infinite;
}

@keyframes wb-status-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
