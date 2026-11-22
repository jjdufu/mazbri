<template>
  <aside
    class="wb-sidebar"
    :class="{
      'is-collapsed': collapsed,
      'is-rail-only': railOnly,
      'is-panel-only': panelOnly,
    }"
  >
    <nav
      v-if="!panelOnly"
      class="wb-rail"
      :aria-label="$t('upstreamConnector.workbenchTitle')"
    >
      <button
        v-for="item in railItems"
        :key="item.id"
        type="button"
        class="wb-rail__btn"
        :class="{ 'is-active': !collapsed && panel === item.id }"
        :title="item.label"
        :aria-label="item.label"
        @click="onRailClick(item.id)"
      >
        <ArtSvgIcon :icon="item.icon" />
        <span v-if="item.badge" class="wb-rail__badge tabular-nums">
          {{ item.badge }}
        </span>
      </button>
    </nav>

    <div v-if="!railOnly && !collapsed" class="wb-panel">
      <!-- Panel 1: Folder-Grouped Endpoint Tree -->
      <template v-if="panel === 'endpoints'">
        <div class="wb-panel__head">
          <span class="wb-panel__title">{{
            $t("upstreamConnector.sidebarEndpoints")
          }}</span>
          <button
            v-if="canWrite"
            type="button"
            class="wb-hopp-icon"
            :title="$t('upstreamConnector.newEndpoint')"
            :aria-label="$t('upstreamConnector.newEndpoint')"
            @click="emit('add-endpoint', '')"
          >
            <ArtSvgIcon icon="ri:add-line" />
          </button>
        </div>

        <div class="wb-panel__search">
          <ArtSvgIcon icon="ri:search-line" class="wb-panel__search-icon" />
          <input
            v-model="epSearch"
            type="text"
            class="wb-panel__search-input"
            :placeholder="$t('upstreamConnector.searchEndpoints')"
            :aria-label="$t('upstreamConnector.searchEndpoints')"
          />
          <button
            v-if="epSearch"
            type="button"
            class="wb-panel__search-clear"
            :aria-label="$t('upstreamConnector.clearKv')"
            @click="epSearch = ''"
          >
            <ArtSvgIcon icon="ri:close-line" />
          </button>
        </div>

        <div class="wb-panel__body">
          <div v-if="!groupedEndpoints.length" class="wb-panel__empty">
            {{ $t("upstreamConnector.noEndpoints") }}
          </div>

          <div
            v-for="group in groupedEndpoints"
            :key="group.tag || '__default__'"
            class="wb-folder"
          >
            <div class="wb-folder__head">
              <button
                type="button"
                class="wb-folder__toggle"
                @click="toggleFolder(group.tag)"
              >
                <ArtSvgIcon
                  :icon="
                    isFolderOpen(group.tag)
                      ? 'ri:arrow-down-s-line'
                      : 'ri:arrow-right-s-line'
                  "
                />
                <ArtSvgIcon icon="ri:stack-line" />
                <span class="wb-folder__name">
                  {{ group.tag || $t("upstreamConnector.untaggedGroup") }}
                </span>
                <span class="wb-folder__count tabular-nums">
                  {{ group.items.length }}
                </span>
              </button>
              <button
                v-if="canWrite"
                type="button"
                class="wb-folder__add"
                :title="$t('upstreamConnector.newEndpoint')"
                @click="emit('add-endpoint', group.tag)"
              >
                <ArtSvgIcon icon="ri:add-line" />
              </button>
            </div>

            <div v-show="isFolderOpen(group.tag)" class="wb-folder__items">
              <div
                v-for="{ ep, index } in group.items"
                :key="ep.id || index"
                class="wb-ep-node"
                :class="{ 'is-active': index === activeIndex }"
                @click="emit('select-endpoint', index)"
              >
                <span
                  class="wb-ep-node__method"
                  :data-tone="methodTone(ep.method)"
                >
                  {{ displayMethodBadge(ep.method) }}
                </span>
                <div class="wb-ep-node__meta">
                  <div class="wb-ep-node__name">
                    {{ ep.name || ep.path || "endpoint" }}
                  </div>
                  <div class="wb-ep-node__path">{{ ep.path || "/" }}</div>
                </div>
                <button
                  type="button"
                  class="wb-ep-node__gate"
                  :class="gateDotClass(ep)"
                  :disabled="!canWrite"
                  :title="
                    ep.gate === false
                      ? $t('upstreamConnector.gateOffHint')
                      : $t('upstreamConnector.gateOnHint')
                  "
                  @click.stop="canWrite && emit('toggle-gate', index)"
                />
                <HoppMenu
                  v-if="canWrite"
                  placement="bottom-end"
                  :append-to="appendTo"
                >
                  <template #trigger>
                    <button
                      type="button"
                      class="wb-ep-node__more"
                      :aria-label="$t('upstreamConnector.sessionActions')"
                      @click.stop
                    >
                      <ArtSvgIcon icon="ri:more-2-fill" />
                    </button>
                  </template>
                  <button
                    type="button"
                    class="hopp-menu-item"
                    @click="emit('endpoint-action', { cmd: 'duplicate', index })"
                  >
                    {{ $t("upstreamConnector.duplicate") }}
                  </button>
                  <button
                    v-if="endpoints.length > 1"
                    type="button"
                    class="hopp-menu-item is-divided is-danger"
                    @click="emit('endpoint-action', { cmd: 'delete', index })"
                  >
                    {{ $t("common.actions.delete") }}
                  </button>
                </HoppMenu>
              </div>
            </div>
          </div>
        </div>
      </template>

      <!-- Panel 2: Multi-Environment & Session Vault -->
      <template v-else-if="panel === 'env'">
        <div class="wb-panel__head">
          <span class="wb-panel__title">{{
            $t("upstreamConnector.sidebarEnv")
          }}</span>
          <div class="wb-panel__head-actions">
            <button
              v-if="canWrite"
              type="button"
              class="wb-hopp-icon"
              :title="$t('upstreamConnector.importEnvJson')"
              @click="envImportInput?.click()"
            >
              <ArtSvgIcon icon="ri:upload-2-line" />
            </button>
            <button
              type="button"
              class="wb-hopp-icon"
              :title="$t('upstreamConnector.exportEnvJson')"
              @click="emit('export-env')"
            >
              <ArtSvgIcon icon="ri:download-2-line" />
            </button>
          </div>
        </div>
        <input
          ref="envImportInput"
          type="file"
          accept=".json,application/json"
          hidden
          @change="onEnvFile"
        />

        <div class="wb-panel__body wb-panel__body--padded" :inert="!canWrite || undefined">
          <div class="wb-env-sec">
            <div class="wb-env-sec__bar">
              <span class="wb-env-sec__label">{{
                $t("upstreamConnector.namedEnvs")
              }}</span>
              <div class="wb-env-sec__new">
                <input
                  v-model="draftEnvName"
                  type="text"
                  class="wb-env-sec__input"
                  :placeholder="$t('upstreamConnector.envNamePlaceholder')"
                  @keydown.enter.prevent="createEnv"
                />
                <button
                  type="button"
                  class="wb-hopp-action"
                  @click="createEnv"
                >
                  {{ $t("upstreamConnector.newEnv") }}
                </button>
              </div>
            </div>

            <div class="wb-env-pills">
              <button
                type="button"
                class="wb-env-pill"
                :class="{ active: !envState.activeEnvId }"
                @click="emit('select-env', null)"
              >
                {{ $t("upstreamConnector.noEnvSelected") }}
              </button>
              <button
                v-for="env in envState.environments"
                :key="env.id"
                type="button"
                class="wb-env-pill"
                :class="{ active: env.id === envState.activeEnvId }"
                @click="emit('select-env', env.id)"
              >
                <span>{{ env.name }}</span>
              </button>
            </div>

            <div v-if="activeNamedEnv" class="wb-env-active-box">
              <div class="wb-env-active-box__head">
                <input
                  v-model="activeNamedEnv.name"
                  type="text"
                  class="wb-env-sec__input"
                  :aria-label="$t('upstreamConnector.namedEnvs')"
                />
                <button
                  type="button"
                  class="wb-hopp-icon"
                  :title="$t('upstreamConnector.deleteEnv')"
                  @click="emit('delete-env', activeNamedEnv.id)"
                >
                  <ArtSvgIcon icon="ri:delete-bin-line" />
                </button>
              </div>
              <WorkbenchKvRows
                v-model="activeNamedEnv.rows"
                :title="activeNamedEnv.name"
                :add-title="$t('upstreamConnector.addEnv')"
                :clear-title="$t('upstreamConnector.clearKv')"
              />
            </div>
          </div>

          <div class="wb-env-sec">
            <WorkbenchKvRows
              v-model="envState.globalRows"
              :title="$t('upstreamConnector.globalEnv')"
              :add-title="$t('upstreamConnector.addEnv')"
              :clear-title="$t('upstreamConnector.clearKv')"
            />
          </div>

          <div class="wb-vault-card">
            <div class="wb-vault-card__info">
              <span class="wb-vault-card__title">{{
                $t("upstreamConnector.authVaultSection")
              }}</span>
              <span class="wb-vault-card__count tabular-nums">
                {{ credentialCount }}
              </span>
            </div>
            <button
              type="button"
              class="wb-hopp-action"
              @click="emit('open-auth-tab')"
            >
              {{ $t("upstreamConnector.authTab") }}
            </button>
          </div>
        </div>
      </template>

      <!-- Panel 3: Publish Gate Checklist -->
      <template v-else-if="panel === 'gate'">
        <div class="wb-panel__head">
          <span class="wb-panel__title">{{
            $t("upstreamConnector.sidebarGate")
          }}</span>
          <span class="wb-panel__counter tabular-nums">
            {{ gate.ok }}/{{ gate.total }}
          </span>
        </div>

        <div class="wb-panel__body">
          <p v-if="leftoverTokens.length" class="wb-gate-warn">
            {{
              $t("upstreamConnector.staticTokenHint", {
                headers: leftoverTokens.join(", "),
              })
            }}
          </p>
          <button
            v-for="(item, idx) in gate.items"
            :key="item.id"
            type="button"
            class="wb-gate-row"
            @click="emit('select-endpoint', idx)"
          >
            <span
              class="wb-ep-node__method"
              :data-tone="methodTone(item.method)"
            >
              {{ displayMethodBadge(item.method) }}
            </span>
            <div class="wb-gate-row__meta">
              <span class="wb-gate-row__name">{{ item.name }}</span>
              <span class="wb-gate-row__reason">{{ gateReasonText(item) }}</span>
            </div>
            <span
              class="wb-ep-node__gate"
              :class="!item.gated ? 'off' : item.ok ? 'ok' : 'pending'"
            />
          </button>
        </div>
      </template>

      <!-- Panel 4: Filterable Probe History -->
      <template v-else-if="panel === 'history'">
        <div class="wb-panel__head">
          <span class="wb-panel__title">{{
            $t("upstreamConnector.sidebarHistory")
          }}</span>
          <span class="wb-panel__counter tabular-nums">{{ runs.length }}</span>
        </div>

        <div class="wb-panel__search">
          <ArtSvgIcon icon="ri:search-line" class="wb-panel__search-icon" />
          <input
            v-model="historySearch"
            type="text"
            class="wb-panel__search-input"
            :placeholder="$t('upstreamConnector.filterHistoryPh')"
            :aria-label="$t('upstreamConnector.filterHistoryPh')"
          />
        </div>

        <div class="wb-panel__body">
          <div v-if="!filteredRuns.length" class="wb-panel__empty">
            {{ $t("common.feedback.noData") }}
          </div>
          <button
            v-for="r in filteredRuns"
            :key="r.id"
            type="button"
            class="wb-hist-row"
            :class="{ 'is-active': r.id === activeRunId }"
            @click="emit('select-run', r)"
          >
            <span class="wb-hist-row__dot" :class="r.ok ? 'ok' : 'err'" />
            <div class="wb-hist-row__main">
              <div class="wb-hist-row__top">
                <span class="wb-hist-row__ep">{{ r.endpoint_key || "—" }}</span>
                <span class="wb-hist-row__ms tabular-nums">
                  {{ r.latency_ms ?? 0 }}ms
                </span>
              </div>
              <div class="wb-hist-row__sub">
                <span>{{ runModeText(r) }}</span>
                <span v-if="r.response_preview?.status" class="tabular-nums">
                  HTTP {{ r.response_preview.status }}
                </span>
                <span>{{ formatTime(r.created_at) }}</span>
              </div>
            </div>
          </button>
        </div>
      </template>
    </div>
  </aside>
