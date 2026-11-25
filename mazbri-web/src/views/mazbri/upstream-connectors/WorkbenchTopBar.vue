<template>
  <header class="wb-topbar">
    <div class="wb-topbar__left">
      <button
        type="button"
        class="wb-topbar__ghost-btn"
        :title="$t('upstreamConnector.backToSupply')"
        :aria-label="$t('upstreamConnector.backToSupply')"
        @click="emit('back')"
      >
        <ArtSvgIcon icon="ri:arrow-left-line" />
        <span>{{ $t("upstreamConnector.backToSupply") }}</span>
      </button>

      <span class="wb-topbar__sep" aria-hidden="true" />

      <template v-if="draft">
        <input
          v-model="draft.name"
          type="text"
          class="wb-topbar__name-input"
          :readonly="!canWrite"
          :aria-label="$t('upstreamConnector.fields.name')"
          :placeholder="$t('upstreamConnector.untitledName')"
        />

        <HoppMenu
          v-model:visible="switcherOpen"
          placement="bottom-start"
          :append-to="appendTo"
        >
          <template #trigger>
            <button
              type="button"
              class="wb-topbar__switcher-btn"
              :title="$t('upstreamConnector.switchSession')"
              :aria-label="$t('upstreamConnector.switchSession')"
              @click="emit('open-switcher')"
            >
              <span class="wb-topbar__badge-dot" :class="{ linked: isSupplierLinked }" />
              <span class="wb-topbar__switcher-text">
                {{
                  isSupplierLinked
                    ? supplierCode || $t("upstreamConnector.statusPublished")
                    : $t("upstreamConnector.statusDraft")
                }}
              </span>
              <ArtSvgIcon icon="ri:arrow-down-s-line" />
            </button>
          </template>
          <div class="wb-switcher-panel">
            <button
              v-if="canWrite"
              type="button"
              class="hopp-menu-item wb-switcher-new"
              @click="onCreateNew"
            >
              <ArtSvgIcon icon="ri:add-line" />
              <span>{{ $t("upstreamConnector.newSession") }}</span>
            </button>

            <div v-if="recentDrafts.length" class="wb-switcher-group">
              <div class="wb-switcher-group__title">
                {{ $t("upstreamConnector.recentDrafts") }}
              </div>
              <button
                v-for="d in recentDrafts.slice(0, 10)"
                :key="d.id"
                type="button"
                class="hopp-menu-item wb-switcher-row"
                :class="{ 'is-current': d.id === draft.id }"
                @click="onSelectDraft(d.id)"
              >
                <span class="wb-switcher-row__name">{{ d.name || d.draft_code }}</span>
                <span class="wb-switcher-row__code">{{ d.draft_code }}</span>
              </button>
            </div>

            <div v-if="suppliers.length" class="wb-switcher-group">
              <div class="wb-switcher-group__title">
                {{ $t("upstreamConnector.publishedSuppliers") }}
              </div>
              <button
                v-for="s in suppliers.slice(0, 12)"
                :key="s.id"
                type="button"
                class="hopp-menu-item wb-switcher-row"
                :class="{ 'is-current': s.id === draft.linked_supplier_id }"
                @click="onSelectSupplier(s.id)"
              >
                <span class="wb-switcher-row__name">{{ s.name }}</span>
                <span class="wb-switcher-row__code">{{ s.supplier_code }}</span>
              </button>
            </div>
          </div>
        </HoppMenu>

        <span v-if="!canWrite" class="wb-topbar__tag">
          {{ $t("upstreamConnector.readOnlyHint") }}
        </span>
        <span
          v-if="saveBadge"
          class="wb-topbar__tag"
          :data-tone="saveBadge.type"
        >
          {{ saveBadge.text }}
        </span>
      </template>
    </div>

    <div v-if="draft" class="wb-topbar__right">
      <div class="wb-env-ctl">
        <HoppMenu
          v-model:visible="envMenuOpen"
          placement="bottom-end"
          :append-to="appendTo"
        >
          <template #trigger>
            <button
              type="button"
              class="wb-env-ctl__main"
              :title="$t('upstreamConnector.namedEnvs')"
            >
              <span
                class="wb-env-ctl__dot"
                :class="{ active: Boolean(activeEnvName) }"
              />
              <span class="wb-env-ctl__label">
                {{ activeEnvName || $t("upstreamConnector.noEnvSelected") }}
              </span>
              <span class="wb-env-ctl__count tabular-nums">{{ envVarCount }}</span>
              <ArtSvgIcon icon="ri:arrow-down-s-line" />
            </button>
          </template>
          <div class="wb-env-menu">
            <button
              type="button"
              class="hopp-menu-item wb-env-menu__row"
              :class="{ 'is-current': !envState.activeEnvId }"
              @click="onPickEnv(null)"
            >
              <span>{{ $t("upstreamConnector.noEnvSelected") }}</span>
            </button>
            <button
              v-for="env in envState.environments"
              :key="env.id"
              type="button"
              class="hopp-menu-item wb-env-menu__row"
              :class="{ 'is-current': env.id === envState.activeEnvId }"
              @click="onPickEnv(env.id)"
            >
              <span>{{ env.name }}</span>
              <span class="wb-env-menu__n tabular-nums">
                {{ env.rows.filter((r) => r.enabled !== false && r.key.trim()).length }}
              </span>
            </button>
            <div v-if="canWrite" class="wb-env-menu__create">
              <input
                v-model="newEnvName"
                type="text"
                class="wb-env-menu__input"
                :placeholder="$t('upstreamConnector.envNamePlaceholder')"
                :aria-label="$t('upstreamConnector.newEnv')"
                @keydown.enter.prevent="submitNewEnv"
              />
              <button
                type="button"
                class="wb-env-menu__add"
                @click="submitNewEnv"
              >
                {{ $t("upstreamConnector.newEnv") }}
              </button>
            </div>
            <div class="hopp-menu-divider" />
            <button
              type="button"
              class="hopp-menu-item"
              @click="onOpenEnvSidebar"
            >
              {{ $t("upstreamConnector.sidebarEnv") }}
            </button>
          </div>
        </HoppMenu>

        <ElPopover
          placement="bottom-end"
          :width="320"
          trigger="click"
          :append-to="appendTo"
        >
          <div class="wb-env-peek">
            <div class="wb-env-peek__head">
              <span>{{ activeEnvName || $t("upstreamConnector.noEnvSelected") }}</span>
              <span class="tabular-nums">{{ runtimeEntries.length }}</span>
            </div>
            <div v-if="!runtimeEntries.length" class="wb-env-peek__empty">
              {{ $t("upstreamConnector.envPreviewEmpty") }}
            </div>
            <div v-else class="wb-env-peek__list">
              <div
                v-for="[k, v] in runtimeEntries"
                :key="k"
                class="wb-env-peek__row"
              >
                <code>{{ k }}</code>
                <span>{{ !canWrite ? (v ? "••••••••" : "∅") : v || "∅" }}</span>
              </div>
            </div>
          </div>
          <template #reference>
            <button
              type="button"
              class="wb-env-ctl__peek"
              :title="$t('upstreamConnector.envVars')"
              :aria-label="$t('upstreamConnector.envVars')"
            >
              <ArtSvgIcon icon="ri:code-s-slash-line" />
            </button>
          </template>
        </ElPopover>
      </div>

      <button
        type="button"
        class="wb-topbar__icon-btn"
        :title="isDark ? 'Light' : 'Dark'"
        :aria-label="isDark ? 'Light' : 'Dark'"
        @click="toggleTheme"
      >
        <ArtSvgIcon :icon="isDark ? 'ri:sun-fill' : 'ri:moon-line'" />
      </button>

      <HoppMenu v-if="canWrite" placement="bottom-end" :append-to="appendTo">
        <template #trigger>
          <button type="button" class="wb-topbar__btn">
            <span>{{ $t("upstreamConnector.importExport") }}</span>
            <ArtSvgIcon icon="ri:arrow-down-s-line" />
          </button>
        </template>
        <button
          type="button"
          class="hopp-menu-item"
          @click="emit('open-import')"
        >
          {{ $t("upstreamConnector.import") }}
        </button>
        <button
          type="button"
          class="hopp-menu-item"
          @click="emit('export-postman')"
        >
          {{ $t("upstreamConnector.exportPostman") }}
        </button>
        <div class="hopp-menu-divider" />
        <button
          type="button"
          class="hopp-menu-item"
          @click="envFileInput?.click()"
        >
          {{ $t("upstreamConnector.importEnvJson") }}
        </button>
        <button
          type="button"
          class="hopp-menu-item"
          @click="emit('export-env')"
        >
          {{ $t("upstreamConnector.exportEnvJson") }}
        </button>
        <button
          v-if="draft.linked_supplier_id"
          type="button"
          class="hopp-menu-item is-divided"
          @click="emit('resync')"
        >
          {{ $t("upstreamConnector.resyncFromLive") }}
        </button>
      </HoppMenu>

      <input
        v-if="canWrite"
        ref="envFileInput"
        type="file"
        accept=".json,application/json"
        hidden
        @change="onEnvFilePicked"
      />

      <template v-if="canWrite">
        <button
          type="button"
          class="wb-topbar__btn"
          :disabled="saving"
          title="Ctrl/⌘+S"
          @click="emit('save')"
        >
          <ArtSvgIcon icon="ri:save-line" />
          <span>{{ $t("upstreamConnector.saveDraft") }}</span>
        </button>

        <button
          type="button"
          class="wb-topbar__gate tabular-nums"
          :class="{ ready: gate.total > 0 && gate.ok === gate.total }"
          :title="$t('upstreamConnector.gateChecklist')"
          @click="emit('open-gate')"
        >
          <span class="wb-topbar__gate-dot" />
          <span>{{ $t("upstreamConnector.gateShort") }} {{ gate.ok }}/{{ gate.total }}</span>
        </button>

        <button
          type="button"
          class="wb-topbar__primary-btn"
          :disabled="publishing"
          :title="
            canPublish
              ? $t('upstreamConnector.publish')
              : $t('upstreamConnector.publishNeedRun')
          "
          @click="emit('publish')"
        >
          <ArtSvgIcon icon="ri:upload-cloud-2-line" />
          <span>{{ $t("upstreamConnector.publish") }}</span>
        </button>
      </template>
    </div>
  </header>
