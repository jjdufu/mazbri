<template>
  <!-- [不可更改] 运维页统一紧凑自动刷新分体按钮：左键立即局部刷新，右键下拉选择自动刷新周期，严禁退化为宽下拉框 -->
  <div
    class="ops-auto-refresh"
    :class="{ 'is-active': interval > 0 }"
  >
    <button
      type="button"
      class="ops-auto-refresh__trigger"
      :title="t('ops.autoRefresh.refreshNow')"
      :aria-label="t('ops.autoRefresh.refreshNow')"
      @click="onManualRefresh"
    >
      <svg
        class="ops-auto-refresh__icon"
        :class="{ 'is-spinning': spinning }"
        viewBox="0 0 24 24"
        width="14"
        height="14"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
        aria-hidden="true"
      >
        <path d="M21 12a9 9 0 1 1-2.64-6.36" />
        <polyline points="21 3 21 9 15 9" />
      </svg>
      <span v-if="activeShortLabel" class="ops-auto-refresh__badge">
        {{ activeShortLabel }}
      </span>
    </button>

    <ElDropdown
      trigger="click"
      placement="bottom-end"
      @command="onSelectInterval"
    >
      <button
        type="button"
        class="ops-auto-refresh__caret"
        :title="t('ops.autoRefresh.label')"
        :aria-label="t('ops.autoRefresh.label')"
      >
        <svg
          viewBox="0 0 24 24"
          width="12"
          height="12"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <polyline points="6 9 12 15 18 9" />
        </svg>
      </button>
      <template #dropdown>
        <ElDropdownMenu class="ops-auto-refresh__menu">
          <ElDropdownItem
            v-for="opt in options"
            :key="opt.value"
            :command="opt.value"
            :class="{ 'is-selected': interval === opt.value }"
          >
            <span>{{ opt.label }}</span>
          </ElDropdownItem>
        </ElDropdownMenu>
      </template>
    </ElDropdown>
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";

const props = withDefaults(
  defineProps<{
    modelValue?: number;
    /** Called on each tick when interval > 0 or when user clicks manual refresh. */
  }>(),
  { modelValue: 0 },
);

const emit = defineEmits<{
  "update:modelValue": [number];
  tick: [];
  change: [number];
}>();

const { t } = useI18n();
const interval = ref(props.modelValue);
const spinning = ref(false);
let timer: ReturnType<typeof setInterval> | null = null;
let spinTimer: ReturnType<typeof setTimeout> | null = null;

const options = computed(() => [
  { label: t("ops.autoRefresh.off"), shortLabel: "", value: 0 },
  { label: t("ops.autoRefresh.tenSeconds"), shortLabel: "10s", value: 10 },
  { label: t("ops.autoRefresh.thirtySeconds"), shortLabel: "30s", value: 30 },
  { label: t("ops.autoRefresh.oneMinute"), shortLabel: "1m", value: 60 },
]);

const activeShortLabel = computed(() => {
  const matched = options.value.find((opt) => opt.value === interval.value);
  if (matched) return matched.shortLabel;
  return interval.value > 0 ? `${interval.value}s` : "";
});

function clearTimer() {
  if (timer) {
    clearInterval(timer);
    timer = null;
  }
}

function clearSpinTimer() {
  if (spinTimer) {
    clearTimeout(spinTimer);
    spinTimer = null;
  }
}

function onTimerTick() {
  // 断网时跳过本轮，避免每个周期都制造一批必然失败的请求
  if (navigator.onLine === false) return;
  emit("tick");
}

function onOnline() {
  if (interval.value > 0) {
    emit("tick");
    arm();
  }
}

function arm() {
  clearTimer();
  if (interval.value > 0) {
    timer = setInterval(onTimerTick, interval.value * 1000);
  }
}

function onManualRefresh() {
  spinning.value = false;
  clearSpinTimer();
  requestAnimationFrame(() => {
    spinning.value = true;
    spinTimer = setTimeout(() => {
      spinning.value = false;
      spinTimer = null;
    }, 500);
  });
  emit("tick");
  if (interval.value > 0) {
    arm();
  }
}

function onSelectInterval(nextValue: number | string) {
  const numeric = Number(nextValue);
  if (!Number.isFinite(numeric)) return;
  interval.value = numeric;
  emit("update:modelValue", interval.value);
  emit("change", interval.value);
  arm();
}

watch(
  () => props.modelValue,
  (v) => {
    if (v !== interval.value) {
      interval.value = v;
      arm();
    }
  },
);

onMounted(() => {
  arm();
  window.addEventListener("online", onOnline);
});
onBeforeUnmount(() => {
  window.removeEventListener("online", onOnline);
  clearTimer();
  clearSpinTimer();
});
</script>

<style scoped lang="scss">
.ops-auto-refresh {
  display: inline-flex;
  align-items: stretch;
  height: 32px;
  box-sizing: border-box;
  border: 1px solid var(--art-form-control-border, var(--el-border-color));
  border-radius: var(--art-form-control-radius);
  background-color: var(--art-form-control-bg, var(--el-fill-color-blank));
  overflow: hidden;
  flex-shrink: 0;
  transition:
    border-color 0.15s ease,
    background-color 0.15s ease;

  &:hover {
    border-color: var(--el-border-color-hover);
  }

  &.is-active {
    border-color: var(--el-color-primary-light-5);
    background-color: var(--el-color-primary-light-9);

    .ops-auto-refresh__trigger,
    .ops-auto-refresh__caret {
      color: var(--el-color-primary);
    }

    .ops-auto-refresh__caret {
      border-left-color: var(--el-color-primary-light-7);
    }
  }
}

.ops-auto-refresh__trigger,
.ops-auto-refresh__caret {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border: none;
  background: transparent;
  color: var(--el-text-color-regular);
  cursor: pointer;
  padding: 0;
  margin: 0;
  font-size: 12px;
  line-height: 1;
  outline: none;
  transition:
    color 0.15s ease,
    background-color 0.15s ease;

  &:hover {
    color: var(--el-color-primary);
    background-color: var(--el-fill-color-light);
  }

  &:active {
    background-color: var(--el-fill-color);
  }
}

.ops-auto-refresh__trigger {
  gap: 5px;
  padding: 0 9px;
  min-width: 32px;
}

.ops-auto-refresh__caret {
  width: 22px;
  border-left: 1px solid var(--art-form-control-border, var(--el-border-color-lighter));
}

.ops-auto-refresh__icon.is-spinning {
  animation: ops-auto-refresh-spin 0.48s cubic-bezier(0.4, 0, 0.2, 1);
}

.ops-auto-refresh__badge {
  font-size: 12px;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
}

:deep(.ops-auto-refresh__menu .el-dropdown-menu__item.is-selected) {
  color: var(--el-color-primary) !important;
  font-weight: 600;
  background-color: var(--el-color-primary-light-9) !important;
}

@keyframes ops-auto-refresh-spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}
</style>
