<template>
  <div
    ref="pageEl"
    v-loading="bootstrapping"
    class="connector-workbench-page"
  >
    <WorkbenchTopBar
      :draft="draft"
      :recent-drafts="recentDrafts"
      :suppliers="transparentSuppliers"
      :supplier-code="publishForm.supplier_code"
      :is-supplier-linked="isSupplierLinked"
      :can-write="canWriteWorkbench"
      :saving="saving"
      :publishing="publishing"
      :can-publish="canPublish"
      :save-badge="saveBadge"
      :gate="gate"
      :env-state="envState"
      :env-var-count="envVarCount"
      :runtime-env-map="runtimeEnvMap"
      :append-to="pageMount"
      @back="goBack"
      @open-switcher="refreshSwitcherOptions"
      @switch-draft="switchToDraft"
      @switch-supplier="switchToSupplier"
      @new-session="createBlankSession"
      @select-env="setActiveEnv"
      @create-env="addNamedEnv"
      @open-env-panel="openSidebarTo('env')"
      @open-import="showImport = true"
      @export-postman="doExport"
      @import-env="importEnvFromText"
      @export-env="exportEnvToFile"
      @resync="resyncFromLiveSupplier"
      @save="saveDraftExplicit"
      @open-gate="openSidebarTo('gate')"
      @publish="openPublish"
    />

    <div v-if="!draft && !bootstrapping" class="wb-empty">
      <ElEmpty :description="$t('upstreamConnector.emptyHint')">
        <ElButton
          v-if="canWriteWorkbench"
          type="primary"
          @click="createBlankSession"
        >
          {{ $t("upstreamConnector.newSession") }}
        </ElButton>
      </ElEmpty>
    </div>

    <template v-else-if="draft">
      <div class="wb-ide">
        <WorkbenchLeftSidebar
          rail-only
          :panel="activeSidebarPanel"
          :collapsed="sidebarCollapsed"
          :endpoints="endpoints"
          :active-index="activeEpIdx"
          :gate="gate"
          :leftover-tokens="leftoverTokens"
          :runs="runs"
          :active-run-id="lastRun?.id"
          :env-state="envState"
          :env-var-count="envVarCount"
          :credential-count="sessionCredentialKeys.length"
          :can-write="canWriteWorkbench"
          :append-to="pageMount"
          @update:panel="activeSidebarPanel = $event"
          @update:collapsed="sidebarCollapsed = $event"
        />

        <!-- Compact (<1100px) Slide-Over Sidebar Drawer -->
        <template v-if="isCompactViewport && !sidebarCollapsed">
          <div
            class="wb-drawer-backdrop"
            @click="sidebarCollapsed = true"
          />
          <div class="wb-drawer-panel">
            <WorkbenchLeftSidebar
              panel-only
              :panel="activeSidebarPanel"
              :collapsed="false"
              :endpoints="endpoints"
              :active-index="activeEpIdx"
              :gate="gate"
              :leftover-tokens="leftoverTokens"
              :runs="runs"
              :active-run-id="lastRun?.id"
              :env-state="envState"
              :env-var-count="envVarCount"
              :credential-count="sessionCredentialKeys.length"
              :can-write="canWriteWorkbench"
              :append-to="pageMount"
              @add-endpoint="onAddEndpointFromDrawer"
              @select-endpoint="onSelectEndpointFromDrawer"
              @toggle-gate="toggleGate"
              @endpoint-action="onEpAction"
              @select-env="setActiveEnv"
              @create-env="addNamedEnv"
              @delete-env="deleteNamedEnv"
              @import-env="importEnvFromText"
              @export-env="exportEnvToFile"
              @open-auth-tab="onJumpToAuthFromDrawer"
              @select-run="onSelectRunFromDrawer"
            />
          </div>
        </template>

        <Splitpanes
          class="geist-splitter wb-ide__outer"
          @resized="onOuterResized"
        >
          <Pane
            v-if="!isCompactViewport && !sidebarCollapsed"
            :size="sidebarSize"
            :min-size="15"
            :max-size="38"
          >
            <WorkbenchLeftSidebar
              panel-only
              :panel="activeSidebarPanel"
              :collapsed="false"
              :endpoints="endpoints"
              :active-index="activeEpIdx"
              :gate="gate"
              :leftover-tokens="leftoverTokens"
              :runs="runs"
              :active-run-id="lastRun?.id"
              :env-state="envState"
              :env-var-count="envVarCount"
              :credential-count="sessionCredentialKeys.length"
              :can-write="canWriteWorkbench"
              :append-to="pageMount"
              @add-endpoint="addEndpoint"
              @select-endpoint="switchEndpointByIndex"
              @toggle-gate="toggleGate"
              @endpoint-action="onEpAction"
              @select-env="setActiveEnv"
              @create-env="addNamedEnv"
              @delete-env="deleteNamedEnv"
              @import-env="importEnvFromText"
              @export-env="exportEnvToFile"
              @open-auth-tab="reqTab = 'auth'"
              @select-run="selectHistoryRun"
            />
          </Pane>

          <Pane
            :size="
              !isCompactViewport && !sidebarCollapsed ? 100 - sidebarSize : 100
            "
            :min-size="50"
          >
            <div class="wb-workspace">
              <WorkbenchTabStrip
                :tabs="openTabs"
                :active-tab-id="activeTabId"
                :gate-items="gate.items"
                :can-write="canWriteWorkbench"
                @select="openEndpointTab"
                @close="closeEndpointTab"
                @add="addEndpoint('')"
                @duplicate="onEpAction({ cmd: 'duplicate', index: $event })"
              />

              <Splitpanes
                class="geist-splitter wb-ide__inner"
                :horizontal="effectiveHorizontal"
                @resized="onInnerResized"
              >
                <Pane
                  :size="responseCollapsed ? 100 : requestSize"
                  :min-size="25"
                >
                  <div v-if="activeEp" class="wb-request">
                    <WorkbenchMetaBar
                      :endpoint="activeEp"
                      :endpoints="endpoints"
                      :active-index="activeEpIdx"
                      :can-write="canWriteWorkbench"
                      :append-to="pageMount"
                      @select-adapter="onAdapterQuickSelect"
                      @edit-adapter="reqTab = 'transform'"
                    />

                    <div class="wb-runbar">
                      <div
                        class="wb-urlbar"
                        :class="{ 'is-error': urlMissing }"
                        :inert="!canWriteWorkbench || undefined"
                      >
                        <div class="wb-urlbar__method">
                          <WorkbenchMethodSelect
                            :model-value="activeEp.method || 'GET'"
                            :append-to="pageMount"
                            @update:model-value="activeEp.method = $event"
                            @open-change="onMethodOpenChange"
                          />
                        </div>
                        <WorkbenchUrlField
                          ref="urlFieldRef"
                          :model-value="urlDraft"
                          :env="runtimeEnvMap"
                          :placeholder="$t('upstreamConnector.fields.upstreamUrl')"
                          @focus="urlFocused = true"
                          @update:model-value="onUrlInput"
                          @blur="onUrlBlur"
                          @enter="onSendClick"
                          @curl="applyCurlPaste"
                        />
                      </div>

                      <div v-if="canWriteWorkbench" class="wb-runbar__actions">
                        <div class="wb-send-btn">
                          <ElTooltip
                            :content="$t('upstreamConnector.sendProbeTooltip')"
                            placement="bottom"
                            :show-after="300"
                          >
                            <button
                              type="button"
                              class="wb-send-btn__main"
                              :aria-label="
                                running
                                  ? $t('upstreamConnector.sendCancel')
                                  : $t('upstreamConnector.runBusiness')
                              "
                              @click="onSendClick"
                            >
                              <ArtSvgIcon
                                :icon="
                                  running
                                    ? 'ri:close-line'
                                    : 'ri:send-plane-line'
                                "
                              />
                              <span>
                                {{
                                  running
                                    ? $t("upstreamConnector.sendCancel")
                                    : $t("upstreamConnector.send")
                                }}
                              </span>
                            </button>
                          </ElTooltip>

                          <HoppMenu
                            placement="bottom-end"
                            :append-to="pageMount"
                          >
                            <template #trigger>
                              <button
                                type="button"
                                class="wb-send-btn__caret"
                                :aria-label="$t('upstreamConnector.sessionActions')"
                              >
                                <ArtSvgIcon icon="ri:arrow-down-s-line" />
                              </button>
                            </template>
                            <button
                              v-if="draft.linked_supplier_id"
                              type="button"
                              class="hopp-menu-item"
                              @click="onSendCommand('live')"
                            >
                              {{ $t("upstreamConnector.liveRun") }}
                            </button>
                            <button
                              type="button"
                              class="hopp-menu-item"
                              @click="onSendCommand('auth')"
                            >
                              {{ $t("upstreamConnector.runAuth") }}
                            </button>
                            <button
                              type="button"
                              class="hopp-menu-item"
                              @click="onSendCommand('transform')"
                            >
                              {{ $t("upstreamConnector.transformPreview") }}
                            </button>
                            <button
                              type="button"
                              class="hopp-menu-item is-divided"
                              @click="onSendCommand('curl')"
                            >
                              {{ $t("upstreamConnector.copyCurl") }}
                            </button>
                          </HoppMenu>
                        </div>
                      </div>
                    </div>

                    <p
                      v-if="unresolved.length"
                      class="wb-warn-bar"
                    >
                      {{
                        $t("upstreamConnector.unresolvedVars", {
                          vars: unresolved.join(", "),
                        })
                      }}
                    </p>
                    <p
                      v-if="activeStaticCaptureWarnings.length"
                      class="wb-warn-bar"
                    >
                      {{
                        $t("upstreamConnector.staticCaptureWarn", {
                          vars: activeStaticCaptureWarnings.join(", "),
                        })
                      }}
                    </p>

                    <div class="wb-req-subnav" role="tablist">
                      <button
                        v-for="tab in reqSubTabs"
                        :key="tab.id"
                        type="button"
                        role="tab"
                        class="wb-req-subnav__btn"
                        :class="{ 'is-active': reqTab === tab.id }"
                        @click="reqTab = tab.id"
                      >
                        <WorkbenchTabLabel
                          :label="tab.label"
                          :count="tab.count"
                          :dot="tab.dot"
                          :active="reqTab === tab.id"
                        />
                      </button>
                    </div>

                    <div class="wb-req-body">
                      <div
                        v-if="reqTab === 'params'"
                        :inert="!canWriteWorkbench || undefined"
                      >
                        <WorkbenchKvRows
                          v-model="queryRows"
                          :title="$t('upstreamConnector.queryList')"
                          :add-title="$t('upstreamConnector.addQuery')"
                          :clear-title="$t('upstreamConnector.clearKv')"
                          :env="runtimeEnvMap"
                        />
                      </div>

                      <div
                        v-else-if="reqTab === 'body'"
                        class="wb-req-body__fill"
                        :inert="!canWriteWorkbench || undefined"
                      >
                        <WorkbenchBodyPane
                          v-model:gql-var-rows="gqlVarRows"
                          :endpoint="activeEp"
                          :env="runtimeEnvMap"
                          :uploading-artifact="uploadingArtifact"
                          :append-to="pageMount"
                          @pick-artifact="artifactInput?.click()"
                        />
                      </div>

                      <div
                        v-else-if="reqTab === 'headers'"
                        :inert="!canWriteWorkbench || undefined"
                      >
                        <WorkbenchKvRows
                          v-model="headerRows"
                          variant="headers"
                          :title="$t('upstreamConnector.headersList')"
                          :add-title="$t('upstreamConnector.addHeader')"
                          :clear-title="$t('upstreamConnector.clearKv')"
                          :env="runtimeEnvMap"
                          :append-to="pageMount"
                        />
                      </div>

                      <div
                        v-else-if="reqTab === 'auth'"
                        :inert="!canWriteWorkbench || undefined"
                      >
                        <WorkbenchAuthPicker
                          :profile="String(activeEp.auth?.profile || 'none')"
                          :flow="configForm.auth_flow"
                          :adapter="String(activeEp.adapter || 'http_json')"
                          :invalidation-source="activeInvalidationSource"
                          :env="runtimeEnvMap"
                          :seed-json="authSeedJson"
                          :append-to="pageMount"
                          :running="running"
                          @update:profile="setEpAuthProfile"
                          @run="doRun('auth_only', focusUrlField)"
                        />
                      </div>

                      <div
                        v-else-if="reqTab === 'transform'"
                        class="wb-sheet"
                        :inert="!canWriteWorkbench || undefined"
                      >
                        <WorkbenchGhostField
                          :label="$t('upstreamConnector.adapter')"
                          kind="select"
                          :model-value="activeEp.adapter || 'http_json'"
                          :options="adapterSelectOptions"
                          :append-to="pageMount"
                          @update:model-value="activeEp.adapter = String($event)"
                        />
                        <WorkbenchGhostField
                          v-if="
                            activeEp.adapter && activeEp.adapter !== 'http_json'
                          "
                          :label="$t('upstreamConnector.operation')"
                          :model-value="activeEp.operation || ''"
                          :placeholder="$t('upstreamConnector.operationHint')"
                          @update:model-value="
                            activeEp.operation = String($event)
                          "
                        />
                        <WorkbenchGhostField
                          v-if="
                            activeEp.adapter === 'http_envelope' ||
                            activeEp.adapter === 'ws_unary'
                          "
                          :label="$t('upstreamConnector.wrap')"
                          kind="select"
                          :model-value="activeEp.wrap || 'method_biz_content'"
                          :options="wrapSelectOptions"
                          :append-to="pageMount"
                          @update:model-value="activeEp.wrap = String($event)"
                        />
                        <WorkbenchGhostField
                          v-if="
                            activeEp.adapter === 'http_envelope' ||
                            activeEp.adapter === 'http_json' ||
                            activeEp.adapter === 'sign_form'
                          "
                          :label="$t('upstreamConnector.unwrap')"
                          :model-value="activeEp.unwrap || ''"
                          placeholder="$.data"
                          @update:model-value="activeEp.unwrap = String($event)"
                        />
                        <WorkbenchGhostField
                          v-if="
                            activeEp.adapter === 'http_json' ||
                            activeEp.adapter === 'http_envelope'
                          "
                          :label="$t('upstreamConnector.egressContentType')"
                          :model-value="activeEp.content_type || ''"
                          placeholder="application/json"
                          @update:model-value="
                            activeEp.content_type = String($event)
                          "
                        />
                        <WorkbenchGhostField
                          v-if="activeEp.adapter === 'http_json'"
                          :label="$t('upstreamConnector.egressBodyEncoding')"
                          kind="select"
                          :model-value="resolvedBodyEncoding(activeEp)"
                          :options="encodingSelectOptions"
                          :append-to="pageMount"
                          @update:model-value="
                            onBodyEncodingChange(String($event))
                          "
                        />
                        <div class="wb-sheet-row is-block">
                          <div class="wb-sheet-row__label">
                            {{ $t("upstreamConnector.cryptoSection") }}
                          </div>
                          <div class="wb-sheet-row__control">
                            <CryptoProfileEditor
                              :model-value="activeEp.crypto"
                              :disabled="!canWriteWorkbench"
                              :can-toggle-insecure="isSuperAdmin"
                              :credential-keys="sessionCredentialKeys"
                              @update:model-value="onCryptoProfileChange"
                            />
                          </div>
                        </div>
                        <WorkbenchKvRows
                          v-if="activeEp.adapter === 'graphql'"
                          v-model="gqlEnumRows"
                          empty-compact
                          :title="$t('upstreamConnector.graphqlEnumMap')"
                          :add-title="$t('upstreamConnector.addQuery')"
                          :clear-title="$t('upstreamConnector.clearKv')"
                          :env="runtimeEnvMap"
                        />
                      </div>

                      <div
                        v-else-if="reqTab === 'assert'"
                        :inert="!canWriteWorkbench || undefined"
                      >
                        <WorkbenchAssertEditor
                          v-model="activeAssertions"
                          :append-to="pageMount"
                        />
                      </div>

                      <div v-else-if="reqTab === 'captures'">
                        <WorkbenchCapturesEditor
                          v-model="activeCaptures"
                          :captured-vars="capturedVars"
                          :disabled="!canWriteWorkbench"
                          @clear-captured="clearCapturedVars"
                          @remove-captured="removeCapturedVar"
                        />
                      </div>

                      <div v-else-if="reqTab === 'tls'">
                        <WorkbenchTlsTimeoutPane
                          v-model:supplier-timeout-ms="supplierTimeoutMs"
                          v-model:supplier-tls="supplierTlsForm"
                          v-model:endpoint-tls="endpointTlsForm"
                          :endpoint="activeEp"
                          :can-write="canWriteWorkbench"
                          :is-super-admin="isSuperAdmin"
                        />
                      </div>
                    </div>

                    <input
                      ref="artifactInput"
                      type="file"
                      accept=".pb,.fds,.bin,.protobuf"
                      hidden
                      :aria-label="$t('upstreamConnector.artifactUpload')"
                      @change="onArtifactFile"
                    />
                  </div>
                  <ElEmpty
                    v-else
                    :description="$t('upstreamConnector.noEndpoints')"
                  />
                </Pane>

                <Pane
                  v-if="!responseCollapsed"
                  :size="100 - requestSize"
                  :min-size="22"
                >
                  <WorkbenchResponsePane
                    v-model:tab="resTab"
                    :run="lastRun"
                    :runs="runs"
                    :send-block-reason="urlMissing ? 'url' : ''"
                    :loading="running"
                    :adapter="activeEp?.adapter"
                    :inbound="
                      activeEp
                        ? {
                            method: activeEp.method,
                            path: activeEp.path,
                            body: activeEp.body,
                          }
                        : undefined
                    "
                    @select-run="selectHistoryRun"
                    @open-shortcuts="showShortcuts = true"
                  />
                </Pane>
              </Splitpanes>
            </div>
          </Pane>
        </Splitpanes>
      </div>

      <WorkbenchStatusBar
        :sidebar-collapsed="sidebarCollapsed"
        :response-collapsed="responseCollapsed"
        :split-dir="responseSplitDir"
        :active-env-name="activeNamedEnv?.name || ''"
        :env-var-count="envVarCount"
        :captured-count="capturedCount"
        :running="running"
        :last-run="lastRun"
        @toggle-sidebar="sidebarCollapsed = !sidebarCollapsed"
        @toggle-response="responseCollapsed = !responseCollapsed"
        @toggle-split-dir="toggleSplitDirection"
        @open-env="openSidebarTo('env')"
        @open-captures="reqTab = 'captures'"
        @open-shortcuts="showShortcuts = true"
      />
    </template>

    <WorkbenchImportDialog
      v-model="showImport"
      v-model:import-tab="importTab"
      v-model:import-curl-text="importCurlText"
      v-model:import-json-text="importJsonText"
      v-model:mode="importMode"
      :importing="importing"
      @confirm="doImport"
    />

    <WorkbenchCommandPalette
      v-model="showCommands"
      :commands="commandItems"
      @run="onCommand"
    />
    <WorkbenchShortcutsPanel v-model="showShortcuts" :rows="shortcutRows" />

    <WorkbenchPublishDrawer
      v-model="showPublish"
      v-model:form="publishForm"
      v-model:channels="publishChannels"
      :publishing="publishing"
      :result="publishResult"
      :result-desc="publishResultDesc"
      :gate="gate"
      :can-force-publish="canForcePublish"
      :leftover-tokens="leftoverTokens"
      :unresolved-vars="unresolvedAll"
      :endpoints="endpoints"
      @confirm="doPublish"
    />
  </div>
