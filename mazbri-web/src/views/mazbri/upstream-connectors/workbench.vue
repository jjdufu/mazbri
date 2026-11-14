<template>
  <div
    ref="pageEl"
    v-loading="bootstrapping"
    class="connector-workbench-page"
  >
    <header class="wb-top">
      <div class="wb-top__left">
        <ElButton
          size="default"
          text
          class="wb-top__back"
          :aria-label="$t('upstreamConnector.backToSupply')"
          @click="goBack"
        >
          <ArtSvgIcon icon="ri:arrow-left-line" class="mr-1" />
          {{ $t("upstreamConnector.backToSupply") }}
        </ElButton>
        <input
          v-if="draft"
          v-model="draft.name"
          type="text"
          class="wb-top__name-input"
          :readonly="!canWriteWorkbench"
          :aria-label="$t('upstreamConnector.fields.name')"
          :placeholder="$t('upstreamConnector.untitledName')"
        />
        <ElTag
          size="small"
          :type="isSupplierLinked ? 'warning' : 'info'"
          effect="plain"
        >
          {{
            isSupplierLinked
              ? $t("upstreamConnector.modeEditSupplierCode", {
                  code: publishForm.supplier_code || "—",
                })
              : $t("upstreamConnector.modeNewDraft")
          }}
        </ElTag>
        <ElTag
          v-if="!canWriteWorkbench"
          size="small"
          type="info"
          effect="plain"
        >
          {{ $t("upstreamConnector.readOnlyHint") }}
        </ElTag>
        <ElTag
          v-if="saveBadge"
          size="small"
          :type="saveBadge.type"
          effect="plain"
        >
          {{ saveBadge.text }}
        </ElTag>
      </div>

      <div v-if="draft" class="wb-top__right">
        <ElButton size="default" @click="envDrawerVisible = true">
          <svg
            class="mr-1"
            width="14"
            height="14"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            <path
              d="M8 3H7a2 2 0 0 0-2 2v5a2 2 0 0 1-2 2 2 2 0 0 1 2 2v5c0 1.1.9 2 2 2h1M16 21h1a2 2 0 0 0 2-2v-5c0-1.1.9-2 2-2a2 2 0 0 1-2-2V5a2 2 0 0 0-2-2h-1"
            />
          </svg>
          {{ $t("upstreamConnector.envDrawerBtn", { n: envCount }) }}
        </ElButton>
        <template v-if="canWriteWorkbench">
          <HoppMenu placement="bottom-end" :append-to="pageMount">
            <template #trigger>
              <ElButton size="default">
                {{ $t("upstreamConnector.importExport") }}
                <ArtSvgIcon icon="ri:arrow-down-s-line" class="ml-1" />
              </ElButton>
            </template>
            <button
              type="button"
              class="hopp-menu-item"
              @click="showImport = true"
            >
              {{ $t("upstreamConnector.import") }}
            </button>
            <button
              type="button"
              class="hopp-menu-item"
              @click="doExport()"
            >
              {{ $t("upstreamConnector.exportPostman") }}
            </button>
            <button
              v-if="draft.linked_supplier_id"
              type="button"
              class="hopp-menu-item is-divided"
              @click="resyncFromLiveSupplier()"
            >
              {{ $t("upstreamConnector.resyncFromLive") }}
            </button>
          </HoppMenu>
          <ElButton
            size="default"
            :loading="saving"
            title="Ctrl/⌘+S"
            @click="saveDraftExplicit()"
          >
            <ArtSvgIcon icon="ri:save-line" class="mr-1" />
            {{ $t("upstreamConnector.saveDraft") }}
          </ElButton>
          <span
            class="wb-top__gate-pill"
            :class="{ 'is-ready': gate.total > 0 && gate.ok === gate.total }"
            :title="$t('upstreamConnector.gateChecklist')"
          >
            {{ $t("upstreamConnector.gateShort") }} {{ gate.ok }}/{{ gate.total }}
          </span>
          <ElButton
            type="primary"
            size="default"
            :loading="publishing"
            :title="
              canPublish
                ? $t('upstreamConnector.publish')
                : $t('upstreamConnector.publishNeedRun')
            "
            @click="openPublish()"
          >
            <ArtSvgIcon icon="ri:upload-cloud-2-line" class="mr-1" />
            {{ $t("upstreamConnector.publish") }}
          </ElButton>
        </template>
      </div>
    </header>

    <div v-if="!draft && !bootstrapping" class="wb-empty">
      <ElEmpty :description="$t('upstreamConnector.emptyHint')">
        <ElButton
          v-if="canWriteWorkbench"
          type="primary"
          @click="createBlankSession()"
        >
          {{ $t("upstreamConnector.newSession") }}
        </ElButton>
      </ElEmpty>
    </div>

    <template v-else-if="draft">
      <WorkbenchSplit>
        <template #left>
          <div class="wb-pane">
            <WorkbenchEndpointList
              v-model:session-name="draft.name"
              :endpoints="endpoints"
              :active-index="activeEpIdx"
              :gate="gate"
              :gate-items="gate.items"
              :runs="runs"
              :active-run-id="lastRun?.id"
              :read-only="!canWriteWorkbench"
              @add="addEndpoint"
              @select="activeEpIdx = $event"
              @toggle-gate="toggleGate"
              @action="onEpAction"
              @session-action="onSessionAction"
              @reorder="onEpReorder($event.from, $event.to)"
              @select-run="lastRun = $event"
            />
          </div>
        </template>

        <template #center>
          <div class="wb-main">
            <WorkbenchSplitLayout
              ref="vsplitRef"
              v-model:direction="splitDirection"
              :min-bottom="220"
              :min-right="360"
            >
              <template #top>
                <div v-if="activeEp" class="wb-request">
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
                        :env="envMap"
                        :placeholder="$t('upstreamConnector.fields.upstreamUrl')"
                        @focus="urlFocused = true"
                        @update:model-value="onUrlInput"
                        @blur="onUrlBlur"
                        @enter="onSendClick"
                        @curl="applyCurlPaste"
                      />
                    </div>
                    <WorkbenchAdapterChip
                      :adapter="activeEp.adapter || 'http_json'"
                      :encoding="resolvedBodyEncoding(activeEp)"
                      :append-to="pageMount"
                      :disabled="!canWriteWorkbench"
                      @select="onAdapterQuickSelect"
                      @edit="reqTab = 'transform'"
                    />
                    <button
                      type="button"
                      class="wb-split-dir-btn"
                      :disabled="vsplitRef?.canSideBySide === false"
                      :title="splitToggleTitle"
                      :aria-label="splitToggleTitle"
                      @click="vsplitRef?.toggleDirection()"
                    >
                      <svg
                        v-if="
                          (vsplitRef?.effectiveDirection || splitDirection) ===
                          'vertical'
                        "
                        width="14"
                        height="14"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="1.8"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        aria-hidden="true"
                      >
                        <rect x="3" y="3" width="18" height="18" rx="2" />
                        <line x1="3" y1="12" x2="21" y2="12" />
                      </svg>
                      <svg
                        v-else
                        width="14"
                        height="14"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="1.8"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                        aria-hidden="true"
                      >
                        <rect x="3" y="3" width="18" height="18" rx="2" />
                        <line x1="12" y1="3" x2="12" y2="21" />
                      </svg>
                    </button>
                    <div v-if="canWriteWorkbench" class="wb-runbar__actions">
                      <div class="hopp-btn hopp-btn--primary">
                        <ElTooltip
                          :content="$t('upstreamConnector.sendProbeTooltip')"
                          placement="bottom"
                          :show-after="300"
                        >
                          <button
                            type="button"
                            class="hopp-btn__main"
                            :aria-label="
                              running
                                ? $t('upstreamConnector.sendCancel')
                                : $t('upstreamConnector.runBusiness')
                            "
                            @click="onSendClick"
                          >
                            <ArtSvgIcon v-if="running" icon="ri:close-line" />
                            {{
                              running
                                ? $t("upstreamConnector.sendCancel")
                                : $t("upstreamConnector.send")
                            }}
                          </button>
                        </ElTooltip>
                        <HoppMenu
                          placement="bottom-end"
                          :append-to="pageMount"
                        >
                          <template #trigger>
                            <button
                              type="button"
                              class="hopp-btn__caret"
                              :aria-label="$t('upstreamConnector.sessionActions')"
                            >
                              <span class="hopp-menu-caret" />
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
                  <p v-if="unresolved.length" class="wb-unresolved">
                    {{
                      $t("upstreamConnector.unresolvedVars", {
                        vars: unresolved.join(", "),
                      })
                    }}
                  </p>

                  <div class="req-tabs-shell">
                  <ElTabs v-model="reqTab" class="req-tabs">
                    <ElTabPane name="params">
                      <template #label>
                        <WorkbenchTabLabel
                          :label="$t('upstreamConnector.paramsTab')"
                          :count="queryCount"
                          :active="reqTab === 'params'"
                        />
                      </template>
                      <div :inert="!canWriteWorkbench || undefined">
                        <WorkbenchKvRows
                          v-model="queryRows"
                          :title="$t('upstreamConnector.queryList')"
                          :add-title="$t('upstreamConnector.addQuery')"
                          :clear-title="$t('upstreamConnector.clearKv')"
                          :env="envMap"
                        />
                      </div>
                    </ElTabPane>
                    <ElTabPane name="body">
                      <template #label>
                        <WorkbenchTabLabel
                          :label="$t('upstreamConnector.bodyTab')"
                          :dot="bodyFilled"
                          :active="reqTab === 'body'"
                        />
                      </template>
                      <div :inert="!canWriteWorkbench || undefined">
                        <WorkbenchBodyPane
                          v-model:gql-var-rows="gqlVarRows"
                          :endpoint="activeEp"
                          :env="envMap"
                          :uploading-artifact="uploadingArtifact"
                          :append-to="pageMount"
                          @pick-artifact="artifactInput?.click()"
                        />
                      </div>
                    </ElTabPane>
                    <ElTabPane name="headers">
                      <template #label>
                        <WorkbenchTabLabel
                          :label="$t('upstreamConnector.headersTab')"
                          :count="headerCount"
                          :active="reqTab === 'headers'"
                        />
                      </template>
                      <div :inert="!canWriteWorkbench || undefined">
                        <WorkbenchKvRows
                          v-model="headerRows"
                          variant="headers"
                          :title="$t('upstreamConnector.headersList')"
                          :add-title="$t('upstreamConnector.addHeader')"
                          :clear-title="$t('upstreamConnector.clearKv')"
                          :env="envMap"
                          :append-to="pageMount"
                        />
                      </div>
                    </ElTabPane>
                    <ElTabPane name="auth">
                      <template #label>
                        <WorkbenchTabLabel
                          :label="$t('upstreamConnector.authTab')"
                          :dot="authFilled"
                          :active="reqTab === 'auth'"
                        />
                      </template>
                      <div :inert="!canWriteWorkbench || undefined">
                        <WorkbenchAuthPicker
                          :profile="String(activeEp.auth?.profile || 'none')"
                          :flow="configForm.auth_flow"
                          :adapter="String(activeEp.adapter || 'http_json')"
                          :invalidation-source="activeInvalidationSource"
                          :env="envMap"
                          :seed-json="authSeedJson"
                          :append-to="pageMount"
                          :running="running"
                          @update:profile="setEpAuthProfile"
                          @run="doRun('auth_only')"
                        />
                      </div>
                    </ElTabPane>
                    <ElTabPane name="transform">
                      <template #label>
                        <WorkbenchTabLabel
                          :label="$t('upstreamConnector.transformTab')"
                          :dot="transformFilled"
                          :active="reqTab === 'transform'"
                        />
                      </template>
                      <div class="wb-sheet">
                        <div :inert="!canWriteWorkbench || undefined">
                        <WorkbenchGhostField
                          :label="$t('upstreamConnector.adapter')"
                          kind="select"
                          :model-value="activeEp.adapter || 'http_json'"
                          :options="adapterSelectOptions"
                          :append-to="pageMount"
                          @update:model-value="activeEp.adapter = String($event)"
                        />
                        <WorkbenchGhostField
                          v-if="activeEp.adapter && activeEp.adapter !== 'http_json'"
                          :label="$t('upstreamConnector.operation')"
                          :model-value="activeEp.operation || ''"
                          :placeholder="$t('upstreamConnector.operationHint')"
                          @update:model-value="activeEp.operation = String($event)"
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
                          @update:model-value="activeEp.content_type = String($event)"
                        />
                        <WorkbenchGhostField
                          v-if="activeEp.adapter === 'http_json'"
                          :label="$t('upstreamConnector.egressBodyEncoding')"
                          kind="select"
                          :model-value="resolvedBodyEncoding(activeEp)"
                          :options="encodingSelectOptions"
                          :append-to="pageMount"
                          @update:model-value="onBodyEncodingChange(String($event))"
                        />
                        <div class="wb-sheet-row is-block">
                          <div class="wb-sheet-row__label">{{
                            $t("upstreamConnector.cryptoSection")
                          }}</div>
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
                        :env="envMap"
                      />
                        </div>
                      <button
                        type="button"
                        class="wb-sheet-row is-toggle is-head"
                          @click="xfPipeOpen = !xfPipeOpen"
                        >
                          <span class="wb-sheet-row__label">{{
                            $t("upstreamConnector.pipelineSection")
                          }}</span>
                          <span class="wb-sheet-row__control">
                            <span class="wb-sheet-row__summary">{{
                              lastRun
                                ? $t("upstreamConnector.sectionConfigured")
                                : $t("upstreamConnector.sectionUnconfigured")
                            }}</span>
                          </span>
                        </button>
                        <div v-show="xfPipeOpen" class="wb-sheet-body">
                          <WorkbenchPipeline
                            :run="lastRun"
                            :inbound="{
                              method: activeEp.method,
                              path: activeEp.path,
                              body: activeEp.body,
                            }"
                          />
                        </div>
                      </div>
                    </ElTabPane>
                    <ElTabPane name="assert">
                      <template #label>
                        <WorkbenchTabLabel
                          :label="$t('upstreamConnector.assertTab')"
                          :dot="assertFilled"
                          :active="reqTab === 'assert'"
                        />
                      </template>
                      <div :inert="!canWriteWorkbench || undefined">
                        <WorkbenchAssertEditor
                          v-model="activeAssertions"
                          :append-to="pageMount"
                        />
                      </div>
                    </ElTabPane>
                  </ElTabs>
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
              </template>
              <template #bottom>
                <WorkbenchResponsePane
                  :run="lastRun"
                  :runs="runs"
                  :send-block-reason="urlMissing ? 'url' : ''"
                  :loading="running"
                  :adapter="activeEp?.adapter"
                  @select-run="lastRun = $event"
                  @open-shortcuts="showShortcuts = true"
                />
              </template>
            </WorkbenchSplitLayout>
          </div>
        </template>
      </WorkbenchSplit>
    </template>

    <AdaptiveDrawer
      v-model="envDrawerVisible"
      :title="$t('upstreamConnector.envVars')"
      size="520px"
    >
      <p class="wb-env-drawer__hint">{{ $t("upstreamConnector.envVarsHint") }}</p>
      <div :inert="!canWriteWorkbench || undefined">
        <WorkbenchKvRows
          v-model="envRows"
          :title="$t('upstreamConnector.envVars')"
          :add-title="$t('upstreamConnector.addEnv')"
          :clear-title="$t('upstreamConnector.clearKv')"
        />
      </div>
    </AdaptiveDrawer>

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
import { watchDebounced } from "@vueuse/core";
import { saveAs } from "file-saver";
import { ElMessage, ElMessageBox } from "element-plus";
import { $t } from "@/locales";
import { type SupplierConfigForm } from "@/views/mazbri/suppliers/components/SupplierTransparentConfig.vue";
import {
  createEmptyAuthFlow,
  serializeAuthFlow,
  resolveUpstreamType,
} from "@/views/mazbri/suppliers/supplier-form";
import {
  type AssertionSpec,
  type ConnectorDraft,
  type ConnectorEndpoint,
  type ConnectorRun,
  type PublishResult,
  createDraft,
  draftFromSupplier,
  exportPostman,
  getDraft,
  importCurl,
  importOpenApi,
  importPostman,
  listDrafts,
  listRuns,
  liveRun,
  publishDraft,
  runDraft,
  transformPreview,
  updateDraft,
  uploadArtifact,
} from "@/api/upstream-connectors";
import { fetchSupplierById } from "@/api/supplier";
import { useUserStore } from "@/store/modules/user";
import { canWriteUpstreamConnector } from "@/utils/ops/roles";
import { copyWithFeedback } from "@/utils/ui/copy-feedback";
import { isHttpError } from "@/utils/http/error";
import {
  AUTH_PROFILES,
  CODE_FREE_UPSTREAM_TYPE,
  EGRESS_ADAPTERS,
  WRAP_OPTIONS,
  WRITE_METHODS,
  adapterBodyFilled,
  applyEnv,
  asCodeFreeDraft,
  autoChannelsFromEndpoints,
  buildCurl,
  gateViewFromServer,
  leftoverTokenHeaderNames,
  envFromRows,
  ensureEndpointAdapterFields,
  fileToBase64,
  findUnresolvedVars,
  workbenchEnvSurfaceParts,
  genSessionCode,
  isPersistedDraftId,
  blankWorkbenchEndpoint,
  isMicroserviceUpstream,
  defaultBodyContentType,
  resolvedBodyEncoding,
  setBodyEncoding,
  entriesToKv,
  isLegalHttpMethod,
  kvEnabledCount,
  kvToEntries,
  kvToMap,
  workbenchEnvToKv,
  lastAuthJson,
  mapToKv,
  mergeQueryFromMap,
  mergeImportedEndpoints,
  normalizeWorkbenchEndpoint,
  parseCurl,
  sanitizeAuthSpec,
  parseOpenApiDocument,
  parsePostmanCollection,
  recordFromUnknown,
  composeRequestUrl,
  trySplitRequestUrl,
  type ChannelDraft,
  type KvRow,
  type ParsedImport,
  type SplitDirection,
} from "./workbenchHelpers";
import { isSessionHttpAuthActive } from "./authMatrix";
import { invalidationFromApi, invalidationSource } from "./authInvalidation";
import AdaptiveDrawer from "@/components/common/AdaptiveDrawer.vue";
import CryptoProfileEditor from "./CryptoProfileEditor.vue";
import {
  parseCryptoProfile,
  validateCryptoProfileClient,
} from "./cryptoProfileHelpers";
import WorkbenchSplit from "./WorkbenchSplit.vue";
import WorkbenchSplitLayout from "./WorkbenchSplitLayout.vue";
import WorkbenchEndpointList from "./WorkbenchEndpointList.vue";
import WorkbenchKvRows from "./WorkbenchKvRows.vue";
import WorkbenchAssertEditor from "./WorkbenchAssertEditor.vue";
import HoppMenu from "@/components/common/hopp-menu/index.vue";
import WorkbenchMethodSelect from "./hopp/WorkbenchMethodSelect.vue";
import WorkbenchGhostField from "./hopp/WorkbenchGhostField.vue";
import WorkbenchAuthPicker from "./hopp/WorkbenchAuthPicker.vue";
import WorkbenchBodyPane from "./hopp/WorkbenchBodyPane.vue";
import WorkbenchUrlField from "./hopp/WorkbenchUrlField.vue";
import WorkbenchAdapterChip from "./hopp/WorkbenchAdapterChip.vue";
import WorkbenchCommandPalette from "./hopp/WorkbenchCommandPalette.vue";
import WorkbenchShortcutsPanel from "./hopp/WorkbenchShortcutsPanel.vue";
import WorkbenchTabLabel from "./hopp/WorkbenchTabLabel.vue";
import WorkbenchPipeline from "./WorkbenchPipeline.vue";
import WorkbenchResponsePane from "./WorkbenchResponsePane.vue";
import WorkbenchImportDialog from "./WorkbenchImportDialog.vue";
import WorkbenchPublishDrawer from "./WorkbenchPublishDrawer.vue";

