<template>
  <div class="wb-tabs" role="tablist">
    <div class="wb-tabs__scroll">
      <div
        v-for="(item, idx) in tabs"
        :key="item.id"
        role="tab"
        tabindex="0"
        class="wb-tabs__item"
        :class="{ 'is-active': item.id === activeTabId }"
        :aria-selected="item.id === activeTabId"
        @click="emit('select', item.id)"
        @dblclick="canWrite && emit('duplicate', item.index)"
        @keydown="onTabKeydown($event, idx)"
      >
        <span
          class="wb-tabs__method"
          :data-tone="methodTone(item.ep.method)"
        >
          {{ displayMethodBadge(item.ep.method) }}
        </span>
        <span class="wb-tabs__title">
          {{ item.ep.name || item.ep.path || "endpoint" }}
        </span>
        <span
          v-if="item.ep.adapter && item.ep.adapter !== 'http_json'"
          class="wb-tabs__adapter"
        >
          {{ shortAdapterBadge(item.ep.adapter) }}
        </span>
        <ArtSvgIcon
          v-if="item.runtime.running"
          icon="ri:refresh-line"
          class="wb-tabs__spin"
        />
        <span
          v-else
          class="wb-tabs__gate"
          :class="gateClass(item.ep)"
        />
        <button
          v-if="tabs.length > 1"
          type="button"
          class="wb-tabs__close"
          :title="$t('upstreamConnector.closeTab')"
          :aria-label="$t('upstreamConnector.closeTab')"
          @click.stop="emit('close', item.id)"
        >
          <ArtSvgIcon icon="ri:close-line" />
        </button>
      </div>
    </div>

    <button
      v-if="canWrite"
      type="button"
      class="wb-tabs__add"
      :title="$t('upstreamConnector.newEndpoint')"
      :aria-label="$t('upstreamConnector.newEndpoint')"
      @click="emit('add')"
    >
      <ArtSvgIcon icon="ri:add-line" />
    </button>
  </div>
</template>

<script setup lang="ts">
import type { ConnectorEndpoint } from "@/api/upstream-connectors";
import {
  displayMethodBadge,
  methodTone,
  type GateEpStatus,
} from "./workbenchHelpers";
import type { EndpointTabRuntime } from "./useWorkbenchStore";

const props = defineProps<{
  tabs: Array<{
    id: string;
    index: number;
    ep: ConnectorEndpoint;
    runtime: EndpointTabRuntime;
  }>;
  activeTabId: string;
  gateItems: GateEpStatus[];
  canWrite: boolean;
}>();

const emit = defineEmits<{
  select: [string];
  close: [string];
  add: [];
  duplicate: [number];
}>();

function gateClass(ep: ConnectorEndpoint) {
  if (ep.gate === false) return "off";
  const hit = props.gateItems.find(
    (g) => g.id === ep.id || g.name === ep.name,
  );
  return hit?.ok ? "ok" : "pending";
}

function shortAdapterBadge(adapter: string): string {
  switch (adapter) {
    case "http_envelope":
      return "ENV";
    case "sign_form":
      return "SIGN";
    case "jsonrpc":
      return "RPC";
    case "graphql":
      return "GQL";
    case "grpc":
      return "gRPC";
    case "soap_xml":
      return "SOAP";
    case "ws_unary":
      return "WS";
    case "job_poll":
      return "JOB";
    default:
      return adapter.toUpperCase();
  }
}

function onTabKeydown(ev: KeyboardEvent, idx: number) {
  if (ev.key === "ArrowRight") {
    ev.preventDefault();
    const next = props.tabs[(idx + 1) % props.tabs.length];
    if (next) emit("select", next.id);
  } else if (ev.key === "ArrowLeft") {
    ev.preventDefault();
    const prev =
      props.tabs[(idx - 1 + props.tabs.length) % props.tabs.length];
    if (prev) emit("select", prev.id);
  } else if (ev.key === "Enter" || ev.key === " ") {
    ev.preventDefault();
    const cur = props.tabs[idx];
    if (cur) emit("select", cur.id);
  }
}
</script>