</template>

<script setup lang="ts">
import "splitpanes/dist/splitpanes.css";
import { Pane, Splitpanes, type SplitpanesResizedPayload } from "splitpanes";
import { useMediaQuery } from "@vueuse/core";
import { onBeforeRouteLeave, onBeforeRouteUpdate } from "vue-router";
import { $t } from "@/locales";
import HoppMenu from "@/components/common/hopp-menu/index.vue";
import CryptoProfileEditor from "./CryptoProfileEditor.vue";
import WorkbenchAssertEditor from "./WorkbenchAssertEditor.vue";
import WorkbenchCapturesEditor from "./WorkbenchCapturesEditor.vue";
import WorkbenchImportDialog from "./WorkbenchImportDialog.vue";
import WorkbenchKvRows from "./WorkbenchKvRows.vue";
import WorkbenchLeftSidebar from "./WorkbenchLeftSidebar.vue";
import WorkbenchMetaBar from "./WorkbenchMetaBar.vue";
import WorkbenchPublishDrawer from "./WorkbenchPublishDrawer.vue";
import WorkbenchResponsePane from "./WorkbenchResponsePane.vue";
import WorkbenchStatusBar from "./WorkbenchStatusBar.vue";
import WorkbenchTabStrip from "./WorkbenchTabStrip.vue";
import WorkbenchTlsTimeoutPane from "./WorkbenchTlsTimeoutPane.vue";
import WorkbenchTopBar from "./WorkbenchTopBar.vue";
import WorkbenchAuthPicker from "./hopp/WorkbenchAuthPicker.vue";
import WorkbenchBodyPane from "./hopp/WorkbenchBodyPane.vue";
import WorkbenchCommandPalette from "./hopp/WorkbenchCommandPalette.vue";
import WorkbenchGhostField from "./hopp/WorkbenchGhostField.vue";
import WorkbenchMethodSelect from "./hopp/WorkbenchMethodSelect.vue";
import WorkbenchShortcutsPanel from "./hopp/WorkbenchShortcutsPanel.vue";
import WorkbenchTabLabel from "./hopp/WorkbenchTabLabel.vue";
import WorkbenchUrlField from "./hopp/WorkbenchUrlField.vue";
import {
  EGRESS_ADAPTERS,
  WRAP_OPTIONS,
  resolvedBodyEncoding,
} from "./workbenchHelpers";
import {
  useWorkbenchStore,
  type RequestSubTabId,
  type SidebarPanelId,
} from "./useWorkbenchStore";