defineOptions({ name: "UpstreamConnectorWorkbench" });

const route = useRoute();
const router = useRouter();

const WORKBENCH_PATH = "/operations/upstream-supply/workbench";

const draft = ref<ConnectorDraft | null>(null);
const endpoints = ref<ConnectorEndpoint[]>([]);
const activeEpIdx = ref(0);
const runs = ref<ConnectorRun[]>([]);
const lastRun = ref<ConnectorRun | null>(null);

function isSuccessfulSend(r: ConnectorRun | null | undefined): boolean {
  return Boolean(r?.ok) && r?.mode !== "transform_preview";
}

const canPublish = computed(
  () => runs.value.some((r) => isSuccessfulSend(r)) || isSuccessfulSend(lastRun.value),
);
const reqTab = ref("params");
const headerRows = ref<KvRow[]>([]);
const queryRows = ref<KvRow[]>([]);
const envRows = ref<KvRow[]>([]);
const gqlVarRows = ref<KvRow[]>([]);
const gqlEnumRows = ref<KvRow[]>([]);
const artifactInput = ref<HTMLInputElement | null>(null);
const uploadingArtifact = ref(false);
const urlDraft = ref("");
const urlFocused = ref(false);
const urlFieldRef = ref<{ blur: () => void; focus: () => void } | null>(null);
const urlMissing = ref(false);
const showCommands = ref(false);
const showShortcuts = ref(false);
const bootstrapping = ref(true);
const dirty = ref(false);
const suppressDirty = ref(false);
const editGen = ref(0);
const saveError = ref(false);
const lastSavedAt = ref<Date | null>(null);

