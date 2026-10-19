<template>
  <div class="wb-auth">
    <div class="wb-hopp-sticky">
      <span class="wb-hopp-sticky__label">{{ $t("upstreamConnector.authType") }}</span>
      <WorkbenchHoppSelect
        :model-value="picker"
        :options="pickerOptions"
        :append-to="appendTo"
        @update:model-value="onPick"
      />
      <div class="wb-hopp-sticky__session">
        <span class="wb-hopp-sticky__sess-label">
          {{ $t("upstreamConnector.authSessionDefault") }}:
          {{ sessionLabel }}
        </span>
        <button
          type="button"
          class="wb-hopp-action"
          :disabled="!canSetSession"
          @click="setSessionDefault"
        >
          {{ $t("upstreamConnector.authSetSessionDefault") }}
        </button>
      </div>
      <div class="wb-hopp-sticky__right">
        <label class="wb-hopp-check">
          <input
            type="checkbox"
            :checked="flow.enabled"
            @change="onEnabledChange"
          />
          {{ $t("upstreamConnector.authEnabled") }}
        </label>
        <button
          type="button"
          class="wb-hopp-icon"
          :title="$t('upstreamConnector.authClear')"
          @click="clearProfile"
        >
          <ArtSvgIcon icon="ri:delete-bin-line" />
        </button>
        <button
          type="button"
          class="wb-hopp-action"
          :disabled="running"
          @click="emit('run')"
        >
          {{ $t("upstreamConnector.runAuth") }}
        </button>
      </div>
    </div>

    <p
      v-if="flow.enabled && !sessionProfileLabel(flow.profile)"
      class="wb-auth__warn"
    >
      {{ $t("upstreamConnector.authInheritHint") }}
    </p>

    <AuthProfileFields
      :picker="picker"
      :flow="flow"
      :adapter="adapter"
      :env="env"
      :seed-json="seedJson"
      :append-to="appendTo"
    />
  </div>
</template>

<script setup lang="ts">
import { $t } from "@/locales";
import type { AuthFlowForm } from "@/views/mazbri/suppliers/components/SupplierTransparentConfig.vue";
import {
  type AuthPickerId,
  authProfileFromPicker,
  authPickerFromState,
  applyPickerDefaults,
  sessionProfileLabel,
  visibleAuthPickers,
} from "../authMatrix";
import AuthProfileFields from "./AuthProfileFields.vue";
import WorkbenchHoppSelect from "./WorkbenchHoppSelect.vue";

const props = withDefaults(
  defineProps<{
    profile: string;
    flow: AuthFlowForm;
    adapter?: string;
    env?: Record<string, string>;
    seedJson?: string;
    appendTo?: HTMLElement | string;
    running?: boolean;
  }>(),
  {
    adapter: "",
    env: () => ({}),
    seedJson: "",
    appendTo: undefined,
    running: false,
  },
);

const emit = defineEmits<{
  "update:profile": [string];
  run: [];
}>();

const picker = computed(() =>
  authPickerFromState(props.profile, props.flow.enabled),
);

const pickerLabel = (id: AuthPickerId): string => {
  switch (id) {
    case "inherit":
      return $t("upstreamConnector.authInherit");
    case "none":
      return $t("upstreamConnector.authNone");
    case "api_key":
      return $t("upstreamConnector.authApiKey");
    case "oauth_cc_form":
      return $t("upstreamConnector.authOauthCc");
    case "gettoken":
      return $t("upstreamConnector.authGettoken");
    case "basic":
      return $t("upstreamConnector.authBasic");
    case "sign_form":
      return $t("upstreamConnector.authSignForm");
    case "grpc_metadata":
      return $t("upstreamConnector.authGrpcMeta");
    case "bearer":
      return $t("upstreamConnector.authBearer");
    case "jwt":
      return $t("upstreamConnector.authJwt");
    case "digest":
      return $t("upstreamConnector.authDigest");
    case "aws_sigv4":
      return $t("upstreamConnector.authAws");
    case "hawk":
      return $t("upstreamConnector.authHawk");
    default:
      return id;
  }
};

const pickerOptions = computed(() =>
  visibleAuthPickers(props.flow.enabled).map((id) => ({
    label: pickerLabel(id),
    value: id,
  })),
);

const sessionLabel = computed(() => {
  const p = sessionProfileLabel(props.flow.profile);
  return p ? pickerLabel(p as AuthPickerId) : $t("upstreamConnector.authSessionUnset");
});

const canSetSession = computed(() => {
  const id = picker.value;
  return id !== "inherit" && id !== "none";
});

function onPick(value: string) {
  const next = value as AuthPickerId;
  emit("update:profile", authProfileFromPicker(next));
  applyPickerDefaults(next, props.flow);
}

function onEnabledChange(ev: Event) {
  props.flow.enabled = (ev.target as HTMLInputElement).checked;
}

function clearProfile() {
  emit("update:profile", "none");
}

function setSessionDefault() {
  const id = picker.value;
  if (id === "inherit" || id === "none") return;
  props.flow.profile = id;
}
</script>

<style scoped lang="scss">
.wb-auth {
  display: flex;
  flex-direction: column;
  min-height: 0;
}
.wb-hopp-sticky {
  position: sticky;
  top: 0;
  z-index: 2;
  display: flex;
  align-items: center;
  gap: 4px;
  min-height: 36px;
  padding: 0 4px 0 0;
  background: var(--wb-surface, var(--el-bg-color));
  border-bottom: 1px solid var(--wb-divider-light, var(--el-border-color-extra-light));
}
.wb-hopp-sticky__label {
  box-sizing: border-box;
  flex: 0 0 var(--wb-label-col, 10.5rem);
  width: var(--wb-label-col, 10.5rem);
  font-size: 12px;
  font-weight: 600;
  color: var(--wb-text-muted, var(--el-text-color-secondary));
  padding: 0 12px 0 16px;
  white-space: nowrap;
  overflow: visible;
}
.wb-hopp-sticky__session {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--wb-text-muted, var(--el-text-color-secondary));
  min-width: 0;
}
.wb-hopp-sticky__sess-label {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.wb-hopp-sticky__right {
  margin-left: auto;
  display: flex;
  align-items: center;
  gap: 4px;
}
.wb-hopp-check {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--wb-text-muted, var(--el-text-color-secondary));
  padding: 0 8px;
  cursor: pointer;
  user-select: none;
  input {
    margin: 0;
  }
}
.wb-hopp-icon {
  width: 32px;
  height: 32px;
  border: none;
  background: transparent;
  color: var(--wb-text-muted, var(--el-text-color-secondary));
  border-radius: var(--wb-radius, var(--art-form-control-radius));
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  &:hover {
    color: var(--wb-text, var(--el-text-color-primary));
    background: var(--wb-surface-hover, var(--el-fill-color));
  }
}
.wb-auth__warn {
  margin: 8px 16px 0;
  font-size: 12px;
  color: var(--el-color-warning);
}
</style>