defineOptions({ name: "UpstreamConnectorWorkbench" });

const route = useRoute();
const store = useWorkbenchStore();

const {
  isSuperAdmin,
  canWriteWorkbench,
  canForcePublish,
  activeSidebarPanel,
  sidebarCollapsed,
  responseCollapsed,
  responseSplitDir,
  sidebarSize,
  requestSize,
  draft,
  recentDrafts,
  transparentSuppliers,
  refreshSwitcherOptions,
  switchToDraft,
  switchToSupplier,
  createBlankSession,
  bootstrapping,
  saving,
  saveBadge,
  isSupplierLinked,
  shouldGuardLeave,
  confirmDiscardIfDirty,
  saveDraftExplicit,
  resyncFromLiveSupplier,
  goBack,
  bootstrap,
  endpoints,
  openTabs,
  activeTabId,
  activeEpIdx,
  activeEp,
  reqTab,
  resTab,
  running,
  lastRun,
  runs,
  openEndpointTab,
  closeEndpointTab,
  switchEndpointByIndex,
  addEndpoint,
  onEpAction,
  toggleGate,
  selectHistoryRun,
  headerRows,
  queryRows,
  gqlVarRows,
  gqlEnumRows,
  urlDraft,
  urlFocused,
  urlMissing,
  uploadingArtifact,
  configForm,
  supplierTimeoutMs,
  supplierTlsForm,
  endpointTlsForm,
  activeInvalidationSource,
  activeAssertions,
  activeCaptures,
  sessionCredentialKeys,
  authSeedJson,
  queryCount,
  headerCount,
  captureCount,
  bodyFilled,
  authFilled,
  transformFilled,
  assertFilled,
  tlsFilled,
  setEpAuthProfile,
  onCryptoProfileChange,
  onBodyEncodingChange,
  onAdapterQuickSelect,
  onUrlInput,
  onUrlBlur,
  applyCurlPaste,
  copyCurl,
  onArtifactFile,
  envState,
  activeNamedEnv,
  capturedVars,
  capturedCount,
  envVarCount,
  runtimeEnvMap,
  unresolved,
  unresolvedAll,
  activeStaticCaptureWarnings,
  addNamedEnv,
  deleteNamedEnv,
  setActiveEnv,
  exportEnvToFile,
  importEnvFromText,
  clearCapturedVars,
  removeCapturedVar,
  doRun,
  doLiveRun,
  doTransformPreview,
  abortActiveRun,
  abortAllRuns,
  importing,
  publishing,
  showImport,
  showPublish,
  showCommands,
  showShortcuts,
  importTab,
  importMode,
  importCurlText,
  importJsonText,
  publishForm,
  publishChannels,
  publishResult,
  publishResultDesc,
  canPublish,
  gate,
  leftoverTokens,
  doImport,
  doExport,
  openPublish,
  doPublish,
} = store;