const saving = ref(false);
const leavePromptOpen = ref(false);
const running = ref(false);
const runAbort = ref<AbortController | null>(null);
let runGen = 0;
const RUN_HTTP = { showErrorMessage: false as const, timeout: 120_000 };

const importing = ref(false);
const publishing = ref(false);

const showImport = ref(false);
const showPublish = ref(false);
const envDrawerVisible = ref(false);
const splitDirection = ref<SplitDirection>("horizontal");
const importTab = ref("curl");
const importMode = ref("merge");
const importCurlText = ref("");
const importJsonText = ref("");

const isEditMode = computed(
  () =>
    typeof route.query.supplier_id === "string" &&
    route.query.supplier_id.length > 0,
);

const isSupplierLinked = computed(() =>
  Boolean(draft.value?.linked_supplier_id || isEditMode.value),
);

const saveBadge = computed(() => {
  if (saving.value) {
    return { type: "info" as const, text: $t("upstreamConnector.savingDraft") };
  }
  if (saveError.value) {
    return {
      type: "danger" as const,
      text: $t("upstreamConnector.saveDraftFailed"),
    };
  }
  if (dirty.value) {
    return { type: "warning" as const, text: $t("upstreamConnector.unsaved") };
  }
  if (lastSavedAt.value && isPersistedDraftId(draft.value?.id)) {
    return { type: "success" as const, text: $t("upstreamConnector.saved") };
  }
  return null;
});

const publishForm = ref({
  mode: "supplier_channels" as "supplier_only" | "supplier_channels" | "full_onboarding",
  supplier_code: "",
  supplier_name: "",
  product_id: "",
  require_gate: true,
  force_publish: false,
  force_reason: "",
  comment: "",
});
const publishChannels = ref<ChannelDraft[]>([]);
const publishResult = ref<PublishResult | null>(null);

const pageEl = ref<HTMLElement | null>(null);
const pageMount = computed(() => pageEl.value ?? undefined);

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
const xfPipeUser = ref<boolean | null>(null);

const xfPipeOpen = computed({
  get: () => (xfPipeUser.value != null ? xfPipeUser.value : Boolean(lastRun.value)),
  set: (v: boolean) => {
    xfPipeUser.value = v;
  },
});

const activeEp = computed(() => endpoints.value[activeEpIdx.value] || null);

/** 当前接口生效的认证失效规则来源：渠道 egress.auth.invalidation > 供应商 auth_flow.invalidation > 默认。 */
const activeInvalidationSource = computed(() =>
  invalidationSource(
    (activeEp.value?.auth as Record<string, unknown> | null | undefined) ?? null,
    configForm.value.auth_flow,
  ),
);

const activeAssertions = computed<AssertionSpec[]>({
  get: () => activeEp.value?.assertions || [],
  set: (v) => {
    if (activeEp.value) activeEp.value.assertions = v;
  },
});

const configForm = ref<SupplierConfigForm>({
  upstream_type: CODE_FREE_UPSTREAM_TYPE,
  auth_flow: createEmptyAuthFlow(),
});

const userStore = useUserStore();
const isSuperAdmin = computed(() => userStore.isAdmin);
/** 工作台写权限：业务管理角色（R_ADMIN / R_FINANCE）；观察者（R_VIEWER）只读。 */
const canWriteWorkbench = computed(() =>
  canWriteUpstreamConnector(userStore.info?.role),
);
/** 强制发布与工作台写权限同界（R_ADMIN / R_FINANCE）。 */
const canForcePublish = computed(() => canWriteWorkbench.value);

const sessionCredentialKeys = computed(() =>
  configForm.value.auth_flow.credentials
    .map((item) => item.key.trim())
    .filter(Boolean),
);

const gate = computed(() =>
  gateViewFromServer(endpoints.value, draft.value?.gate),
);

const leftoverTokens = computed(() =>
  leftoverTokenHeaderNames({
    endpoints: endpoints.value,
    authEnabled: isSessionHttpAuthActive(
      configForm.value.auth_flow.enabled,
      configForm.value.auth_flow.profile,
    ),
    injectTo: configForm.value.auth_flow.inject_to,
    injectKey: configForm.value.auth_flow.inject_key,
  }),
);

const authSeedJson = computed(() => lastAuthJson(lastRun.value));

const envMap = computed(() => envFromRows(envRows.value));

const queryCount = computed(() => kvEnabledCount(queryRows.value));
const headerCount = computed(() => kvEnabledCount(headerRows.value));
const envCount = computed(() => kvEnabledCount(envRows.value));
const bodyFilled = computed(() =>
  activeEp.value ? adapterBodyFilled(activeEp.value) : false,
);
const authFilled = computed(() => {
  if (
    isSessionHttpAuthActive(
      configForm.value.auth_flow.enabled,
      configForm.value.auth_flow.profile,
    )
  ) {
    return true;
  }
  const profile = String(activeEp.value?.auth?.profile || "none");
  return (AUTH_PROFILES as readonly string[]).includes(profile) && profile !== "none";
});
const transformFilled = computed(() => {
  const ep = activeEp.value;
  if (!ep) return false;
  if ((ep.adapter || "http_json") !== "http_json") return true;
  if ((ep.operation || "").trim()) return true;
  if ((ep.unwrap || "").trim()) return true;
  const wrap = (ep.wrap || "").trim();
  if (Boolean(wrap) && wrap !== "none") return true;
  return Boolean(ep.crypto && typeof ep.crypto === "object");
});
const assertFilled = computed(() =>
  (activeEp.value?.assertions || []).some((a) => (a.type || "").trim()),
);

function unresolvedOn(ep: ConnectorEndpoint | null | undefined): string[] {
  if (!ep) return [];
  return findUnresolvedVars(
    workbenchEnvSurfaceParts({
      upstreamUrl: draft.value?.upstream_url,
      endpoint: ep,
      authFlow: configForm.value.auth_flow,
    }),
    envMap.value,
  );
}