</template>

<script setup lang="ts">
import HoppMenu from "@/components/common/hopp-menu/index.vue";
import { $t } from "@/locales";
import type {
  ConnectorEndpoint,
  ConnectorRun,
} from "@/api/upstream-connectors";
import {
  BYPASS_MGMT_PROBE_PATH,
  displayMethodBadge,
  formatTime,
  groupEndpointsByTag,
  methodTone,
  type GateEpStatus,
  type WorkbenchEnvState,
} from "./workbenchHelpers";
import type { SidebarPanelId } from "./useWorkbenchStore";
import WorkbenchKvRows from "./WorkbenchKvRows.vue";

const props = withDefaults(
  defineProps<{
    panel: SidebarPanelId;
    collapsed: boolean;
    railOnly?: boolean;
    panelOnly?: boolean;
    endpoints: ConnectorEndpoint[];
    activeIndex: number;
    gate: { ok: number; total: number; items: GateEpStatus[] };
    leftoverTokens: string[];
    runs: ConnectorRun[];
    activeRunId?: string | null;
    envState: WorkbenchEnvState;
    envVarCount: number;
    credentialCount: number;
    canWrite: boolean;
    appendTo?: HTMLElement;
  }>(),
  {
    railOnly: false,
    panelOnly: false,
    activeRunId: null,
    appendTo: undefined,
  },
);