const pageEl = ref<HTMLElement | null>(null);
const pageMount = computed(() => pageEl.value ?? undefined);
const urlFieldRef = ref<{ blur: () => void; focus: () => void } | null>(null);
const artifactInput = ref<HTMLInputElement | null>(null);

const isCompactViewport = useMediaQuery("(max-width: 1100px)");

watch(isCompactViewport, (compact) => {
  if (compact) sidebarCollapsed.value = true;
});

const effectiveHorizontal = computed(() => {
  if (isCompactViewport.value) return true;
  return responseSplitDir.value === "horizontal";
});

function focusUrlField() {
  void nextTick(() => urlFieldRef.value?.focus());
}

function openSidebarTo(panel: SidebarPanelId) {
  activeSidebarPanel.value = panel;
  sidebarCollapsed.value = false;
}

function toggleSplitDirection() {
  responseSplitDir.value =
    responseSplitDir.value === "horizontal" ? "vertical" : "horizontal";
}

function onOuterResized(payload: SplitpanesResizedPayload) {
  if (isCompactViewport.value || sidebarCollapsed.value) return;
  const first = payload.panes?.[0];
  if (first && Number.isFinite(first.size)) {
    sidebarSize.value = Math.min(38, Math.max(15, Math.round(first.size)));
  }
}