const unresolved = computed(() => unresolvedOn(activeEp.value));

const unresolvedAll = computed(() => {
  const names = new Set<string>();
  for (const ep of endpoints.value) {
    for (const n of unresolvedOn(ep)) names.add(n);
  }
  if (!endpoints.value.length) {
    for (const n of unresolvedOn({
      method: "GET",
      path: "/",
    })) {
      names.add(n);
    }
  }
  return [...names];
});

function apiConfigToForm(cfg: Record<string, unknown>): SupplierConfigForm {
  const auth = (cfg.auth_flow || {}) as Record<string, unknown>;
  const creds = (auth.credentials || {}) as Record<string, string>;
  const credsList = Object.entries(creds).map(([key, value]) => ({
    key,
    value: String(value ?? ""),
  }));
  const rawProfile = String(auth.profile || "");
  const httpActive = isSessionHttpAuthActive(Boolean(auth.enabled), rawProfile);
  return {
    upstream_type: CODE_FREE_UPSTREAM_TYPE,
    auth_flow: {
      ...createEmptyAuthFlow(),
      enabled: httpActive,
      profile: rawProfile,
      token_url: String(auth.token_url || ""),
      token_path: String(auth.token_path || ""),
      method: String(auth.method || "POST"),
      headers: mapToKv(auth.headers as Record<string, string> | undefined),
      request_body_tpl: String(auth.request_body_tpl || ""),
      credentials: credsList.length ? credsList : createEmptyAuthFlow().credentials,
      token_jsonpath: String(auth.token_jsonpath || "$.token"),
      expiry_jsonpath: String(auth.expiry_jsonpath || "$.expires_in"),
      inject_to: String(auth.inject_to || "header"),
      inject_key: String(auth.inject_key || "Authorization"),
      inject_value_format: String(auth.inject_value_format || "Bearer {}"),
      jwt: { ...createEmptyAuthFlow().jwt, ...((auth.jwt as object) || {}) },
      digest: { ...createEmptyAuthFlow().digest, ...((auth.digest as object) || {}) },
      aws: { ...createEmptyAuthFlow().aws, ...((auth.aws as object) || {}) },
      hawk: { ...createEmptyAuthFlow().hawk, ...((auth.hawk as object) || {}) },
      invalidation: invalidationFromApi(auth.invalidation),
    },
  };
}

function formToApiConfig(form: SupplierConfigForm): Record<string, unknown> {
  const credsMap: Record<string, string> = {};
  form.auth_flow.credentials.forEach((item) => {
    if (item.key.trim()) credsMap[item.key.trim()] = item.value;
  });
  const hasCreds = Object.keys(credsMap).length > 0;
  const httpActive = isSessionHttpAuthActive(
    form.auth_flow.enabled,
    form.auth_flow.profile,
  );
  const payload: Record<string, unknown> = {
    upstream_type: CODE_FREE_UPSTREAM_TYPE,
    auth_flow: httpActive
      ? serializeAuthFlow(form.auth_flow, credsMap)
      : hasCreds
        ? { enabled: false, profile: "none", credentials: credsMap }
        : null,
  };
  const timeouts = (draft.value?.config as { timeouts?: unknown })?.timeouts;
  if (timeouts) payload.timeouts = timeouts;
  return payload;
}

function syncKvFromActive() {
  const ep = activeEp.value;
  headerRows.value = entriesToKv(ep?.header_entries, ep?.headers || {});
  queryRows.value = entriesToKv(ep?.query_entries, ep?.query || {});
  gqlVarRows.value = mapToKv(recordFromUnknown(ep?.graphql?.variables_map));
  gqlEnumRows.value = mapToKv(recordFromUnknown(ep?.graphql?.enum_map));
}

function flushKvToActive() {
  if (!activeEp.value) return;
  activeEp.value.header_entries = headerRows.value.map((r) => ({
    key: r.key,
    value: r.value,
    desc: r.desc ?? "",
    enabled: r.enabled !== false,
  }));
  activeEp.value.query_entries = queryRows.value.map((r) => ({
    key: r.key,
    value: r.value,
    desc: r.desc ?? "",
    enabled: r.enabled !== false,
  }));
  activeEp.value.headers = kvToMap(headerRows.value);
  activeEp.value.query = kvToMap(queryRows.value);
  if (activeEp.value.adapter === "graphql") {
    ensureEndpointAdapterFields(activeEp.value);
    activeEp.value.graphql = {
      ...(activeEp.value.graphql || {}),
      variables_map: kvToMap(gqlVarRows.value),
      enum_map: kvToMap(gqlEnumRows.value),
    };
  }
}

const publishResultDesc = computed(() => {
  if (!publishResult.value) return "";
  const r = publishResult.value;
  return `supplier=${r.supplier_id} version=v${r.version_no} route_version=${r.route_version ?? "—"}`;
});

async function syncSessionRoute(d: ConnectorDraft) {
  if (d.linked_supplier_id) {
    if (route.query.supplier_id === d.linked_supplier_id && !route.query.session) {
      return;
    }
    await router.replace({
      path: WORKBENCH_PATH,
      query: { supplier_id: d.linked_supplier_id },
    });
    return;
  }
  if (!isPersistedDraftId(d.id)) {
    if (!route.query.session && !route.query.supplier_id) return;
    await router.replace({ path: WORKBENCH_PATH });
    return;
  }
  if (route.query.session === d.id && !route.query.supplier_id) return;
  await router.replace({
    path: WORKBENCH_PATH,
    query: { session: d.id },
  });
}

async function loadReadOnlySupplierDraft(supplierId: string) {
  const res = await listDrafts({ page: 1, page_size: 100 });
  const matched = (res.drafts || []).find(
    (item) => item.linked_supplier_id === supplierId && item.status !== "archived",
  );
  if (matched) {
    const full = await getDraft(matched.id);
    await openSession(full);
    return;
  }
  draft.value = null;
}

function markDirty() {
  if (!suppressDirty.value) {
    dirty.value = true;
    editGen.value += 1;
  }
}

function leaveForMicroservice(supplierId?: string | null) {
  ElMessage.warning($t("upstreamConnector.transparentOnly"));
  if (supplierId) {
    void router.replace({ name: "SupplierDetail", params: { id: supplierId } });
    return;
  }
  void router.replace({ path: "/operations/upstream-supply", query: { tab: "suppliers" } });
}

async function openSession(d: ConnectorDraft): Promise<boolean> {
  if (isMicroserviceUpstream(d) && d.linked_supplier_id) {
    draft.value = null;
    leaveForMicroservice(d.linked_supplier_id);
    return false;
  }
  const coerced = isMicroserviceUpstream(d);
  if (coerced) {
    d = asCodeFreeDraft(d);
  }
  suppressDirty.value = true;
  draft.value = {
    ...d,
    upstream_type: CODE_FREE_UPSTREAM_TYPE,
    endpoints: Array.isArray(d.endpoints) ? d.endpoints : [],
    config: { ...(d.config || {}), upstream_type: CODE_FREE_UPSTREAM_TYPE },
  };
  configForm.value = apiConfigToForm(
    (draft.value.config || {}) as Record<string, unknown>,
  );
  endpoints.value = (draft.value.endpoints || []).map((ep, i) =>
    normalizeWorkbenchEndpoint(ep, i),
  );
  activeEpIdx.value = 0;
  if (d.linked_supplier_id) {
    try {
      const sup = await fetchSupplierById(d.linked_supplier_id);
      publishForm.value.supplier_code = sup.supplier_code;
      publishForm.value.supplier_name = sup.name;
      if (draft.value) draft.value.name = sup.name;
    } catch {
      publishForm.value.supplier_code = d.draft_code
        .replace(/^from-/i, "")
        .replace(/-[a-f0-9]{6}$/i, "")
        .toUpperCase();
      publishForm.value.supplier_name = d.name;
    }
  } else {
    publishForm.value.supplier_code =
      d.name
        .replace(/[^A-Za-z0-9_-]/g, "-")
        .replace(/-+/g, "-")
        .replace(/^-|-$/g, "")
        .toUpperCase()
        .slice(0, 32) || genSessionCode().toUpperCase();
    publishForm.value.supplier_name = d.name;
  }
  publishResult.value = null;
  envRows.value = workbenchEnvToKv(d.workbench_env ?? []);
  syncKvFromActive();
  refreshUrlDraft();
  saveError.value = false;
  lastSavedAt.value = isPersistedDraftId(d.id) ? new Date(d.updated_at) : null;
  if (isPersistedDraftId(d.id)) {
    try {
      runs.value = (await listRuns(d.id)) || [];
      lastRun.value = runs.value[0] || null;
    } catch {
      runs.value = [];
      lastRun.value = null;
    }
  } else {
    runs.value = [];
    lastRun.value = null;
  }
  await syncSessionRoute(d);
  await nextTick();
  dirty.value = coerced;
  if (coerced) editGen.value += 1;
  suppressDirty.value = false;
  if (!coerced) {
    await nextTick();
    dirty.value = false;
  }
  return true;
}