const emit = defineEmits<{
  "update:panel": [SidebarPanelId];
  "update:collapsed": [boolean];
  "add-endpoint": [string];
  "select-endpoint": [number];
  "toggle-gate": [number];
  "endpoint-action": [{ cmd: string; index: number }];
  "select-env": [string | null];
  "create-env": [string];
  "delete-env": [string];
  "import-env": [string];
  "export-env": [];
  "open-auth-tab": [];
  "select-run": [ConnectorRun];
}>();

const epSearch = ref("");
const historySearch = ref("");
const draftEnvName = ref("");
const envImportInput = ref<HTMLInputElement | null>(null);
const closedFolders = reactive<Record<string, boolean>>({});

const railItems = computed(() => [
  {
    id: "endpoints" as SidebarPanelId,
    icon: "ri:node-tree",
    label: $t("upstreamConnector.sidebarEndpoints"),
    badge: props.endpoints.length ? String(props.endpoints.length) : "",
  },
  {
    id: "env" as SidebarPanelId,
    icon: "ri:code-s-slash-line",
    label: $t("upstreamConnector.sidebarEnv"),
    badge: props.envVarCount ? String(props.envVarCount) : "",
  },
  {
    id: "gate" as SidebarPanelId,
    icon: "ri:shield-check-line",
    label: $t("upstreamConnector.sidebarGate"),
    badge: props.gate.total ? `${props.gate.ok}/${props.gate.total}` : "",
  },
  {
    id: "history" as SidebarPanelId,
    icon: "ri:history-line",
    label: $t("upstreamConnector.sidebarHistory"),
    badge: props.runs.length ? String(props.runs.length) : "",
  },
]);