function onInnerResized(payload: SplitpanesResizedPayload) {
  if (responseCollapsed.value) return;
  const first = payload.panes?.[0];
  if (first && Number.isFinite(first.size)) {
    requestSize.value = Math.min(78, Math.max(25, Math.round(first.size)));
  }
}

function onAddEndpointFromDrawer(tag: string) {
  addEndpoint(tag);
  sidebarCollapsed.value = true;
}

function onSelectEndpointFromDrawer(idx: number) {
  switchEndpointByIndex(idx);
  sidebarCollapsed.value = true;
}

function onJumpToAuthFromDrawer() {
  reqTab.value = "auth";
  sidebarCollapsed.value = true;
}

function onSelectRunFromDrawer(r: Parameters<typeof selectHistoryRun>[0]) {
  selectHistoryRun(r);
  sidebarCollapsed.value = true;
}

function optionLabel(group: "adapterNames" | "wrapNames", value: string) {
  const key = `upstreamConnector.${group}.${value}`;
  const t = $t(key);
  return t === key ? value : t;
}

const adapterSelectOptions = computed(() =>
  EGRESS_ADAPTERS.map((a) => ({
    label: optionLabel("adapterNames", a),
    value: a,
  })),
);

const wrapSelectOptions = computed(() =>
  WRAP_OPTIONS.map((w) => ({
    label: optionLabel("wrapNames", w),
    value: w,
  })),
);