watch(activeEpIdx, async () => {
  suppressDirty.value = true;
  flushKvToActive();
  syncKvFromActive();
  refreshUrlDraft();
  await nextTick();
  suppressDirty.value = false;
});

watch(
  () => activeEp.value?.adapter,
  () => {
    if (!activeEp.value) return;
    ensureEndpointAdapterFields(activeEp.value);
    syncKvFromActive();
  },
);

watch(
  [() => draft.value?.upstream_url, () => activeEp.value?.path, queryRows],
  () => refreshUrlDraft(),
  { deep: true },
);

watch(
  [
    endpoints,
    () => draft.value?.name,
    () => draft.value?.upstream_url,
    configForm,
    headerRows,
    queryRows,
    envRows,
    gqlVarRows,
    gqlEnumRows,
  ],
  () => markDirty(),
  { deep: true },
);

function toggleGate(index: number) {
  const ep = endpoints.value[index];
  if (!ep) return;
  ep.gate = ep.gate === false;
}

function setEpAuthProfile(profile: string) {
  const ep = activeEp.value;
  if (!ep) return;
  const next = profile === "inherit" ? "none" : profile;
  ep.auth = { ...(ep.auth || {}), profile: next };
}


function onCryptoProfileChange(value: Record<string, unknown> | null) {
  if (!activeEp.value) return;
  activeEp.value.crypto = value;
}

function cryptoIssuesOnEndpoints(): string[] {
  const msgs: string[] = [];
  for (const ep of endpoints.value) {
    if (!ep.crypto) continue;
    const issues = validateCryptoProfileClient(parseCryptoProfile(ep.crypto));
    for (const issue of issues) {
      msgs.push(`${ep.name || ep.path || ep.id || "endpoint"}: ${issue.message}`);
    }
  }
  return msgs;
}

function onBodyEncodingChange(value: string) {
  const ep = activeEp.value;
  if (!ep) return;
  const enc =
    value === "form" || value === "none" || value === "json" ? value : "json";
  setBodyEncoding(ep, enc);
}

function addEndpoint() {
  endpoints.value.push(
    normalizeWorkbenchEndpoint(
      {
        id: `ep-${Date.now().toString(36)}`,
        name: `endpoint-${endpoints.value.length + 1}`,
        method: "POST",
        path: "",
        adapter: "http_json",
        body_content_type: "json",
        body_encoding: "json",
        body: {},
        wrap: "method_biz_content",
        assertions: [],
        gate: true,
      },
      endpoints.value.length,
    ),
  );
  activeEpIdx.value = endpoints.value.length - 1;
}

async function onArtifactFile(ev: Event) {
  const input = ev.target as HTMLInputElement;
  const file = input.files?.[0];
  input.value = "";
  if (!file || !activeEp.value) return;
  uploadingArtifact.value = true;
  try {
    const content_base64 = await fileToBase64(file);
    const art = await uploadArtifact({
      artifact_type: "fds",
      name: file.name,
      content_base64,
    });
    activeEp.value.artifact_id = art.id;
    ElMessage.success($t("upstreamConnector.artifactUploaded"));
  } finally {
    uploadingArtifact.value = false;
  }
}

function onEpAction({ cmd, index }: { cmd: string; index: number }) {
  if (!canWriteWorkbench.value) return;
  if (cmd === "duplicate") {
    const src = endpoints.value[index];
    if (!src) return;
    endpoints.value.splice(index + 1, 0, {
      ...JSON.parse(JSON.stringify(src)),
      id: `ep-${Date.now().toString(36)}`,
      name: `${src.name || "endpoint"}-copy`,
    });
    activeEpIdx.value = index + 1;
    return;
  }
  if (cmd === "delete") {
    if (endpoints.value.length <= 1) {
      ElMessage.warning($t("upstreamConnector.keepOneEndpoint"));
      return;
    }
    endpoints.value.splice(index, 1);
    if (activeEpIdx.value >= endpoints.value.length) {
      activeEpIdx.value = endpoints.value.length - 1;
    }
  }
}

function onEpReorder(from: number, to: number) {
  if (from === to || from < 0 || to < 0) return;
  const list = endpoints.value;
  if (from >= list.length || to >= list.length) return;
  const activeId = list[activeEpIdx.value]?.id;
  const next = [...list];
  const [moved] = next.splice(from, 1);
  if (!moved) return;
  next.splice(to, 0, moved);
  endpoints.value = next;
  if (activeId) {
    const idx = next.findIndex((e) => e.id === activeId);
    if (idx >= 0) activeEpIdx.value = idx;
  }
}

function sessionHasPersistableContent(): boolean {
  const d = draft.value;
  if (!d) return false;
  const untitled = $t("upstreamConnector.untitledName");
  if (d.name.trim() && d.name.trim() !== untitled) return true;
  if (d.upstream_url.trim()) return true;
  if (envRows.value.some((row) => row.key.trim())) return true;
  if (endpoints.value.length > 1) return true;
  const ep = endpoints.value[0];
  if (!ep) return false;
  if ((ep.path || "").trim()) return true;
  if ((ep.name || "").trim() && !/^endpoint-\d+$/i.test(ep.name || "")) return true;
  if ((ep.method || "GET").toUpperCase() !== "GET") return true;
  return false;
}

function shouldGuardLeave(): boolean {
  if (!canWriteWorkbench.value) return false;
  if (!draft.value || bootstrapping.value) return false;
  if (!dirty.value) return false;
  return isPersistedDraftId(draft.value.id) || sessionHasPersistableContent();
}

async function saveDraft(opts?: {
  silent?: boolean;
  signal?: AbortSignal;
  skipRouteSync?: boolean;
}) {
  if (!canWriteWorkbench.value) return;
  if (!draft.value) return;
  saving.value = true;
  suppressDirty.value = true;
  try {
    flushKvToActive();
    for (const ep of endpoints.value) {
      if (ep.auth) {
        const next = sanitizeAuthSpec(ep.auth as Record<string, unknown>);
        ep.auth = next || { profile: "none" };
      }
    }
    const body = {
      name: draft.value.name,
      upstream_url: draft.value.upstream_url,
      upstream_type: CODE_FREE_UPSTREAM_TYPE,
      config: formToApiConfig(configForm.value),
      endpoints: endpoints.value,
      workbench_env: kvToEntries(envRows.value),
    };
    const reqOpts = {
      ...(opts?.signal ? { signal: opts.signal, ...RUN_HTTP } : {}),
      showErrorMessage: !opts?.silent,
    };
    const updated = isPersistedDraftId(draft.value.id)
      ? await updateDraft(draft.value.id, body, reqOpts)
      : await createDraft(
          {
            draft_code: draft.value.draft_code || genSessionCode(),
            ...body,
            source: "manual",
          },
          reqOpts,
        );
    draft.value = {
      ...draft.value,
      ...updated,
      endpoints: endpoints.value,
      upstream_type: CODE_FREE_UPSTREAM_TYPE,
    };
    dirty.value = false;
    saveError.value = false;
    lastSavedAt.value = new Date();
    await nextTick();
    if (!opts?.skipRouteSync) await syncSessionRoute(draft.value);
    if (!opts?.silent) ElMessage.success($t("common.feedback.saveSuccess"));
  } catch (err) {
    saveError.value = true;
    throw err;
  } finally {
    suppressDirty.value = false;
    saving.value = false;
  }
}

watchDebounced(
  editGen,
  async () => {
    if (!canWriteWorkbench.value) return;
    if (!dirty.value || bootstrapping.value || saving.value || leavePromptOpen.value || !draft.value) return;
    if (!isPersistedDraftId(draft.value.id) && !sessionHasPersistableContent()) {
      return;
    }
    try {
      await saveDraft({ silent: true });
    } catch {
      saveError.value = true;
    }
  },
  { debounce: 900 },
);

function appliedRequest() {
  const ep = activeEp.value;
  if (!ep) return null;
  flushKvToActive();
  const env = envMap.value;
  const path = applyEnv(ep.path || "/", env);
  const headers: Record<string, string> = {};
  Object.entries(ep.headers || {}).forEach(([k, v]) => {
    headers[applyEnv(k, env)] = applyEnv(String(v), env);
  });
  const query: Record<string, string> = {};
  Object.entries(ep.query || {}).forEach(([k, v]) => {
    query[applyEnv(k, env)] = applyEnv(String(v), env);
  });
  let body: unknown = ep.body;
  if (defaultBodyContentType(ep) === "none") {
    body = undefined;
  } else if (typeof body === "string") {
    body = applyEnv(body, env);
  } else if (body != null) {
    try {
      body = JSON.parse(applyEnv(JSON.stringify(body), env));
    } catch {
      void 0;
    }
  }
  return { method: ep.method || "GET", path, headers, query, body };
}

function isRunCancelled(err: unknown): boolean {
  return isHttpError(err) && err.message === $t("httpMsg.requestCancelled");
}

const vsplitRef = ref<{
  expand: () => void;
  toggle: () => void;
  toggleDirection: () => void;
  canSideBySide?: boolean;
  effectiveDirection?: SplitDirection;
} | null>(null);

const splitToggleTitle = computed(() =>
  (vsplitRef.value?.effectiveDirection || splitDirection.value) === "vertical"
    ? $t("upstreamConnector.splitTopBottom")
    : $t("upstreamConnector.splitSideBySide"),
);