<style scoped lang="scss">
.wb-tabs {
  display: flex;
  flex-shrink: 0;
  align-items: stretch;
  height: 36px;
  min-height: 36px;
  background: var(--wb-surface-subtle);
  border-bottom: 1px solid var(--wb-divider);
}

.wb-tabs__scroll {
  display: flex;
  flex: 1;
  align-items: stretch;
  min-width: 0;
  overflow: auto hidden;
}

.wb-tabs__item {
  position: relative;
  display: inline-flex;
  flex-shrink: 0;
  gap: 6px;
  align-items: center;
  max-width: 220px;
  padding: 0 10px 0 12px;
  font-size: 12px;
  color: var(--wb-text-muted);
  cursor: pointer;
  user-select: none;
  background: transparent;
  border-right: 1px solid var(--wb-divider);
  outline: none;

  &:hover {
    color: var(--wb-text);
    background: var(--wb-surface-hover);
  }

  &.is-active {
    font-weight: 600;
    color: var(--wb-primary);
    background: var(--wb-surface);

    &::before {
      position: absolute;
      top: 0;
      right: 0;
      left: 0;
      height: 2px;
      content: "";
      background: var(--wb-primary);
    }
  }
}

.wb-tabs__method {
  flex-shrink: 0;
  font-family: var(--wb-font-mono);
  font-size: 10.5px;
  font-weight: 700;

  &[data-tone="get"] {
    color: var(--wb-method-get);
  }

  &[data-tone="post"] {
    color: var(--wb-method-post);
  }

  &[data-tone="put"] {
    color: var(--wb-method-put);
  }

  &[data-tone="patch"] {
    color: var(--wb-method-patch);
  }

  &[data-tone="delete"] {
    color: var(--wb-method-delete);
  }

  &[data-tone="head"] {
    color: var(--wb-method-head);
  }

  &[data-tone="options"] {
    color: var(--wb-method-options);
  }

  &[data-tone="default"] {
    color: var(--wb-method-default);
  }
}

.wb-tabs__title {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.wb-tabs__adapter {
  flex-shrink: 0;
  padding: 0 4px;
  font-family: var(--wb-font-mono);
  font-size: 9.5px;
  font-weight: 600;
  color: var(--wb-text-muted);
  background: var(--wb-surface-input);
  border: 1px solid var(--wb-divider);
  border-radius: 3px;
}

.wb-tabs__gate {
  flex-shrink: 0;
  width: 6px;
  height: 6px;
  border-radius: 50%;

  &.ok {
    background: var(--wb-success);
  }

  &.pending {
    background: var(--wb-warning);
  }

  &.off {
    background: transparent;
    border: 1px solid var(--wb-text-faint);
  }
}

.wb-tabs__spin {
  flex-shrink: 0;
  font-size: 12px;
  color: var(--wb-primary);
  animation: wb-tab-spin 0.8s linear infinite;
}

@keyframes wb-tab-spin {
  to {
    transform: rotate(360deg);
  }
}

.wb-tabs__close {
  display: inline-flex;
  flex-shrink: 0;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
  padding: 0;
  font-size: 13px;
  color: var(--wb-text-faint);
  cursor: pointer;
  background: transparent;
  border: none;
  border-radius: 3px;
  opacity: 0;

  &:hover {
    color: var(--wb-text);
    background: var(--wb-surface-active);
    opacity: 1;
  }
}

.wb-tabs__item:hover .wb-tabs__close,
.wb-tabs__item.is-active .wb-tabs__close {
  opacity: 0.75;
}

.wb-tabs__add {
  display: inline-flex;
  flex-shrink: 0;
  align-items: center;
  justify-content: center;
  width: 34px;
  color: var(--wb-text-muted);
  cursor: pointer;
  background: transparent;
  border: none;
  border-left: 1px solid var(--wb-divider);

  &:hover {
    color: var(--wb-text);
    background: var(--wb-surface-hover);
  }
}
</style>
