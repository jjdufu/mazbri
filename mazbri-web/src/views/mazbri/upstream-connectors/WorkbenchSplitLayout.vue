<template>
  <div
    ref="rootRef"
    class="wb-split-layout"
    :class="[
      `is-${effectiveDirection}`,
      { 'is-collapsed': collapsed },
    ]"
  >
    <div class="wb-split-layout__pane wb-split-layout__pane--primary">
      <slot name="top" />
    </div>
    <div
      class="wb-split-layout__gutter"
      :class="{ 'is-collapsed': collapsed }"
      @mousedown.prevent="startDrag"
      @dblclick.prevent="toggle"
    >
      <button
        type="button"
        class="wb-split-toggle"
        :title="
          collapsed
            ? $t('upstreamConnector.expandResponse')
            : $t('upstreamConnector.collapseResponse')
        "
        :aria-label="
          collapsed
            ? $t('upstreamConnector.expandResponse')
            : $t('upstreamConnector.collapseResponse')
        "
        :aria-expanded="!collapsed"
        @click.stop="toggle"
        @mousedown.stop
      >
        <ArtSvgIcon :icon="toggleIcon" />
      </button>
    </div>
    <div
      class="wb-split-layout__pane wb-split-layout__pane--secondary"
      :class="{ 'is-collapsed': collapsed }"
      :style="secondaryStyle"
    >
      <div v-show="!collapsed" class="wb-split-layout__body">
        <slot name="bottom" />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { $t } from "@/locales";
import {
  SPLIT_DEFAULT_RATIO,
  SPLIT_DEFAULT_RIGHT,
  SPLIT_LAYOUT_STORAGE_KEY,
  VSPLIT_COLLAPSED_HEIGHT,
  VSPLIT_DEFAULT_BOTTOM,
  parseSplitLayoutState,
  parseVSplitState,
  type SplitDirection,
} from "./workbenchHelpers";

const WIDE_BREAKPOINT = 1280;

const props = withDefaults(
  defineProps<{
    minBottom?: number;
    minTop?: number;
    minRight?: number;
    minLeft?: number;
  }>(),
  {
    minBottom: 220,
    minTop: 200,
    minRight: 360,
    minLeft: 400,
  },
);

const direction = defineModel<SplitDirection>("direction", {
  default: "horizontal",
});

const rootRef = ref<HTMLElement | null>(null);
const bottomH = ref(VSPLIT_DEFAULT_BOTTOM);
const rightW = ref(SPLIT_DEFAULT_RIGHT);
const ratio = ref(SPLIT_DEFAULT_RATIO);
const collapsed = ref(false);
const hasExplicitSize = ref(false);
const viewportW = ref(
  typeof window !== "undefined" ? window.innerWidth : WIDE_BREAKPOINT,
);

const canSideBySide = computed(() => viewportW.value >= WIDE_BREAKPOINT);
const effectiveDirection = computed<SplitDirection>(() =>
  direction.value === "vertical" && canSideBySide.value
    ? "vertical"
    : "horizontal",
);

const toggleIcon = computed(() => {
  if (effectiveDirection.value === "vertical") {
    return collapsed.value ? "ri:arrow-left-s-line" : "ri:arrow-right-s-line";
  }
  return collapsed.value ? "ri:arrow-up-s-line" : "ri:arrow-down-s-line";
});

const secondaryStyle = computed(() => {
  if (effectiveDirection.value === "vertical") {
    if (collapsed.value) {
      return {
        width: `${VSPLIT_COLLAPSED_HEIGHT}px`,
        minWidth: `${VSPLIT_COLLAPSED_HEIGHT}px`,
      };
    }
    return {
      width: `${rightW.value}px`,
      minWidth: `${props.minRight}px`,
    };
  }
  if (collapsed.value) {
    return {
      height: `${VSPLIT_COLLAPSED_HEIGHT}px`,
      minHeight: `${VSPLIT_COLLAPSED_HEIGHT}px`,
    };
  }
  return {
    height: `${Math.max(bottomH.value, props.minBottom)}px`,
    minHeight: `${props.minBottom}px`,
  };
});