function onAdapterQuickSelect(next: string) {
  if (!canWriteWorkbench.value || !activeEp.value) return;
  activeEp.value.adapter = next;
  ensureEndpointAdapterFields(activeEp.value);
  syncKvFromActive();
}

async function saveDraftExplicit() {
  if (!canWriteWorkbench.value || !draft.value) return;
  try {
    await saveDraft({ silent: false });
  } catch {
    void 0;
  }
}

async function resyncFromLiveSupplier() {
  if (!canWriteWorkbench.value || !draft.value?.linked_supplier_id) return;
  try {
    await ElMessageBox.confirm(
      $t("upstreamConnector.resyncFromLiveConfirm"),
      $t("upstreamConnector.resyncFromLive"),
      { type: "warning" },
    );
  } catch {
    return;
  }
  bootstrapping.value = true;
  try {
    const d = await draftFromSupplier(draft.value.linked_supplier_id, {
      force_resync: true,
    });
    await openSession(d);
    ElMessage.success($t("upstreamConnector.resyncOk"));
  } catch (e) {
    ElMessage.error(
      isHttpError(e) ? e.message : $t("upstreamConnector.bootstrapFailed"),
    );
  } finally {
    bootstrapping.value = false;
  }
}

function goBack() {
  if (draft.value?.linked_supplier_id) {
    void router.push({
      name: "SupplierDetail",
      params: { id: draft.value.linked_supplier_id },
    });
    return;
  }
  void router.push({
    path: "/operations/upstream-supply",
    query: { tab: "suppliers" },
  });
}

function beginRun(): AbortSignal {
  vsplitRef.value?.expand();
  runAbort.value?.abort();
  const ac = new AbortController();
  runAbort.value = ac;
  runGen += 1;
  running.value = true;
  return ac.signal;
}

function finishRun(gen: number) {
  if (gen !== runGen) return;
  running.value = false;
  runAbort.value = null;
}

function abortRun() {
  runAbort.value?.abort();
}

function onSendClick() {
  if (!canWriteWorkbench.value) return;
  if (running.value) {
    abortRun();
    return;
  }
  void doRun("business");
}

function applyCurlPaste(parsed: ParsedImport) {
  const src = parsed.endpoints[0];
  if (!src || !draft.value || !activeEp.value) return;
  if (parsed.upstream_url) draft.value.upstream_url = parsed.upstream_url;
  activeEp.value.method = src.method || activeEp.value.method;
  activeEp.value.path = src.path || "/";
  activeEp.value.headers = src.headers || {};
  activeEp.value.query = src.query || {};
  if (src.body !== undefined) activeEp.value.body = src.body;
  headerRows.value = mapToKv(activeEp.value.headers);
  queryRows.value = mapToKv(activeEp.value.query);
  urlDraft.value = composeRequestUrl(
    draft.value.upstream_url,
    activeEp.value.path,
    queryRows.value,
  );
  ElMessage.success($t("upstreamConnector.pasteCurlOk"));
}

function blockIfUnresolved(names: string[]): boolean {
  if (!names.length) return false;
  ElMessage.warning(
    $t("upstreamConnector.unresolvedVarsBlock", { vars: names.join(", ") }),
  );
  return true;
}

async function refreshGate() {
  if (!draft.value) return;
  try {
    const d = await getDraft(draft.value.id);
    draft.value = { ...draft.value, gate: d.gate };
  } catch {
    void 0;
  }
}

function localSendBlock(): boolean {
  const url = (
    urlDraft.value ||
    draft.value?.upstream_url ||
    ""
  ).trim();
  if (!url) {
    urlMissing.value = true;
    ElMessage.warning($t("upstreamConnector.urlRequired"));
    vsplitRef.value?.expand();
    void nextTick(() => urlFieldRef.value?.focus());
    return true;
  }
  urlMissing.value = false;
  const method = (activeEp.value?.method || "GET").trim();
  if (!isLegalHttpMethod(method)) {
    ElMessage.warning($t("upstreamConnector.methodInvalid"));
    return true;
  }
  return false;
}

async function doRun(mode: string) {
  if (!draft.value || !activeEp.value) return;
  if (localSendBlock()) return;
  if (blockIfUnresolved(unresolved.value)) return;
  const method = (activeEp.value.method || "GET").toUpperCase();
  if (mode === "business" && WRITE_METHODS.has(method)) {
    try {
      await ElMessageBox.confirm(
        $t("upstreamConnector.writeConfirmMsg"),
        $t("upstreamConnector.writeConfirmTitle"),
        { type: "warning" },
      );
    } catch {
      return;
    }
  }
  const gen = runGen + 1;
  const signal = beginRun();
  try {
    await saveDraft({ silent: true, signal });
    if (!appliedRequest()) return;
    const r = await runDraft(
      draft.value.id,
      {
        mode,
        endpoint_id: activeEp.value.id,
        assertions: activeEp.value.assertions as AssertionSpec[],
      },
      { signal, ...RUN_HTTP },
    );
    lastRun.value = r;
    runs.value = [r, ...runs.value].slice(0, 50);
    await refreshGate();
    if (r.ok) ElMessage.success($t("upstreamConnector.runOk"));
    else ElMessage.error(r.error_message || r.error_code || $t("upstreamConnector.runFail"));
  } catch (err) {
    if (isRunCancelled(err)) {
      ElMessage.info($t("upstreamConnector.runCancelled"));
      return;
    }
    ElMessage.error(
      isHttpError(err) ? err.message : $t("upstreamConnector.runFail"),
    );
  } finally {
    finishRun(gen);
  }
}

async function doLiveRun() {
  if (!draft.value?.linked_supplier_id || !activeEp.value) return;
  flushKvToActive();
  const epId = activeEp.value.id;
  const channelCode = autoChannelsFromEndpoints(endpoints.value).find(
    (c) => c.endpoint_id === epId,
  )?.channel_code;
  if (!channelCode) return;
  const gen = runGen + 1;
  const signal = beginRun();
  try {
    const r = await liveRun(
      draft.value.linked_supplier_id,
      { channel_code: channelCode, payload: activeEp.value.body ?? {} },
      { signal, ...RUN_HTTP },
    );
    lastRun.value = r;
    if (r.ok) ElMessage.success($t("upstreamConnector.runOk"));
    else ElMessage.error(r.error_message || r.error_code || $t("upstreamConnector.runFail"));
  } catch (err) {
    if (isRunCancelled(err)) {
      ElMessage.info($t("upstreamConnector.runCancelled"));
      return;
    }
    ElMessage.error(
      isHttpError(err) ? err.message : $t("upstreamConnector.runFail"),
    );
  } finally {
    finishRun(gen);
  }
}

async function doTransformPreview() {
  if (!draft.value || !activeEp.value) return;
  if (blockIfUnresolved(unresolved.value)) return;
  const gen = runGen + 1;
  const signal = beginRun();
  try {
    await saveDraft({ silent: true, signal });
    const r = await transformPreview(
      draft.value.id,
      {
        endpoint_id: activeEp.value.id,
      },
      { signal, ...RUN_HTTP },
    );
    lastRun.value = {
      id: "preview",
      mode: "transform_preview",
      ok: r.ok,
      latency_ms: 0,
      stages: r.stages || [],
      assertions: [],
      created_at: new Date().toISOString(),
    };
    reqTab.value = "transform";
  } catch (err) {
    if (isRunCancelled(err)) {
      ElMessage.info($t("upstreamConnector.runCancelled"));
      return;
    }
    const message = isHttpError(err) ? err.message : $t("upstreamConnector.runFail");
    lastRun.value = {
      id: "preview-error",
      mode: "transform_preview",
      ok: false,
      latency_ms: 0,
      stages: [],
      assertions: [],
      error_code: "TRANSFORM_PREVIEW_FAILED",
      error_message: message,
      created_at: new Date().toISOString(),
    };
    ElMessage.error(message);
  } finally {
    finishRun(gen);
  }
}

function buildLocalBlankDraft(): ConnectorDraft {
  const now = new Date().toISOString();
  return {
    id: "",
    draft_code: genSessionCode(),
    name: $t("upstreamConnector.untitledName"),
    description: null,
    upstream_type: CODE_FREE_UPSTREAM_TYPE,
    upstream_url: "",
    config: {
      upstream_type: CODE_FREE_UPSTREAM_TYPE,
      timeouts: { total_ms: 15000 },
    },
    endpoints: [blankWorkbenchEndpoint(0)],
    status: "draft",
    source: "manual",
    linked_supplier_id: null,
    created_by: null,
    created_at: now,
    updated_at: now,
  };
}

async function createBlankSession() {
  if (!canWriteWorkbench.value) return;

  await openSession(buildLocalBlankDraft());
}

