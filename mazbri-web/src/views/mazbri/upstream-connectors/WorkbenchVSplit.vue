<template>
  <div ref="rootRef" class="wb-vsplit">
    <div class="wb-vsplit__pane wb-vsplit__pane--top">
      <slot name="top" />
    </div>
    <div
      class="wb-vsplit__gutter"
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
        :aria-expanded="!collapsed"
        @click.stop="toggle"
        @mousedown.stop
      >
        <ArtSvgIcon
          :icon="collapsed ? 'ri:arrow-up-s-line' : 'ri:arrow-down-s-line'"
        />
      </button>
    </div>
    <div
      class="wb-vsplit__pane"
      :class="{ 'is-collapsed': collapsed }"
      :style="bottomStyle"
    >
      <div v-show="!collapsed" class="wb-vsplit__body">
        <slot name="bottom" />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { $t } from "@/locales";
import {
  VSPLIT_COLLAPSED_HEIGHT,
  VSPLIT_DEFAULT_BOTTOM,
  parseVSplitState,
} from "./workbenchHelpers";

const STORAGE_KEY = "mazbri.workbench.split.v2.bottom";

const props = withDefaults(
  defineProps<{
    minBottom?: number;
    minTop?: number;
  }>(),
  { minBottom: 120, minTop: 200 },
);

const rootRef = ref<HTMLElement | null>(null);
const bottomH = ref(VSPLIT_DEFAULT_BOTTOM);
const collapsed = ref(true);

const bottomStyle = computed(() => {
  if (collapsed.value) {
    return {
      height: `${VSPLIT_COLLAPSED_HEIGHT}px`,
      minHeight: `${VSPLIT_COLLAPSED_HEIGHT}px`,
    };
  }
  return {
    height: `${bottomH.value}px`,
    minHeight: `${props.minBottom}px`,
  };
});

onMounted(() => {
  try {
    const st = parseVSplitState(localStorage.getItem(STORAGE_KEY));
    bottomH.value = st.bottom;
    collapsed.value = st.collapsed;
  } catch {
    void 0;
  }
});

function persist() {
  try {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({ bottom: bottomH.value, collapsed: collapsed.value }),
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

function startDrag(ev: MouseEvent) {
  const startY = ev.clientY;
  const startBottom = collapsed.value ? props.minBottom : bottomH.value;
  const total = rootRef.value?.clientHeight || 0;
  let moved = false;

  const onMove = (e: MouseEvent) => {
    const dy = e.clientY - startY;
    if (!moved && Math.abs(dy) < 3) return;
    moved = true;
    if (collapsed.value) collapsed.value = false;
    const next = startBottom - dy;
    const max = total - props.minTop;
    bottomH.value = Math.min(
      Math.max(next, props.minBottom),
      Math.max(max, props.minBottom),
    );
  };
  const onUp = () => {
    window.removeEventListener("mousemove", onMove);
    window.removeEventListener("mouseup", onUp);
    persist();
  };
  window.addEventListener("mousemove", onMove);
  window.addEventListener("mouseup", onUp);
}

defineExpose({ expand, toggle });
</script>

<style scoped lang="scss">
.wb-vsplit {
  display: flex;
  flex-direction: column;
  min-height: 0;
  flex: 1;
  width: 100%;
  height: 100%;
}
.wb-vsplit__pane {
  min-width: 0;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}
.wb-vsplit__pane--top {
  flex: 1;
  min-height: 0;
}
.wb-vsplit__gutter {
  height: 8px;
  cursor: row-resize;
  flex-shrink: 0;
  background: transparent;
  position: relative;
  &.is-collapsed {
    height: 16px;
    cursor: pointer;
  }
}
.wb-vsplit__gutter::after {
  content: "";
  position: absolute;
  left: 0;
  right: 0;
  top: 50%;
  height: 2px;
  margin-top: -1px;
  background: var(--el-border-color-lighter);
}
.wb-vsplit__gutter:hover::after,
.wb-vsplit__gutter.is-collapsed::after {
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
.wb-vsplit__body {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
</style>
