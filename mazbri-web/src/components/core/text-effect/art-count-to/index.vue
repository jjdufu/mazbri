<template>
  <span
    class="text-g-900 tabular-nums"
    :class="isRunning ? 'transition-opacity duration-300 ease-in-out' : ''"
  >
    {{ formattedValue }}
  </span>
</template>

<script setup lang="ts">
import { computed, watch, nextTick, onUnmounted, shallowRef } from "vue";
import { useTransition, TransitionPresets } from "@vueuse/core";
import { ErrorCodes } from "@/constants/error-codes";
import { logger } from "@/utils/sys";
interface CountToProps {
  target: number;
  duration?: number;
  autoStart?: boolean;
  decimals?: number;
  decimal?: string;
  separator?: string;
  prefix?: string;
  suffix?: string;
  easing?: keyof typeof TransitionPresets;
  disabled?: boolean;
}
interface CountToEmits {
  started: [value: number];
  finished: [value: number];
  paused: [value: number];
  reset: [];
}
interface CountToExpose {
  start: (target?: number) => void;
  pause: () => void;
  reset: (newTarget?: number) => void;
  stop: () => void;
  setTarget: (target: number) => void;
  readonly isRunning: boolean;
  readonly isPaused: boolean;
  readonly currentValue: number;
  readonly targetValue: number;
  readonly progress: number;
}
const EPSILON = Number.EPSILON;
const MIN_DURATION = 100;
const MAX_DURATION = 60000;
const MAX_DECIMALS = 10;
const DEFAULT_EASING = "easeOutExpo";
const DEFAULT_DURATION = 2000;
const props = withDefaults(defineProps<CountToProps>(), {
  target: 0,
  duration: DEFAULT_DURATION,
  autoStart: true,
  decimals: 0,
  decimal: ".",
  separator: "",
  prefix: "",
  suffix: "",
  easing: DEFAULT_EASING,
  disabled: false,
});
const emit = defineEmits<CountToEmits>();
const validateNumber = (
  value: number,
  name: string,
  defaultValue: number,
): number => {
  if (!Number.isFinite(value)) {
    logger.warn("CountTo received invalid numeric prop", {
      service: "mazbri-mgmt-web",
      event: "count_to_invalid_number",
      severity: "warn",
      error_code: ErrorCodes.VALIDATION_ERROR,
      prop_name: name,
      value,
    });
    return defaultValue;
  }
  return value;
};
const clamp = (value: number, min: number, max: number): number => {
  return Math.max(min, Math.min(value, max));
};
const formatNumber = (
  value: number,
  decimals: number,
  decimal: string,
  separator: string,
): string => {
  let result =
    decimals > 0 ? value.toFixed(decimals) : Math.floor(value).toString();
  if (decimal !== "." && result.includes(".")) {
    result = result.replace(".", decimal);
  }
  if (separator) {
    const parts = result.split(decimal);
    parts[0] = parts[0].replace(/\B(?=(\d{3})+(?!\d))/g, separator);
    result = parts.join(decimal);
  }
  return result;
};
const safeTarget = computed(() => validateNumber(props.target, "target", 0));
const safeDuration = computed(() =>
  clamp(
    validateNumber(props.duration, "duration", DEFAULT_DURATION),
    MIN_DURATION,
    MAX_DURATION,
  ),
);
const safeDecimals = computed(() =>
  clamp(validateNumber(props.decimals, "decimals", 0), 0, MAX_DECIMALS),
);
const safeEasing = computed(() => {
  const easing = props.easing;
  if (!(easing in TransitionPresets)) {
    logger.warn("CountTo received invalid easing preset", {
      service: "mazbri-mgmt-web",
      event: "count_to_invalid_easing",
      severity: "warn",
      error_code: ErrorCodes.VALIDATION_ERROR,
      easing,
    });
    return DEFAULT_EASING;
  }
  return easing;
});
const currentValue = shallowRef(0);
const targetValue = shallowRef(safeTarget.value);
const isRunning = shallowRef(false);
const isPaused = shallowRef(false);
const pausedValue = shallowRef(0);
const transitionValue = useTransition(currentValue, {
  duration: safeDuration,
  transition: computed(() => TransitionPresets[safeEasing.value]),
  onStarted: () => {
    isRunning.value = true;
    isPaused.value = false;
    emit("started", targetValue.value);
  },
  onFinished: () => {
    isRunning.value = false;
    isPaused.value = false;
    emit("finished", targetValue.value);
  },
});
const formattedValue = computed(() => {
  const value = isPaused.value ? pausedValue.value : transitionValue.value;
  if (!Number.isFinite(value)) {
    return `${props.prefix}0${props.suffix}`;
  }
  const formattedNumber = formatNumber(
    value,
    safeDecimals.value,
    props.decimal,
    props.separator,
  );
  return `${props.prefix}${formattedNumber}${props.suffix}`;
});
const shouldSkipAnimation = (target: number): boolean => {
  const current = isPaused.value ? pausedValue.value : transitionValue.value;
  return Math.abs(current - target) < EPSILON;
};
const resetPauseState = (): void => {
  isPaused.value = false;
  pausedValue.value = 0;
};
const start = (target?: number): void => {
  if (props.disabled) {
    logger.warn("CountTo animation start skipped because it is disabled", {
      service: "mazbri-mgmt-web",
      event: "count_to_animation_disabled",
      severity: "warn",
      error_code: ErrorCodes.VALIDATION_ERROR,
    });
    return;
  }
  const finalTarget = target !== undefined ? target : targetValue.value;
  if (!Number.isFinite(finalTarget)) {
    logger.warn("CountTo received invalid start target", {
      service: "mazbri-mgmt-web",
      event: "count_to_invalid_start_target",
      severity: "warn",
      error_code: ErrorCodes.VALIDATION_ERROR,
      target: finalTarget,
    });
    return;
  }
  targetValue.value = finalTarget;
  if (shouldSkipAnimation(finalTarget)) {
    return;
  }
  if (isPaused.value) {
    currentValue.value = pausedValue.value;
    resetPauseState();
  }
  nextTick(() => {
    currentValue.value = finalTarget;
  });
};
const pause = (): void => {
  if (!isRunning.value || isPaused.value) {
    return;
  }
  isPaused.value = true;
  pausedValue.value = transitionValue.value;
  currentValue.value = pausedValue.value;
  emit("paused", pausedValue.value);
};
const reset = (newTarget = 0): void => {
  const target = validateNumber(newTarget, "reset target", 0);
  currentValue.value = target;
  targetValue.value = target;
  resetPauseState();
  emit("reset");
};
const setTarget = (target: number): void => {
  if (!Number.isFinite(target)) {
    logger.warn("CountTo received invalid setTarget value", {
      service: "mazbri-mgmt-web",
      event: "count_to_invalid_set_target",
      severity: "warn",
      error_code: ErrorCodes.VALIDATION_ERROR,
      target,
    });
    return;
  }
  targetValue.value = target;
  if ((isRunning.value || props.autoStart) && !props.disabled) {
    start(target);
  }
};
const stop = (): void => {
  if (isRunning.value || isPaused.value) {
    currentValue.value = 0;
    resetPauseState();
    emit("paused", 0);
  }
};
watch(
  safeTarget,
  (newTarget) => {
    if (props.autoStart && !props.disabled) {
      start(newTarget);
    } else {
      targetValue.value = newTarget;
    }
  },
  { immediate: props.autoStart && !props.disabled },
);
watch(
  () => props.disabled,
  (disabled) => {
    if (disabled && isRunning.value) {
      stop();
    }
  },
);
onUnmounted(() => {
  if (isRunning.value) {
    stop();
  }
});
defineExpose<CountToExpose>({
  start,
  pause,
  reset,
  stop,
  setTarget,
  get isRunning() {
    return isRunning.value;
  },
  get isPaused() {
    return isPaused.value;
  },
  get currentValue() {
    return isPaused.value ? pausedValue.value : transitionValue.value;
  },
  get targetValue() {
    return targetValue.value;
  },
  get progress() {
    const current = isPaused.value ? pausedValue.value : transitionValue.value;
    const target = targetValue.value;
    if (target === 0) return current === 0 ? 1 : 0;
    return Math.abs(current / target);
  },
});
</script>