function syncInitialRatioSize() {
  const el = rootRef.value;
  if (!el || hasExplicitSize.value) return;
  if (effectiveDirection.value === "horizontal" && el.clientHeight > 0) {
    const target = Math.round(el.clientHeight * ratio.value);
    const max = Math.max(el.clientHeight - props.minTop, props.minBottom);
    bottomH.value = Math.min(Math.max(target, props.minBottom), max);
  } else if (effectiveDirection.value === "vertical" && el.clientWidth > 0) {
    const target = Math.round(el.clientWidth * Math.max(ratio.value, 0.42));
    const max = Math.max(el.clientWidth - props.minLeft, props.minRight);
    rightW.value = Math.min(Math.max(target, props.minRight), max);
  }
}

function onWinResize() {
  viewportW.value = window.innerWidth;
  syncInitialRatioSize();
}

onMounted(() => {
  try {
    const raw = localStorage.getItem(SPLIT_LAYOUT_STORAGE_KEY);
    if (raw) {
      const st = parseSplitLayoutState(raw);
      direction.value = st.direction;
      ratio.value = st.ratio;
      bottomH.value = Math.max(st.bottom, props.minBottom);
      rightW.value = Math.max(st.right, props.minRight);
      collapsed.value = st.collapsed;
      hasExplicitSize.value = true;
    } else {
      const legacy = localStorage.getItem("mazbri:wb:vsplit");
      if (legacy) {
        const v = parseVSplitState(legacy);
        bottomH.value = Math.max(v.bottom, props.minBottom);
        collapsed.value = v.collapsed;
        hasExplicitSize.value = true;
        localStorage.removeItem("mazbri:wb:vsplit");
        persist();
      } else {
        const st = parseSplitLayoutState(null);
        direction.value = st.direction;
        ratio.value = st.ratio;
        bottomH.value = Math.max(st.bottom, props.minBottom);
        rightW.value = Math.max(st.right, props.minRight);
        collapsed.value = st.collapsed;
        hasExplicitSize.value = false;
      }
    }
  } catch {
    void 0;
  }
  window.addEventListener("resize", onWinResize);
  void nextTick(syncInitialRatioSize);
});

onBeforeUnmount(() => {
  window.removeEventListener("resize", onWinResize);
});

function persist() {
  try {
    localStorage.setItem(
      SPLIT_LAYOUT_STORAGE_KEY,
      JSON.stringify({
        direction: direction.value,
        ratio: ratio.value,
        bottom: bottomH.value,
        right: rightW.value,
        collapsed: collapsed.value,
      }),
    );
  } catch {
    void 0;
  }
}

function toggle() {
  collapsed.value = !collapsed.value;
  persist();
}

function expand() {
  if (!collapsed.value) return;
  collapsed.value = false;
  persist();
}

function toggleDirection() {
  direction.value =
    effectiveDirection.value === "horizontal" ? "vertical" : "horizontal";
  if (collapsed.value) collapsed.value = false;
  void nextTick(() => {
    const el = rootRef.value;
    if (el) {
      if (effectiveDirection.value === "vertical" && el.clientWidth > 0) {
        const max = Math.max(el.clientWidth - props.minLeft, props.minRight);
        rightW.value = Math.min(
          Math.max(rightW.value, Math.round(el.clientWidth * 0.42), props.minRight),
          max,
        );
      } else if (effectiveDirection.value === "horizontal" && el.clientHeight > 0) {
        const max = Math.max(el.clientHeight - props.minTop, props.minBottom);
        bottomH.value = Math.min(Math.max(bottomH.value, props.minBottom), max);
      }
    }
    persist();
  });
}