const encodingSelectOptions = computed(() => [
  { label: $t("upstreamConnector.egressEncodingJson"), value: "json" },
  { label: $t("upstreamConnector.egressEncodingForm"), value: "form" },
  { label: $t("upstreamConnector.egressEncodingNone"), value: "none" },
]);

const reqSubTabs = computed<
  Array<{ id: RequestSubTabId; label: string; count?: number; dot?: boolean }>
>(() => [
  {
    id: "params",
    label: $t("upstreamConnector.paramsTab"),
    count: queryCount.value,
  },
  {
    id: "body",
    label: $t("upstreamConnector.bodyTab"),
    dot: bodyFilled.value,
  },
  {
    id: "headers",
    label: $t("upstreamConnector.headersTab"),
    count: headerCount.value,
  },
  {
    id: "auth",
    label: $t("upstreamConnector.authTab"),
    dot: authFilled.value,
  },
  {
    id: "transform",
    label: $t("upstreamConnector.transformTab"),
    dot: transformFilled.value,
  },
  {
    id: "assert",
    label: $t("upstreamConnector.assertTab"),
    dot: assertFilled.value,
  },
  {
    id: "captures",
    label: $t("upstreamConnector.capturesTab"),
    count: captureCount.value,
  },
  {
    id: "tls",
    label: $t("upstreamConnector.tlsTab"),
    dot: tlsFilled.value,
  },
]);

function onSendClick() {
  if (!canWriteWorkbench.value) return;
  if (running.value) {
    abortActiveRun();
    return;
  }
  void doRun("business", focusUrlField);
}

function onSendCommand(cmd: string) {
  if (cmd === "auth") void doRun("auth_only", focusUrlField);
  else if (cmd === "transform") void doTransformPreview();
  else if (cmd === "curl") void copyCurl();
  else if (cmd === "live") void doLiveRun();
}

function onMethodOpenChange(open: boolean) {
  if (open) {
    urlFieldRef.value?.blur();
    return;
  }
  const active = document.activeElement;
  if (active instanceof HTMLElement && active.closest(".wb-method")) {
    active.blur();
  }
}

const shortcutRows = computed(() => [
  {
    id: "send",
    label: $t("upstreamConnector.send"),
    keys: $t("upstreamConnector.sendHotkey"),
  },
  {
    id: "save",
    label: $t("upstreamConnector.syncDraftNow"),
    keys: "Ctrl/⌘+S",
  },
  {
    id: "sidebar",
    label: $t("upstreamConnector.collapseSidebar"),
    keys: "Ctrl/⌘+B",
  },
  {
    id: "shortcuts",
    label: $t("upstreamConnector.shortcutsTitle"),
    keys: "Ctrl/⌘+/",
  },
  {
    id: "commands",
    label: $t("upstreamConnector.commandPalette"),
    keys: "Ctrl/⌘+K",
  },
  {
    id: "close",
    label: $t("upstreamConnector.shortcutsClose"),
    keys: "Esc",
  },
]);