const groupedEndpoints = computed(() =>
  groupEndpointsByTag(props.endpoints, epSearch.value),
);

const activeNamedEnv = computed(
  () =>
    props.envState.environments.find(
      (e) => e.id === props.envState.activeEnvId,
    ) || null,
);

const filteredRuns = computed(() => {
  const q = historySearch.value.trim().toLowerCase();
  if (!q) return props.runs;
  return props.runs.filter((r) => {
    const blob = [
      r.endpoint_key || "",
      r.mode || "",
      String(r.response_preview?.status ?? ""),
      r.error_code || "",
    ]
      .join(" ")
      .toLowerCase();
    return blob.includes(q);
  });
});

function onRailClick(id: SidebarPanelId) {
  if (!props.collapsed && props.panel === id) {
    emit("update:collapsed", true);
    return;
  }
  emit("update:panel", id);
  if (props.collapsed) emit("update:collapsed", false);
}

function isFolderOpen(tag: string) {
  return !closedFolders[tag || "__default__"];
}

function toggleFolder(tag: string) {
  const key = tag || "__default__";
  closedFolders[key] = !closedFolders[key];
}

function gateDotClass(ep: ConnectorEndpoint) {
  if (ep.gate === false) return "off";
  const hit = props.gate.items.find(
    (g) => g.id === ep.id || g.name === ep.name,
  );
  return hit?.ok ? "ok" : "pending";
}