function startDrag(ev: MouseEvent) {
  const isVert = effectiveDirection.value === "vertical";
  const startPos = isVert ? ev.clientX : ev.clientY;
  const startSize = collapsed.value
    ? isVert
      ? props.minRight
      : props.minBottom
    : isVert
      ? rightW.value
      : bottomH.value;
  const total = isVert
    ? rootRef.value?.clientWidth || 0
    : rootRef.value?.clientHeight || 0;
  let moved = false;

  const onMove = (e: MouseEvent) => {
    const delta = (isVert ? e.clientX : e.clientY) - startPos;
    if (!moved && Math.abs(delta) < 3) return;
    moved = true;
    hasExplicitSize.value = true;
    if (collapsed.value) collapsed.value = false;
    const next = startSize - delta;
    if (isVert) {
      const max = Math.max(total - props.minLeft, props.minRight);
      rightW.value = Math.min(Math.max(next, props.minRight), max);
      if (total > 0) ratio.value = Number((rightW.value / total).toFixed(3));
    } else {
      const max = Math.max(total - props.minTop, props.minBottom);
      bottomH.value = Math.min(Math.max(next, props.minBottom), max);
      if (total > 0) ratio.value = Number((bottomH.value / total).toFixed(3));
    }
  };
  const onUp = () => {
    window.removeEventListener("mousemove", onMove);
    window.removeEventListener("mouseup", onUp);
    persist();
  };
  window.addEventListener("mousemove", onMove);
  window.addEventListener("mouseup", onUp);
}

defineExpose({ expand, toggle, toggleDirection, canSideBySide, effectiveDirection });
</script>

<style scoped lang="scss">
.wb-split-layout {
  display: flex;
  flex-direction: column;
  min-height: 0;
  min-width: 0;
  flex: 1;
  width: 100%;
  height: 100%;
  &.is-vertical {
    flex-direction: row;
  }
}
.wb-split-layout__pane {
  min-width: 0;
  min-height: 0;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}
.wb-split-layout__pane--primary {
  flex: 1;
  min-height: 0;
  min-width: 0;
}
.wb-split-layout__pane--secondary {
  flex-shrink: 0;
}
.wb-split-layout.is-vertical .wb-split-layout__pane--secondary {
  height: 100%;
}
.wb-split-layout__gutter {
  position: relative;
  flex-shrink: 0;
  background: transparent;
}
.wb-split-layout.is-horizontal > .wb-split-layout__gutter {
  height: 8px;
  cursor: row-resize;
  &.is-collapsed {
    height: 16px;
    cursor: pointer;
  }
  &::after {
    content: "";
    position: absolute;
    left: 0;
    right: 0;
    top: 50%;
    height: 2px;
    margin-top: -1px;
    background: var(--el-border-color-lighter);
  }
}
.wb-split-layout.is-vertical > .wb-split-layout__gutter {
  width: 8px;
  cursor: col-resize;
  &.is-collapsed {
    width: 16px;
    cursor: pointer;
  }
  &::after {
    content: "";
    position: absolute;
    top: 0;
    bottom: 0;
    left: 50%;
    width: 2px;
    margin-left: -1px;
    background: var(--el-border-color-lighter);
  }
}
.wb-split-layout__gutter:hover::after,
.wb-split-layout__gutter.is-collapsed::after {
  background: var(--el-color-primary-light-5);
}
.wb-split-toggle {
  position: absolute;
  left: 50%;
  top: 50%;
  z-index: 1;
  transform: translate(-50%, -50%);
  width: 24px;
  height: 16px;
  border: 1px solid var(--el-border-color-lighter);
  padding: 0;
  background: var(--el-bg-color);
  color: var(--el-text-color-placeholder);
  cursor: pointer;
  border-radius: var(--art-form-control-radius-sm);
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 14px;
  line-height: 1;
  &:hover {
    color: var(--el-color-primary);
    border-color: var(--el-color-primary-light-5);
    background: var(--el-fill-color);
  }
}
.wb-split-layout.is-vertical .wb-split-toggle {
  width: 16px;
  height: 24px;
}
.wb-split-layout__body {
  flex: 1;
  min-height: 0;
  min-width: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
</style>