const commandItems = computed(() => {
  const items: { id: string; label: string; hint?: string; keywords?: string }[] =
    [
      {
        id: "send",
        label: $t("upstreamConnector.send"),
        hint: $t("upstreamConnector.sendHotkey"),
      },
      { id: "auth", label: $t("upstreamConnector.runAuth") },
      { id: "transform", label: $t("upstreamConnector.transformPreview") },
      { id: "curl", label: $t("upstreamConnector.copyCurl") },
      {
        id: "save",
        label: $t("upstreamConnector.syncDraftNow"),
        hint: "Ctrl/⌘+S",
      },
      { id: "publish", label: $t("upstreamConnector.publish") },
      {
        id: "tab:params",
        label: $t("upstreamConnector.paramsTab"),
        keywords: "params query",
      },
      {
        id: "tab:body",
        label: $t("upstreamConnector.bodyTab"),
        keywords: "body json",
      },
      {
        id: "tab:headers",
        label: $t("upstreamConnector.headersTab"),
        keywords: "headers",
      },
      {
        id: "tab:auth",
        label: $t("upstreamConnector.authTab"),
        keywords: "auth",
      },
      {
        id: "tab:transform",
        label: $t("upstreamConnector.transformTab"),
        keywords: "egress",
      },
      {
        id: "tab:assert",
        label: $t("upstreamConnector.assertTab"),
        keywords: "assert",
      },
      {
        id: "tab:captures",
        label: $t("upstreamConnector.capturesTab"),
        keywords: "captures extract",
      },
      {
        id: "tab:tls",
        label: $t("upstreamConnector.tlsTab"),
        keywords: "tls mtls timeout",
      },
      {
        id: "panel:env",
        label: $t("upstreamConnector.sidebarEnv"),
        keywords: "env variables",
      },
      { id: "method:GET", label: "GET" },
      { id: "method:POST", label: "POST" },
      { id: "method:PUT", label: "PUT" },
      { id: "method:PATCH", label: "PATCH" },
      { id: "method:DELETE", label: "DELETE" },
    ];
  endpoints.value.forEach((ep, i) => {
    items.push({
      id: `ep:${i}`,
      label: ep.name || ep.path || `ep-${i + 1}`,
      keywords: `${ep.method || ""} ${ep.path || ""} ${ep.tag || ""}`,
    });
  });
  return items;
});

function onCommand(id: string) {
  if (id === "send") {
    onSendClick();
    return;
  }
  if (id === "auth") {
    void doRun("auth_only", focusUrlField);
    return;
  }
  if (id === "transform") {
    void doTransformPreview();
    return;
  }
  if (id === "curl") {
    void copyCurl();
    return;
  }
  if (id === "save") {
    void saveDraftExplicit();
    return;
  }
  if (id === "publish") {
    void openPublish();
    return;
  }
  if (id === "panel:env") {
    openSidebarTo("env");
    return;
  }
  if (id.startsWith("tab:")) {
    reqTab.value = id.slice(4) as RequestSubTabId;
    return;
  }
  if (id.startsWith("method:") && activeEp.value) {
    activeEp.value.method = id.slice(7);
    return;
  }
  if (id.startsWith("ep:")) {
    const idx = Number(id.slice(3));
    if (Number.isFinite(idx)) switchEndpointByIndex(idx);
  }
}

function onHotkey(e: KeyboardEvent) {
  if (e.key === "Escape") {
    if (showCommands.value) {
      e.preventDefault();
      showCommands.value = false;
      return;
    }
    if (showShortcuts.value) {
      e.preventDefault();
      showShortcuts.value = false;
      return;
    }
    if (isCompactViewport.value && !sidebarCollapsed.value) {
      e.preventDefault();
      sidebarCollapsed.value = true;
    }
    return;
  }
  const meta = e.ctrlKey || e.metaKey;
  if (!meta) return;
  if (e.key === "Enter") {
    e.preventDefault();
    onSendClick();
  } else if (e.key.toLowerCase() === "s") {
    e.preventDefault();
    void saveDraftExplicit();
  } else if (e.key.toLowerCase() === "b") {
    e.preventDefault();
    sidebarCollapsed.value = !sidebarCollapsed.value;
  } else if (e.key.toLowerCase() === "k") {
    e.preventDefault();
    showCommands.value = !showCommands.value;
    showShortcuts.value = false;
  } else if (e.key === "/") {
    e.preventDefault();
    showShortcuts.value = !showShortcuts.value;
    showCommands.value = false;
  }
}

const scrollHintTimers = new WeakMap<HTMLElement, number>();

function onWorkbenchScroll(e: Event) {
  const t = e.target;
  if (!(t instanceof HTMLElement)) return;
  t.classList.add("wb-scrolling");
  const prev = scrollHintTimers.get(t);
  if (prev) window.clearTimeout(prev);
  scrollHintTimers.set(
    t,
    window.setTimeout(() => {
      t.classList.remove("wb-scrolling");
      scrollHintTimers.delete(t);
    }, 700),
  );
}

function onBeforeUnload(e: BeforeUnloadEvent) {
  if (!shouldGuardLeave()) return;
  e.preventDefault();
  e.returnValue = "";
}

onBeforeRouteLeave(async () => {
  return await confirmDiscardIfDirty();
});

onBeforeRouteUpdate(async () => {
  return await confirmDiscardIfDirty();
});

onMounted(() => {
  void bootstrap();
  window.addEventListener("keydown", onHotkey);
  window.addEventListener("beforeunload", onBeforeUnload);
  pageEl.value?.addEventListener("scroll", onWorkbenchScroll, true);
});

onUnmounted(() => {
  abortAllRuns();
  window.removeEventListener("keydown", onHotkey);
  window.removeEventListener("beforeunload", onBeforeUnload);
  pageEl.value?.removeEventListener("scroll", onWorkbenchScroll, true);
});