async function doImport() {
  if (!canWriteWorkbench.value) return;

  importing.value = true;
  try {
    if (importMode.value === "merge" && draft.value) {
      let parsed;
      if (importTab.value === "curl") parsed = parseCurl(importCurlText.value);
      else if (importTab.value === "postman")
        parsed = parsePostmanCollection(JSON.parse(importJsonText.value));
      else parsed = parseOpenApiDocument(JSON.parse(importJsonText.value));
      endpoints.value = mergeImportedEndpoints(endpoints.value, parsed.endpoints);
      if (parsed.upstream_url && !draft.value.upstream_url.trim()) {
        draft.value.upstream_url = parsed.upstream_url;
      }
      if (parsed.name && draft.value.name === $t("upstreamConnector.untitledName")) {
        draft.value.name = parsed.name;
      }
      showImport.value = false;
      ElMessage.success($t("upstreamConnector.importedMerge", { n: parsed.endpoints.length }));
      return;
    }
    let d: ConnectorDraft;
    if (importTab.value === "curl") {
      d = await importCurl({ curl: importCurlText.value });
    } else if (importTab.value === "postman") {
      d = await importPostman({ collection: JSON.parse(importJsonText.value) });
    } else {
      d = await importOpenApi({ document: JSON.parse(importJsonText.value) });
    }
    showImport.value = false;
    await openSession(d);
    ElMessage.success($t("upstreamConnector.imported"));
  } catch (e) {
    ElMessage.error((e as Error)?.message || "Import failed");
  } finally {
    importing.value = false;
  }
}

async function openPublish() {
  if (!canWriteWorkbench.value) return;

  if (!canPublish.value) {
    ElMessage.warning($t("upstreamConnector.publishNeedRun"));
    return;
  }
  publishResult.value = null;
  publishChannels.value = autoChannelsFromEndpoints(endpoints.value);
  if (draft.value?.name) publishForm.value.supplier_name = draft.value.name;
  const cryptoMsgs = cryptoIssuesOnEndpoints();
  if (cryptoMsgs.length) {
    ElMessage.warning(cryptoMsgs[0]);
    return;
  }
  try {
    await saveDraft({ silent: true });
  } catch {
    ElMessage.error($t("upstreamConnector.saveDraftFailed"));
    return;
  }
  showPublish.value = true;
}

async function doPublish() {
  if (!canWriteWorkbench.value) return;
  if (!draft.value) return;
  if (blockIfUnresolved(unresolvedAll.value)) return;
  const cryptoMsgsPublish = cryptoIssuesOnEndpoints();
  if (cryptoMsgsPublish.length) {
    ElMessage.warning(cryptoMsgsPublish[0]);
    return;
  }
  if (!draft.value.name.trim() || !draft.value.upstream_url.trim()) {
    ElMessage.warning($t("upstreamConnector.fieldsRequired"));
    return;
  }
  if (!publishForm.value.supplier_code.trim() || !publishForm.value.supplier_name.trim()) {
    ElMessage.warning($t("upstreamConnector.publishFieldsRequired"));
    return;
  }
  publishing.value = true;
  publishResult.value = null;
  try {
    await saveDraft({ silent: true });
    const r = await publishDraft(draft.value.id, {
      mode: publishForm.value.mode as "supplier_only" | "supplier_channels" | "full_onboarding",
      supplier: {
        supplier_code: publishForm.value.supplier_code,
        name: publishForm.value.supplier_name,
        status: "active",
      },
      channels:
        publishForm.value.mode === "supplier_only"
          ? undefined
          : publishChannels.value.map((c) => ({
              endpoint_id: c.endpoint_id,
              channel_code: c.channel_code,
              name: c.name,
              path_prefix: c.path_prefix,
              cost_price: c.cost_price,
              qps_limit: c.qps_limit,
            })),
      route:
        publishForm.value.mode === "full_onboarding"
          ? {
              product_ref: { product_id: publishForm.value.product_id },
              priority: 100,
              weight: 100,
              version_tag: "v1.0",
            }
          : undefined,
      options: {
        require_gate: publishForm.value.require_gate,
        force: {
          force_publish: publishForm.value.force_publish,
          force_reason: publishForm.value.force_reason || null,
        },
        fail_if_route_exists: true,
      },
      comment: publishForm.value.comment,
    });
    publishResult.value = r;
    ElMessage.success($t("upstreamConnector.publishOk"));
    if (r.supplier_id && draft.value) {
      draft.value.linked_supplier_id = r.supplier_id;
      await syncSessionRoute(draft.value);
    }
  } finally {
    publishing.value = false;
  }
}

async function doExport() {
  if (!canWriteWorkbench.value) return;
  if (!draft.value) return;
  try {
    await saveDraft({ silent: true });
  } catch {
    ElMessage.error($t("upstreamConnector.saveDraftFailed"));
    return;
  }
  const col = await exportPostman(draft.value.id);
  const blob = new Blob([JSON.stringify(col, null, 2)], { type: "application/json" });
  saveAs(blob, `${draft.value.name || "connector"}.postman.json`);
}

function onSendCommand(cmd: string) {
  if (cmd === "auth") void doRun("auth_only");
  else if (cmd === "transform") void doTransformPreview();
  else if (cmd === "curl") void copyCurl();
  else if (cmd === "live") void doLiveRun();
}

function onSessionAction(cmd: string) {
  if (!canWriteWorkbench.value) return;
  if (cmd === "import") showImport.value = true;
  else if (cmd === "export") void doExport();
}

function composedUrl() {
  return composeRequestUrl(
    draft.value?.upstream_url || "",
    activeEp.value?.path || "",
    queryRows.value,
  );
}

function refreshUrlDraft() {
  if (urlFocused.value) return;
  urlDraft.value = composedUrl();
}

function applyRequestUrl(raw: string) {
  if (!draft.value || !activeEp.value) return;
  const parts = trySplitRequestUrl(raw);
  if (!parts) return;
  if (parts.base) draft.value.upstream_url = parts.base;
  activeEp.value.path = parts.path;
  queryRows.value = mergeQueryFromMap(parts.query, queryRows.value);
}

function onUrlInput(raw: string) {
  urlDraft.value = raw;
  applyRequestUrl(raw);
}