</template>

<script setup lang="ts">
import HoppMenu from "@/components/common/hopp-menu/index.vue";
import { SystemThemeEnum } from "@/enums/appEnum";
import { $t } from "@/locales";
import { useSettingStore } from "@/store/modules/setting";
import type { ConnectorDraft } from "@/api/upstream-connectors";
import type { Supplier } from "@/api/supplier";
import type { WorkbenchEnvState } from "./workbenchHelpers";

const props = defineProps<{
  draft: ConnectorDraft | null;
  recentDrafts: ConnectorDraft[];
  suppliers: Supplier[];
  supplierCode: string;
  isSupplierLinked: boolean;
  canWrite: boolean;
  saving: boolean;
  publishing: boolean;
  canPublish: boolean;
  saveBadge: { type: "info" | "danger" | "warning" | "success"; text: string } | null;
  gate: { ok: number; total: number };
  envState: WorkbenchEnvState;
  envVarCount: number;
  runtimeEnvMap: Record<string, string>;
  appendTo?: HTMLElement;
}>();

const emit = defineEmits<{
  back: [];
  "open-switcher": [];
  "switch-draft": [string];
  "switch-supplier": [string];
  "new-session": [];
  "select-env": [string | null];
  "create-env": [string];
  "open-env-panel": [];
  "open-import": [];
  "export-postman": [];
  "import-env": [string];
  "export-env": [];
  resync: [];
  save: [];
  "open-gate": [];
  publish: [];
}>();