watch(
  () => [route.query.supplier_id, route.query.session] as const,
  async ([supplierId, sessionId], prev) => {
    if (bootstrapping.value) return;
    const [prevSid, prevSession] = prev || [undefined, undefined];
    if (supplierId === prevSid && sessionId === prevSession) return;
    if (supplierId && typeof supplierId === "string") {
      if (draft.value?.linked_supplier_id === supplierId) return;
      await switchToSupplier(supplierId);
      return;
    }
    if (
      sessionId &&
      typeof sessionId === "string" &&
      sessionId !== draft.value?.id
    ) {
      await switchToDraft(sessionId);
      return;
    }
    if (!supplierId && !sessionId) {
      await createBlankSession();
    }
  },
);
</script>

<style lang="scss">
@use "./workbench-tokens.scss";
</style>

<style scoped lang="scss">
.connector-workbench-page {
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100dvh;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
  box-sizing: border-box;
}

.wb-empty {
  display: flex;
  flex: 1;
  align-items: center;
  justify-content: center;
}

.wb-ide {
  position: relative;
  display: flex;
  flex: 1;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
}

.wb-ide__outer,
.wb-ide__inner {
  flex: 1;
  min-width: 0;
  min-height: 0;
}

.wb-drawer-backdrop {
  position: absolute;
  inset: 0 0 0 44px;
  z-index: 30;
  background: rgb(15 23 42 / 38%);
}

.wb-drawer-panel {
  position: absolute;
  top: 0;
  bottom: 0;
  left: 44px;
  z-index: 31;
  width: min(300px, calc(100vw - 56px));
  background: var(--wb-surface);
  border-right: 1px solid var(--wb-divider);
}

.wb-workspace {
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  min-width: 0;
  min-height: 0;
}

.wb-request {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
  background: var(--wb-surface);
}

.wb-runbar {
  display: flex;
  flex-shrink: 0;
  gap: 8px;
  align-items: center;
  padding: 8px 12px;
  border-bottom: 1px solid var(--wb-divider-light);
}

.wb-urlbar {
  display: flex;
  flex: 1;
  align-items: center;
  min-width: 0;
  height: var(--wb-urlbar-h, 34px);
  overflow: hidden;
  background: var(--wb-surface-input);
  border: 1px solid var(--wb-divider);
  border-radius: var(--wb-radius);

  &:focus-within {
    background: var(--wb-surface);
    border-color: var(--wb-primary);
    box-shadow: 0 0 0 2px var(--wb-primary-soft);
  }

  &.is-error {
    border-color: var(--wb-danger);
  }
}

.wb-urlbar__method {
  display: inline-flex;
  flex-shrink: 0;
  height: 100%;

  :deep(.el-tooltip__trigger) {
    display: inline-flex;
    width: 6.25rem;
    height: 100%;
  }
}

.wb-runbar__actions {
  display: flex;
  flex-shrink: 0;
  align-items: center;
}

.wb-send-btn {
  display: inline-flex;
  align-items: stretch;
  height: var(--wb-urlbar-h, 34px);
  overflow: hidden;
  border-radius: var(--wb-radius);

  :deep(.el-tooltip__trigger) {
    display: inline-flex;
    height: 100%;
  }
}

.wb-send-btn__main,
.wb-send-btn__caret {
  display: inline-flex;
  gap: 6px;
  align-items: center;
  justify-content: center;
  height: 100%;
  font-size: 12.5px;
  font-weight: 600;
  color: var(--wb-primary-fg);
  cursor: pointer;
  background: var(--wb-primary);
  border: none;
  transition: background-color 140ms ease;

  &:hover {
    background: var(--wb-primary-hover);
  }

  &:active {
    background: var(--wb-primary-active);
  }
}

.wb-send-btn__main {
  min-width: 4.8rem;
  padding: 0 12px;
}

.wb-send-btn__caret {
  width: 28px;
  padding: 0;
  border-left: 1px solid color-mix(in srgb, var(--wb-primary-fg) 22%, transparent);
}

.wb-warn-bar {
  padding: 4px 12px;
  margin: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  font-size: 11px;
  color: var(--wb-warning);
  white-space: nowrap;
  background: color-mix(in srgb, var(--wb-warning) 8%, transparent);
  border-bottom: 1px solid var(--wb-divider-light);
}

.wb-req-subnav {
  display: flex;
  flex-shrink: 0;
  gap: 2px;
  align-items: stretch;
  height: 36px;
  padding: 0 8px;
  overflow-x: auto;
  background: var(--wb-surface-subtle);
  border-bottom: 1px solid var(--wb-divider);
}

.wb-req-subnav__btn {
  position: relative;
  display: inline-flex;
  flex-shrink: 0;
  align-items: center;
  padding: 0 10px;
  font-size: 12.5px;
  font-weight: 500;
  color: var(--wb-text-muted);
  cursor: pointer;
  background: transparent;
  border: none;

  &:hover {
    color: var(--wb-text);
  }

  &.is-active {
    font-weight: 600;
    color: var(--wb-primary);

    &::after {
      position: absolute;
      right: 6px;
      bottom: 0;
      left: 6px;
      height: 2px;
      content: "";
      background: var(--wb-primary);
    }
  }
}

.wb-req-body {
  flex: 1;
  min-height: 0;
  padding: 10px 12px;
  overflow: auto;
}

.wb-req-body__fill {
  height: 100%;
}
</style>
