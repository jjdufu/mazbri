<template>
  <span class="mfa-step-up-pill-host" :class="{ 'is-hidden': !visible }">
    <ElTooltip :content="tooltip" placement="bottom">
      <button
        type="button"
        class="mfa-step-up-pill"
        :class="pillClass"
        :aria-label="ariaLabel"
        @click="onClick"
      >
        <ArtSvgIcon
          icon="ri:shield-keyhole-line"
          class="mfa-step-up-pill__icon"
        />
        <span class="mfa-step-up-pill__text">{{ label }}</span>
      </button>
    </ElTooltip>
    <span class="mfa-step-up-pill__live" aria-live="polite">{{ liveText }}</span>
  </span>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { ElMessageBox } from "element-plus";
import { storeToRefs } from "pinia";
import { useMfaStepUpStore } from "@/store/modules/mfa-step-up";
import {
  ensureStepUp,
  isMfaStepUpCancelled,
} from "@/utils/http/mfa-dialog-service";
import {
  formatExpiryUtc8,
  formatMmSs,
  isWarning,
} from "@/utils/http/mfa-step-up-state";
import { HttpError, showError } from "@/utils/http/error";

defineOptions({ name: "ArtMfaStepUpPill" });

type PillPhase = "hidden" | "inactive" | "active" | "warning";

const { t } = useI18n();
const store = useMfaStepUpStore();
const { mfaEnabled, loaded, active, expiresAt, clientDeadline } =
  storeToRefs(store);
const now = ref(Date.now());
const liveText = ref("");
let timer: ReturnType<typeof setInterval> | null = null;

onMounted(() => {
  now.value = Date.now();
  timer = setInterval(() => {
    now.value = Date.now();
  }, 1000);
  if (!loaded.value) void store.refresh().catch(() => undefined);
});

onUnmounted(() => {
  if (timer) clearInterval(timer);
});

watch(clientDeadline, () => {
  now.value = Date.now();
});

const remaining = computed(() => {
  if (!active.value || clientDeadline.value == null) return 0;
  return Math.max(0, Math.ceil((clientDeadline.value - now.value) / 1000));
});

const phase = computed<PillPhase>(() => {
  if (!loaded.value || !mfaEnabled.value) return "hidden";
  if (!active.value || remaining.value <= 0) return "inactive";
  if (isWarning(remaining.value)) return "warning";
  return "active";
});

const visible = computed(() => phase.value !== "hidden");
const pillClass = computed(() => ({
  "is-inactive": phase.value === "inactive",
  "is-active": phase.value === "active",
  "is-warning": phase.value === "warning",
}));
const label = computed(() => {
  if (phase.value === "inactive") return t("common.mfaStepUp.inactive");
  return formatMmSs(remaining.value);
});
const ariaLabel = computed(() => {
  if (phase.value === "warning") return t("common.mfaStepUp.ariaWarning");
  if (phase.value === "active") return t("common.mfaStepUp.ariaActive");
  return t("common.mfaStepUp.ariaInactive");
});
const tooltip = computed(() => {
  const expiry = expiresAt.value ? formatExpiryUtc8(expiresAt.value) : "";
  if (phase.value === "warning" && expiry) {
    return t("common.mfaStepUp.tooltipWarning", { time: expiry });
  }
  if (phase.value === "active" && expiry) {
    return t("common.mfaStepUp.tooltipActive", { time: expiry });
  }
  if ((phase.value === "active" || phase.value === "warning") && !expiry) {
    return t("common.mfaStepUp.tooltipNoExpiry");
  }
  return t("common.mfaStepUp.tooltipInactive");
});

watch(phase, (next, prev) => {
  if (prev == null || next === prev) return;
  if (next === "hidden") {
    liveText.value = t("common.mfaStepUp.liveHidden");
    return;
  }
  if (next === "warning") {
    liveText.value = t("common.mfaStepUp.liveWarning");
    return;
  }
  if (next === "active") {
    liveText.value = t("common.mfaStepUp.liveActive");
    return;
  }
  liveText.value = t("common.mfaStepUp.liveInactive");
});

async function onClick(): Promise<void> {
  if (phase.value === "inactive") {
    try {
      await ensureStepUp();
    } catch (error) {
      if (isMfaStepUpCancelled(error)) return;
      if (error instanceof HttpError) showError(error, true);
    }
    return;
  }
  try {
    await ElMessageBox.confirm(
      t("common.mfaStepUp.lockMessage"),
      t("common.mfaStepUp.lockTitle"),
      {
        type: "warning",
        confirmButtonText: t("common.confirm"),
        cancelButtonText: t("common.cancel"),
        closeOnClickModal: false,
      },
    );
  } catch {
    return;
  }
  try {
    await store.lock();
  } catch (error) {
    if (error instanceof HttpError) showError(error, true);
  }
}
</script>

<style scoped lang="scss">
.mfa-step-up-pill-host {
  display: inline-flex;
  align-items: center;

  &.is-hidden {
    display: none;
  }
}

.mfa-step-up-pill {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 34px;
  padding: 0 10px;
  border: 1px solid var(--el-border-color);
  border-radius: 999px;
  background: var(--el-fill-color);
  color: var(--el-text-color-secondary);
  font-size: 12px;
  font-variant-numeric: tabular-nums;
  line-height: 1;
  cursor: pointer;

  &.is-active {
    color: var(--el-color-success);
    background: var(--el-color-success-light-9);
    border-color: var(--el-color-success-light-5);
  }

  &.is-warning {
    color: var(--el-color-warning);
    background: var(--el-color-warning-light-9);
    border-color: var(--el-color-warning-light-5);
  }
}

.mfa-step-up-pill__icon {
  font-size: 14px;
}

.mfa-step-up-pill__live {
  position: absolute;
  width: 1px;
  height: 1px;
  padding: 0;
  margin: -1px;
  overflow: hidden;
  clip: rect(0, 0, 0, 0);
  white-space: nowrap;
  border: 0;
}
</style>