const settingStore = useSettingStore();
const isDark = computed(() => Boolean(settingStore.isDark));

function toggleTheme() {
  const next = isDark.value ? SystemThemeEnum.LIGHT : SystemThemeEnum.DARK;
  settingStore.setGlopTheme(next, next);
  settingStore.applyToDom();
}

const switcherOpen = ref(false);
const envMenuOpen = ref(false);
const newEnvName = ref("");
const envFileInput = ref<HTMLInputElement | null>(null);

const activeEnvName = computed(
  () =>
    props.envState.environments.find((e) => e.id === props.envState.activeEnvId)
      ?.name || "",
);

const runtimeEntries = computed(() => Object.entries(props.runtimeEnvMap));

function onCreateNew() {
  switcherOpen.value = false;
  emit("new-session");
}

function onSelectDraft(id: string) {
  switcherOpen.value = false;
  emit("switch-draft", id);
}

function onSelectSupplier(id: string) {
  switcherOpen.value = false;
  emit("switch-supplier", id);
}

function onPickEnv(id: string | null) {
  envMenuOpen.value = false;
  emit("select-env", id);
}

function submitNewEnv() {
  const name = newEnvName.value.trim();
  if (!name) return;
  emit("create-env", name);
  newEnvName.value = "";
  envMenuOpen.value = false;
}