function gateReasonText(item: GateEpStatus): string {
  if (!item.gated) return $t("upstreamConnector.gateOffHint");
  if (item.ok) return $t("upstreamConnector.runOk");
  switch (item.reason) {
    case "spec_changed":
      return $t("upstreamConnector.gateReasonSpecChanged");
    case "unresolved_vars":
      return $t("upstreamConnector.gateReasonUnresolved");
    case "spec_invalid":
      return $t("upstreamConnector.gateReasonSpecInvalid");
    default:
      return $t("upstreamConnector.gateReasonNotRun");
  }
}

function runModeText(r: ConnectorRun): string {
  if (r.mode === "live" && r.probe_path === BYPASS_MGMT_PROBE_PATH) {
    return $t("upstreamConnector.modeBadge.business");
  }
  const key = `upstreamConnector.modeBadge.${r.mode}`;
  const t = $t(key);
  return t === key ? r.mode : t;
}

function createEnv() {
  const name = draftEnvName.value.trim();
  if (!name) return;
  emit("create-env", name);
  draftEnvName.value = "";
}

async function onEnvFile(ev: Event) {
  const input = ev.target as HTMLInputElement;
  const file = input.files?.[0];
  input.value = "";
  if (!file) return;
  const text = await file.text();
  emit("import-env", text);
}
</script>

<style scoped lang="scss">
.wb-sidebar {
  display: flex;
  width: 100%;
  height: 100%;
  min-height: 0;
  overflow: hidden;
  background: var(--wb-surface);

  &.is-rail-only {
    flex: 0 0 44px;
    width: 44px;
  }
}

.wb-rail {
  display: flex;
  flex-direction: column;
  flex-shrink: 0;
  gap: 4px;
  align-items: center;
  width: 44px;
  padding: 8px 0;
  background: var(--wb-surface-subtle);
  border-right: 1px solid var(--wb-divider);
}

.wb-rail__btn {
  position: relative;
  display: inline-flex;
  flex-direction: column;
  gap: 2px;
  align-items: center;
  justify-content: center;
  width: 34px;
  height: 36px;
  font-size: 16px;
  color: var(--wb-text-faint);
  cursor: pointer;
  background: transparent;
  border: none;
  border-radius: var(--wb-radius);

  &:hover {
    color: var(--wb-text);
    background: var(--wb-surface-hover);
  }

  &.is-active {
    color: var(--wb-text);
    background: var(--wb-surface-active);

    &::before {
      position: absolute;
      top: 6px;
      bottom: 6px;
      left: -5px;
      width: 2px;
      content: "";
      background: var(--wb-primary);
      border-radius: 2px;
    }
  }
}

.wb-rail__badge {
  font-family: var(--wb-font-mono);
  font-size: 9px;
  font-weight: 600;
  line-height: 1;
  color: var(--wb-text-muted);
}

.wb-panel {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
}

.wb-panel__head {
  display: flex;
  flex-shrink: 0;
  align-items: center;
  justify-content: space-between;
  height: 36px;
  padding: 0 10px 0 12px;
  border-bottom: 1px solid var(--wb-divider);
}

