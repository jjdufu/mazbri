<template>
  <div ref="rootRef" class="wb-split">
    <div
      class="wb-split__pane"
      :class="{ 'is-collapsed': collapsed }"
      :style="leftStyle"
    >
      <slot name="left" />
    </div>
    <div
      class="wb-split__gutter"
      :class="{ 'is-collapsed': collapsed }"
      @mousedown.prevent="startDrag"
      @dblclick.prevent="toggle"
    >
      <button
        type="button"
        class="wb-split-toggle"
        :title="
          collapsed
            ? $t('upstreamConnector.expandSidebar')
            : $t('upstreamConnector.collapseSidebar')
        "
        :aria-expanded="!collapsed"
        @click.stop="toggle"
        @mousedown.stop
      >
        <ArtSvgIcon
          :icon="collapsed ? 'ri:arrow-right-s-line' : 'ri:arrow-left-s-line'"
        />
      </button>
    </div>
    <div class="wb-split__pane wb-split__pane--main">
      <slot name="center" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { $t } from "@/locales";
import { HSPLIT_DEFAULT_LEFT, parseHSplitState } from "./workbenchHelpers";

const STORAGE_KEY = "mazbri.workbench.split.v2";

const props = withDefaults(
  defineProps<{
    minLeft?: number;
    minMain?: number;
  }>(),
  { minLeft: 180, minMain: 480 },
);

const rootRef = ref<HTMLElement | null>(null);
const leftW = ref(HSPLIT_DEFAULT_LEFT);
const collapsed = ref(true);

const leftStyle = computed(() => {
  if (collapsed.value) return { width: "0px", minWidth: "0px" };
  return { width: `${leftW.value}px`, minWidth: `${props.minLeft}px` };
});

onMounted(() => {
  try {
    const st = parseHSplitState(localStorage.getItem(STORAGE_KEY));
    leftW.value = st.left;
  } catch {
    void 0;
  }
  collapsed.value = true;
});

function persist() {
  try {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({ left: leftW.value, collapsed: collapsed.value }),
    );
  } catch {
    void 0;
  }
}

function toggle() {
  collapsed.value = !collapsed.value;
  persist();
}

function startDrag(ev: MouseEvent) {
  const startX = ev.clientX;
  const startLeft = collapsed.value ? props.minLeft : leftW.value;
  const total = rootRef.value?.clientWidth || 0;
  let moved = false;

  const onMove = (e: MouseEvent) => {
    const dx = e.clientX - startX;
    if (!moved && Math.abs(dx) < 3) return;
    moved = true;
    if (collapsed.value) collapsed.value = false;
    const next = startLeft + dx;
    const max = total - props.minMain;
    leftW.value = Math.min(Math.max(next, props.minLeft), Math.max(max, props.minLeft));
  };
  const onUp = () => {
    window.removeEventListener("mousemove", onMove);
    window.removeEventListener("mouseup", onUp);
    persist();
  };
  window.addEventListener("mousemove", onMove);
  window.addEventListener("mouseup", onUp);
}
</script>

<style scoped lang="scss">
.wb-split {
  display: flex;
  min-height: 0;
  flex: 1;
  width: 100%;
}
.wb-split__pane {
  min-height: 0;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}
.wb-split__pane.is-collapsed {
  pointer-events: none;
}
.wb-split__pane--main {
  flex: 1;
  min-width: 0;
}
.wb-split__gutter {
  width: 8px;
  cursor: col-resize;
  flex-shrink: 0;
  background: transparent;
  position: relative;
  &.is-collapsed {
    width: 16px;
    cursor: pointer;
  }
}
.wb-split__gutter::after {
  content: "";
  position: absolute;
  top: 0;
  bottom: 0;
  left: 50%;
  width: 2px;
  margin-left: -1px;
  background: var(--el-border-color-lighter);
}
.wb-split__gutter:hover::after,
.wb-split__gutter.is-collapsed::after {
  background: var(--el-color-primary-light-5);
}
.wb-split-toggle {
  position: absolute;
  top: 10px;
  left: 50%;
  z-index: 1;
  transform: translateX(-50%);
  width: 16px;
  height: 24px;
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
</style>