function onOpenEnvSidebar() {
  envMenuOpen.value = false;
  emit("open-env-panel");
}

async function onEnvFilePicked(ev: Event) {
  const input = ev.target as HTMLInputElement;
  const file = input.files?.[0];
  input.value = "";
  if (!file) return;
  const text = await file.text();
  emit("import-env", text);
}
</script>

<style scoped lang="scss">
.wb-topbar {
  display: flex;
  flex-shrink: 0;
  gap: 8px;
  align-items: center;
  justify-content: space-between;
  height: 44px;
  min-height: 44px;
  padding: 0 12px;
  background: var(--wb-surface);
  border-bottom: 1px solid var(--wb-divider);
}

.wb-topbar__left,
.wb-topbar__right {
  display: flex;
  gap: 6px;
  align-items: center;
  min-width: 0;
}

.wb-topbar__left {
  flex: 1 1 auto;
}

.wb-topbar__right {
  flex-shrink: 0;
}

.wb-topbar__sep {
  width: 1px;
  height: 16px;
  background: var(--wb-divider);
}

.wb-topbar__ghost-btn {
  display: inline-flex;
  flex-shrink: 0;
  gap: 4px;
  align-items: center;
  height: 28px;
  padding: 0 8px;
  font-size: 12px;
  font-weight: 500;
  color: var(--wb-text-muted);
  cursor: pointer;
  background: transparent;
  border: none;
  border-radius: var(--wb-radius);

  &:hover {
    color: var(--wb-text);
    background: var(--wb-surface-hover);
  }
}

.wb-topbar__name-input {
  min-width: 110px;
  max-width: 220px;
  height: 28px;
  padding: 0 8px;
  font-size: 13px;
  font-weight: 600;
  color: var(--wb-text);
  background: transparent;
  border: 1px solid transparent;
  border-radius: var(--wb-radius);
  outline: none;

  &:hover:not(:read-only) {
    background: var(--wb-surface-hover);
    border-color: var(--wb-divider);
  }

  &:focus:not(:read-only) {
    background: var(--wb-surface);
    border-color: var(--wb-primary);
  }
}

.wb-topbar__switcher-btn {
  display: inline-flex;
  flex-shrink: 0;
  gap: 6px;
  align-items: center;
  height: 26px;
  padding: 0 8px;
  font-size: 11.5px;
  font-weight: 500;
  color: var(--wb-text-muted);
  cursor: pointer;
  background: var(--wb-surface-subtle);
  border: 1px solid var(--wb-divider);
  border-radius: var(--wb-radius);

  &:hover {
    color: var(--wb-text);
    background: var(--wb-surface-hover);
  }
}