.wb-panel__title {
  font-size: 12px;
  font-weight: 600;
  color: var(--wb-text);
}

.wb-panel__head-actions {
  display: flex;
  gap: 2px;
  align-items: center;
}

.wb-panel__counter {
  font-family: var(--wb-font-mono);
  font-size: 11px;
  color: var(--wb-text-muted);
}

.wb-panel__search {
  display: flex;
  flex-shrink: 0;
  gap: 6px;
  align-items: center;
  height: 32px;
  padding: 0 10px;
  background: var(--wb-surface-subtle);
  border-bottom: 1px solid var(--wb-divider);
}

.wb-panel__search-icon {
  flex-shrink: 0;
  font-size: 13px;
  color: var(--wb-text-faint);
}

.wb-panel__search-input {
  flex: 1;
  min-width: 0;
  font-size: 12px;
  color: var(--wb-text);
  background: transparent;
  border: none;
  outline: none;

  &::placeholder {
    color: var(--wb-text-faint);
  }
}

.wb-panel__search-clear {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 2px;
  color: var(--wb-text-faint);
  cursor: pointer;
  background: transparent;
  border: none;
}

.wb-panel__body {
  flex: 1;
  min-height: 0;
  overflow: auto;

  &--padded {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 10px;
  }
}

.wb-panel__empty {
  padding: 24px 12px;
  font-size: 12px;
  color: var(--wb-text-faint);
  text-align: center;
}

.wb-folder {
  border-bottom: 1px solid var(--wb-divider-light);
}

.wb-folder__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: 28px;
  padding: 0 6px 0 8px;
  background: var(--wb-surface-subtle);
}

.wb-folder__toggle {
  display: inline-flex;
  flex: 1;
  gap: 5px;
  align-items: center;
  min-width: 0;
  padding: 0;
  font-size: 11.5px;
  font-weight: 600;
  color: var(--wb-text-muted);
  text-align: left;
  cursor: pointer;
  background: transparent;
  border: none;
}

.wb-folder__name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.wb-folder__count {
  margin-left: auto;
  font-family: var(--wb-font-mono);
  font-size: 10.5px;
  color: var(--wb-text-faint);
}

.wb-folder__add {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  color: var(--wb-text-faint);
  cursor: pointer;
  background: transparent;
  border: none;
  border-radius: 3px;

  &:hover {
    color: var(--wb-text);
    background: var(--wb-surface-hover);
  }
}

.wb-ep-node {
  display: flex;
  gap: 6px;
  align-items: center;
  padding: 6px 8px 6px 12px;
  cursor: pointer;
  border-left: 2px solid transparent;

  &:hover {
    background: var(--wb-surface-hover);
  }

  &.is-active {
    background: var(--wb-surface-input);
    border-left-color: var(--wb-primary);
  }
}