function onUrlBlur() {
  urlFocused.value = false;
  applyRequestUrl(urlDraft.value);
  urlDraft.value = composedUrl();
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

async function copyCurl() {
  const req = appliedRequest();
  if (!req || !draft.value) return;
  const text = buildCurl({
    baseUrl: draft.value.upstream_url,
    method: req.method,
    path: req.path,
    headers: req.headers,
    query: req.query,
    body: req.body,
  });
  await copyWithFeedback(text);
}

async function bootstrap() {
  bootstrapping.value = true;
  try {
    const supplierId =
      typeof route.query.supplier_id === "string" ? route.query.supplier_id : "";
    const sessionId =
      typeof route.query.session === "string" ? route.query.session : "";

    if (supplierId) {
      const sup = await fetchSupplierById(supplierId);
      if (resolveUpstreamType(sup) === "microservice") {
        leaveForMicroservice(supplierId);
        return;
      }
      if (!canWriteWorkbench.value) {
        await loadReadOnlySupplierDraft(supplierId);
        return;
      }
      const d = await draftFromSupplier(supplierId);
      await openSession(d);
      return;
    }
    if (sessionId) {
      const d = await getDraft(sessionId);
      await openSession(d);
      return;
    }
    if (!canWriteWorkbench.value) {
      draft.value = null;
      return;
    }
    await createBlankSession();
  } catch (e) {
    draft.value = null;
    ElMessage.error((e as Error)?.message || $t("upstreamConnector.bootstrapFailed"));
  } finally {
    bootstrapping.value = false;
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
      { id: "save", label: $t("upstreamConnector.syncDraftNow"), hint: "Ctrl/⌘+S" },
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
        id: "tab:variables",
        label: $t("upstreamConnector.envVars"),
        keywords: "env",
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
      keywords: `${ep.method || ""} ${ep.path || ""}`,
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
    void doRun("auth_only");
    return;
  }
  if (id === "transform") {
    reqTab.value = "transform";
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
  if (id === "tab:variables") {
    envDrawerVisible.value = true;
    return;
  }
  if (id.startsWith("tab:")) {
    reqTab.value = id.slice(4);
    return;
  }
  if (id.startsWith("method:") && activeEp.value) {
    activeEp.value.method = id.slice(7);
    return;
  }
  if (id.startsWith("ep:")) {
    const idx = Number(id.slice(3));
    if (Number.isFinite(idx)) activeEpIdx.value = idx;
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

watch(urlDraft, (v) => {
  if (v.trim()) urlMissing.value = false;
});

function onBeforeUnload(e: BeforeUnloadEvent) {
  if (!shouldGuardLeave()) return;
  e.preventDefault();
  e.returnValue = "";
}

onBeforeRouteLeave(async () => {
  if (!shouldGuardLeave()) return true;
  leavePromptOpen.value = true;
  try {
    await ElMessageBox.confirm(
      $t("upstreamConnector.leaveUnsavedMessage"),
      $t("upstreamConnector.leaveUnsavedTitle"),
      {
        type: "warning",
        distinguishCancelAndClose: true,
        confirmButtonText: $t("upstreamConnector.leaveSave"),
        cancelButtonText: $t("upstreamConnector.leaveDiscard"),
      },
    );
    await saveDraft({ silent: true, skipRouteSync: true });
    return true;
  } catch (action) {
    if (action === "cancel") return true;
    leavePromptOpen.value = false;
    if (action !== "close") {
      ElMessage.error($t("upstreamConnector.saveDraftFailed"));
    }
    return false;
  }
});

onMounted(() => {
  void bootstrap();
  window.addEventListener("keydown", onHotkey);
  window.addEventListener("beforeunload", onBeforeUnload);
  pageEl.value?.addEventListener("scroll", onWorkbenchScroll, true);
});
onUnmounted(() => {
  abortRun();
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
      bootstrapping.value = true;
      try {
        const sup = await fetchSupplierById(supplierId);
        if (resolveUpstreamType(sup) === "microservice") {
          leaveForMicroservice(supplierId);
          return;
        }
        if (!canWriteWorkbench.value) {
          await loadReadOnlySupplierDraft(supplierId);
          return;
        }
        const d = await draftFromSupplier(supplierId);
        await openSession(d);
      } catch (e) {
        ElMessage.error((e as Error)?.message || $t("upstreamConnector.bootstrapFailed"));
      } finally {
        bootstrapping.value = false;
      }
      return;
    }
    if (sessionId && typeof sessionId === "string" && sessionId !== draft.value?.id) {
      bootstrapping.value = true;
      try {
        const d = await getDraft(sessionId);
        await openSession(d);
      } catch (e) {
        draft.value = null;
        ElMessage.error((e as Error)?.message || $t("upstreamConnector.bootstrapFailed"));
      } finally {
        bootstrapping.value = false;
      }
      return;
    }
    if (!supplierId && !sessionId) {
      bootstrapping.value = true;
      try {
        await createBlankSession();
      } catch (e) {
        ElMessage.error((e as Error)?.message || $t("upstreamConnector.bootstrapFailed"));
      } finally {
        bootstrapping.value = false;
      }
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
  height: var(--art-full-height, calc(100vh - 48px));
  min-height: 0;
  min-width: 0;
  width: 100%;
  padding: 0 4px 8px;
  box-sizing: border-box;
}
.wb-top {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  height: 44px;
  min-height: 44px;
  padding: 0 4px;
  margin-bottom: 6px;
  flex-shrink: 0;
  box-sizing: border-box;
}
.wb-top__left {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  flex: 1 1 auto;
  flex-wrap: nowrap;
}
.wb-top__right {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
  flex-wrap: nowrap;
}
.wb-top__back {
  padding: 0 6px;
  flex-shrink: 0;
}
.wb-top__name-input {
  min-width: 120px;
  max-width: 240px;
  height: 30px;
  padding: 0 8px;
  border: 1px solid transparent;
  border-radius: var(--wb-radius, 4px);
  background: transparent;
  color: var(--el-text-color-primary);
  font-size: 14px;
  font-weight: 600;
  outline: none;
  transition: border-color 0.15s ease, background-color 0.15s ease;
  &:hover:not(:read-only) {
    border-color: var(--el-border-color-lighter);
    background: var(--el-fill-color-light);
  }
  &:focus:not(:read-only) {
    border-color: var(--el-color-primary);
    background: var(--el-fill-color-blank);
  }
}
.wb-top__gate-pill {
  display: inline-flex;
  align-items: center;
  height: 28px;
  padding: 0 10px;
  border-radius: 999px;
  border: 1px solid var(--el-border-color-lighter);
  background: var(--el-fill-color-light);
  color: var(--el-text-color-secondary);
  font-size: 12px;
  font-weight: 600;
  white-space: nowrap;
  &.is-ready {
    border-color: color-mix(in srgb, var(--el-color-success) 40%, transparent);
    background: color-mix(in srgb, var(--el-color-success) 12%, var(--el-bg-color));
    color: var(--el-color-success);
  }
}
.wb-env-drawer__hint {
  margin: 0 0 12px;
  font-size: 12px;
  line-height: 1.5;
  color: var(--el-text-color-secondary);
}
.wb-empty {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
}
.wb-pane {
  height: 100%;
  min-height: 0;
  border: 1px solid var(--el-border-color-lighter);
  border-radius: var(--art-card-radius);
  background: var(--el-bg-color);
  overflow: hidden;
}
.wb-main {
  height: 100%;
  min-height: 0;
  min-width: 0;
  border: 1px solid var(--el-border-color-lighter);
  border-radius: var(--art-card-radius);
  background: var(--el-bg-color);
  overflow: hidden;
}
.wb-request {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
  min-width: 0;
  padding: 16px 16px 0;
  overflow: hidden;
}
.wb-runbar {
  display: flex;
  gap: var(--wb-runbar-gap);
  align-items: center;
  flex-wrap: nowrap;
  flex-shrink: 0;
  min-width: 0;
}
.wb-runbar__actions {
  display: flex;
  gap: 8px;
  align-items: center;
  flex-shrink: 0;
}
.wb-urlbar {
  display: flex;
  align-items: center;
  flex: 1;
  min-width: 0;
  height: var(--wb-urlbar-h);
  background: var(--wb-surface-input);
  border: 1px solid var(--wb-divider-subtle);
  border-radius: var(--wb-radius);
  overflow: hidden;
  &.is-error {
    border-color: var(--wb-danger, var(--el-color-danger));
    box-shadow: inset 0 0 0 1px var(--wb-danger, var(--el-color-danger));
  }
}
.wb-split-dir-btn {
  flex: 0 0 32px;
  width: 32px;
  height: var(--wb-urlbar-h, 32px);
  padding: 0;
  border: 1px solid var(--wb-divider-subtle, var(--el-border-color-extra-light));
  border-radius: var(--wb-radius, 4px);
  background: var(--wb-surface-input, var(--el-fill-color-light));
  color: var(--wb-text-muted, var(--el-text-color-secondary));
  font-size: 15px;
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  &:hover:not(:disabled) {
    color: var(--wb-text, var(--el-text-color-primary));
    border-color: var(--el-border-color);
  }
  &:disabled {
    cursor: not-allowed;
    opacity: 0.45;
  }
}
.wb-urlbar__method {
  display: inline-flex;
  height: 100%;
  flex-shrink: 0;
  :deep(.el-tooltip__trigger) {
    display: inline-flex;
    height: 100%;
    width: 6.5rem;
  }
}
.hopp-btn {
  display: inline-flex;
  align-items: stretch;
  height: 32px;
  border-radius: 4px;
  overflow: hidden;
  flex-shrink: 0;
  :deep(.el-tooltip__trigger) {
    display: inline-flex;
    height: 100%;
  }
}
.hopp-btn--primary {
  background: transparent;
  color: var(--wb-accent-contrast);
  .hopp-btn__main,
  .hopp-btn__caret {
    background: var(--wb-accent);
    color: var(--wb-accent-contrast);
  }
  .hopp-btn__caret {
    border-left: 1px solid rgb(255 255 255 / 22%);
  }
  .hopp-btn__main:hover,
  .hopp-btn__caret:hover {
    background: color-mix(in srgb, var(--wb-accent) 86%, #000);
  }
}
.hopp-btn__main,
.hopp-btn__caret {
  border: none;
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  height: 100%;
  font-size: var(--wb-fs-body);
  font-weight: 600;
  padding: 0;
  &:disabled {
    cursor: default;
    opacity: 0.7;
  }
}
.hopp-btn__main {
  min-width: 5rem;
  padding: 0 12px;
  border-radius: 4px 0 0 4px;
}
.hopp-btn__main:only-child {
  border-radius: 4px;
}
.hopp-btn__caret {
  width: 28px;
  flex: 0 0 28px;
  font-size: 16px;
  color: inherit;
}
.wb-unresolved {
  margin: 4px 0 0;
  font-size: 11px;
  line-height: 16px;
  color: var(--el-color-warning);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.req-tabs-shell {
  position: relative;
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  margin-top: 8px;
}
.req-tabs {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  :deep(.el-tabs__header) {
    flex-shrink: 0;
    margin: 0;
  }
  :deep(.el-tabs__nav) {
    display: flex;
    width: 100%;
  }
  :deep(.el-tabs__item) {
    height: var(--wb-tab-h);
    padding: 0 14px;
    font-size: var(--wb-fs-body);
    font-weight: 600;
    color: var(--wb-text-muted);
  }
  :deep(.el-tabs__item.is-active) {
    color: var(--wb-text);
    font-weight: 500;
  }
  :deep(.el-tabs__active-bar) {
    height: 2px;
    background: var(--wb-accent);
  }
  :deep(.el-tabs__nav-wrap::after) {
    display: none;
  }
  :deep(.el-tabs__content) {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding-top: 10px;
  }
  :deep(.el-tab-pane) {
    height: 100%;
  }
}
.req-actions {
  display: flex;
  gap: 8px;
  margin: 10px 0;
}
@media (max-width: 1100px) {
  .connector-workbench-page {
    height: auto;
    min-height: var(--art-full-height, calc(100vh - 48px));
  }
  .wb-top {
    flex-wrap: wrap;
    height: auto;
    padding: 4px;
  }
  .wb-runbar {
    flex-wrap: wrap;
    align-items: stretch;
  }
  .wb-urlbar {
    flex: 1 1 100%;
    min-width: 100%;
  }
  :deep(.wb-adapter-chip) {
    flex: 0 1 auto;
  }
  .wb-runbar__actions {
    flex: 1 1 auto;
    justify-content: flex-end;
    flex-wrap: wrap;
  }
}
</style>