.wb-topbar__badge-dot {
  width: 6px;
  height: 6px;
  background: var(--wb-text-faint);
  border-radius: 50%;

  &.linked {
    background: var(--wb-success);
  }
}

.wb-topbar__switcher-text {
  max-width: 140px;
  overflow: hidden;
  text-overflow: ellipsis;
  font-family: var(--wb-font-mono);
  white-space: nowrap;
}

.wb-topbar__tag {
  display: inline-flex;
  flex-shrink: 0;
  align-items: center;
  height: 22px;
  padding: 0 6px;
  font-size: 11px;
  font-weight: 500;
  color: var(--wb-text-muted);
  background: var(--wb-surface-subtle);
  border: 1px solid var(--wb-divider);
  border-radius: var(--wb-radius);

  &[data-tone="success"] {
    color: var(--wb-success);
  }

  &[data-tone="warning"] {
    color: var(--wb-warning);
  }

  &[data-tone="danger"] {
    color: var(--wb-danger);
  }
}

.wb-env-ctl {
  display: inline-flex;
  align-items: stretch;
  height: 28px;
  overflow: hidden;
  background: var(--wb-surface-subtle);
  border: 1px solid var(--wb-divider);
  border-radius: var(--wb-radius);
}

.wb-env-ctl__main {
  display: inline-flex;
  gap: 6px;
  align-items: center;
  padding: 0 8px;
  font-size: 12px;
  font-weight: 500;
  color: var(--wb-text);
  cursor: pointer;
  background: transparent;
  border: none;

  &:hover {
    background: var(--wb-surface-hover);
  }
}

.wb-env-ctl__dot {
  width: 6px;
  height: 6px;
  background: var(--wb-text-faint);
  border-radius: 50%;

  &.active {
    background: var(--wb-success);
  }
}