.wb-ep-node__method {
  flex-shrink: 0;
  width: 2.8rem;
  font-family: var(--wb-font-mono);
  font-size: 10px;
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

.wb-ep-node__meta {
  flex: 1;
  min-width: 0;
}

.wb-ep-node__name {
  overflow: hidden;
  text-overflow: ellipsis;
  font-size: 12px;
  font-weight: 500;
  color: var(--wb-text);
  white-space: nowrap;
}

.wb-ep-node__path {
  overflow: hidden;
  text-overflow: ellipsis;
  font-family: var(--wb-font-mono);
  font-size: 10.5px;
  color: var(--wb-text-faint);
  white-space: nowrap;
}

.wb-ep-node__gate {
  flex-shrink: 0;
  width: 7px;
  height: 7px;
  padding: 0;
  cursor: pointer;
  border: none;
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

.wb-ep-node__more {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  padding: 0;
  color: var(--wb-text-faint);
  cursor: pointer;
  background: transparent;
  border: none;
  opacity: 0;
}

.wb-ep-node:hover .wb-ep-node__more {
  opacity: 1;
}

.wb-env-sec {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.wb-env-sec__bar {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.wb-env-sec__label {
  font-size: 11.5px;
  font-weight: 600;
  color: var(--wb-text-muted);
}

.wb-env-sec__new {
  display: flex;
  gap: 6px;
}

.wb-env-sec__input {
  flex: 1;
  min-width: 0;
  height: 28px;
  padding: 0 8px;
  font-size: 12px;
  color: var(--wb-text);
  background: var(--wb-surface-input);
  border: 1px solid var(--wb-divider);
  border-radius: var(--wb-radius);
  outline: none;
}

.wb-env-pills {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}

.wb-env-pill {
  height: 24px;
  padding: 0 8px;
  font-size: 11.5px;
  color: var(--wb-text-muted);
  cursor: pointer;
  background: var(--wb-surface-subtle);
  border: 1px solid var(--wb-divider);
  border-radius: var(--wb-radius);

  &.active {
    font-weight: 600;
    color: var(--wb-primary-fg);
    background: var(--wb-primary);
    border-color: var(--wb-primary);
  }
}

.wb-env-active-box {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 8px;
  border: 1px solid var(--wb-divider);
  border-radius: var(--wb-radius);
}

.wb-env-active-box__head {
  display: flex;
  gap: 6px;
  align-items: center;
}

.wb-vault-card {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px;
  background: var(--wb-surface-subtle);
  border: 1px solid var(--wb-divider);
  border-radius: var(--wb-radius);
}

.wb-vault-card__info {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.wb-vault-card__title {
  font-size: 11.5px;
  font-weight: 600;
  color: var(--wb-text);
}

.wb-vault-card__count {
  font-family: var(--wb-font-mono);
  font-size: 11px;
  color: var(--wb-text-muted);
}

.wb-gate-warn {
  padding: 8px 10px;
  margin: 8px;
  font-size: 11.5px;
  color: var(--wb-warning);
  background: color-mix(in srgb, var(--wb-warning) 10%, transparent);
  border-radius: var(--wb-radius);
}

.wb-gate-row {
  display: flex;
  gap: 8px;
  align-items: center;
  width: 100%;
  padding: 8px 10px;
  text-align: left;
  cursor: pointer;
  background: transparent;
  border: none;
  border-bottom: 1px solid var(--wb-divider-light);

  &:hover {
    background: var(--wb-surface-hover);
  }
}

.wb-gate-row__meta {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-width: 0;
}

.wb-gate-row__name {
  overflow: hidden;
  text-overflow: ellipsis;
  font-size: 12px;
  font-weight: 500;
  color: var(--wb-text);
  white-space: nowrap;
}

.wb-gate-row__reason {
  font-size: 11px;
  color: var(--wb-text-muted);
}

.wb-hist-row {
  display: flex;
  gap: 8px;
  align-items: flex-start;
  width: 100%;
  padding: 8px 10px;
  text-align: left;
  cursor: pointer;
  background: transparent;
  border: none;
  border-bottom: 1px solid var(--wb-divider-light);

  &:hover {
    background: var(--wb-surface-hover);
  }

  &.is-active {
    background: var(--wb-surface-input);
  }
}

.wb-hist-row__dot {
  flex-shrink: 0;
  width: 6px;
  height: 6px;
  margin-top: 6px;
  border-radius: 50%;

  &.ok {
    background: var(--wb-success);
  }

  &.err {
    background: var(--wb-danger);
  }
}

.wb-hist-row__main {
  display: flex;
  flex: 1;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.wb-hist-row__top {
  display: flex;
  gap: 6px;
  align-items: center;
  justify-content: space-between;
}

.wb-hist-row__ep {
  overflow: hidden;
  text-overflow: ellipsis;
  font-size: 12px;
  font-weight: 500;
  color: var(--wb-text);
  white-space: nowrap;
}

.wb-hist-row__ms {
  flex-shrink: 0;
  font-family: var(--wb-font-mono);
  font-size: 11px;
  color: var(--wb-text-muted);
}

.wb-hist-row__sub {
  display: flex;
  gap: 6px;
  font-size: 10.5px;
  color: var(--wb-text-faint);
}
</style>