.wb-env-ctl__label {
  max-width: 110px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.wb-env-ctl__count {
  padding: 0 4px;
  font-family: var(--wb-font-mono);
  font-size: 10.5px;
  color: var(--wb-text-muted);
  background: var(--wb-surface);
  border: 1px solid var(--wb-divider);
  border-radius: 3px;
}

.wb-env-ctl__peek {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 100%;
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

.wb-topbar__btn {
  display: inline-flex;
  gap: 5px;
  align-items: center;
  height: 28px;
  padding: 0 10px;
  font-size: 12px;
  font-weight: 500;
  color: var(--wb-text);
  cursor: pointer;
  background: var(--wb-surface);
  border: 1px solid var(--wb-divider);
  border-radius: var(--wb-radius);

  &:hover:not(:disabled) {
    background: var(--wb-surface-hover);
  }

  &:disabled {
    cursor: default;
    opacity: 0.55;
  }
}

.wb-topbar__icon-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  font-size: 14px;
  color: var(--wb-text-muted);
  cursor: pointer;
  background: var(--wb-surface);
  border: 1px solid var(--wb-divider);
  border-radius: var(--wb-radius);

  &:hover {
    color: var(--wb-text);
    background: var(--wb-surface-hover);
  }
}

.wb-topbar__gate {
  display: inline-flex;
  gap: 6px;
  align-items: center;
  height: 28px;
  padding: 0 10px;
  font-size: 12px;
  font-weight: 500;
  color: var(--wb-text-muted);
  cursor: pointer;
  background: var(--wb-surface-subtle);
  border: 1px solid var(--wb-divider);
  border-radius: var(--wb-radius);

  &:hover {
    color: var(--wb-text);
    background: var(--wb-surface-hover);
  }

  &.ready {
    color: var(--wb-success);
  }
}

.wb-topbar__gate-dot {
  width: 6px;
  height: 6px;
  background: var(--wb-warning);
  border-radius: 50%;
}

.wb-topbar__gate.ready .wb-topbar__gate-dot {
  background: var(--wb-success);
}

.wb-topbar__primary-btn {
  display: inline-flex;
  gap: 5px;
  align-items: center;
  height: 28px;
  padding: 0 12px;
  font-size: 12px;
  font-weight: 600;
  color: var(--wb-primary-fg);
  cursor: pointer;
  background: var(--wb-primary);
  border: 1px solid var(--wb-primary);
  border-radius: var(--wb-radius);
  transition:
    background-color 140ms ease,
    border-color 140ms ease;

  &:hover:not(:disabled) {
    background: var(--wb-primary-hover);
    border-color: var(--wb-primary-hover);
  }

  &:active:not(:disabled) {
    background: var(--wb-primary-active);
    border-color: var(--wb-primary-active);
  }

  &:disabled {
    cursor: default;
    opacity: 0.55;
  }
}

.wb-switcher-panel {
  min-width: 260px;
  max-width: 340px;
  max-height: 380px;
  overflow: auto;
}

.wb-switcher-new {
  display: flex;
  gap: 6px;
  align-items: center;
  font-weight: 600;
}

.wb-switcher-group {
  padding-top: 4px;
  margin-top: 4px;
  border-top: 1px solid var(--wb-divider);
}

.wb-switcher-group__title {
  padding: 4px 10px;
  font-size: 10.5px;
  font-weight: 600;
  color: var(--wb-text-faint);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.wb-switcher-row {
  display: flex;
  gap: 8px;
  align-items: center;
  justify-content: space-between;
}

.wb-switcher-row__name {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.wb-switcher-row__code {
  flex-shrink: 0;
  font-family: var(--wb-font-mono);
  font-size: 11px;
  color: var(--wb-text-faint);
}

.wb-env-menu {
  min-width: 220px;
}

.wb-env-menu__row {
  display: flex;
  gap: 8px;
  align-items: center;
  justify-content: space-between;
}

.wb-env-menu__n {
  font-family: var(--wb-font-mono);
  font-size: 11px;
  color: var(--wb-text-faint);
}

.wb-env-menu__create {
  display: flex;
  gap: 4px;
  padding: 6px 8px;
  margin-top: 4px;
  border-top: 1px solid var(--wb-divider);
}

.wb-env-menu__input {
  flex: 1;
  min-width: 0;
  height: 26px;
  padding: 0 8px;
  font-size: 12px;
  color: var(--wb-text);
  background: var(--wb-surface-input);
  border: 1px solid var(--wb-divider);
  border-radius: var(--wb-radius);
  outline: none;

  &:focus {
    background: var(--wb-surface);
    border-color: var(--wb-primary);
  }
}

.wb-env-menu__add {
  flex-shrink: 0;
  height: 26px;
  padding: 0 8px;
  font-size: 11.5px;
  font-weight: 600;
  color: var(--wb-primary-fg);
  cursor: pointer;
  background: var(--wb-primary);
  border: none;
  border-radius: var(--wb-radius);

  &:hover {
    background: var(--wb-primary-hover);
  }
}

.wb-env-peek {
  display: flex;
  flex-direction: column;
  gap: 6px;
  font-size: 12px;
}

.wb-env-peek__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding-bottom: 6px;
  font-weight: 600;
  border-bottom: 1px solid var(--wb-divider);
}

.wb-env-peek__empty {
  padding: 8px 0;
  color: var(--wb-text-faint);
}

.wb-env-peek__list {
  display: flex;
  flex-direction: column;
  gap: 4px;
  max-height: 220px;
  overflow: auto;
}

.wb-env-peek__row {
  display: flex;
  gap: 8px;
  align-items: center;
  justify-content: space-between;

  code {
    font-family: var(--wb-font-mono);
    font-size: 11.5px;
    color: var(--wb-json-key);
  }

  span {
    max-width: 160px;
    overflow: hidden;
    text-overflow: ellipsis;
    font-family: var(--wb-font-mono);
    font-size: 11.5px;
    white-space: nowrap;
  }
}
</style>
