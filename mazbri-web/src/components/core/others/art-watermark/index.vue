<template>
  <div
    v-if="watermarkVisible"
    class="watermark-container"
    :style="{ zIndex: zIndex }"
    aria-hidden="true"
  >
    <svg
      class="watermark-svg"
      xmlns="http://www.w3.org/2000/svg"
      :viewBox="`0 0 ${dimensions.width} ${dimensions.height}`"
      preserveAspectRatio="xMidYMid slice"
    >
      <title>{{ t("watermark.title") }}</title>
      <g
        :transform="`rotate(${rotate}, ${dimensions.width / 2}, ${dimensions.height / 2})`"
      >
        <text
          v-for="pos in positions"
          :key="pos.id"
          :x="pos.x"
          :y="pos.y"
          :fill="fontColor"
          :font-size="fontSize"
          font-family="system-ui, -apple-system, sans-serif"
          text-anchor="middle"
          dominant-baseline="middle"
          style="user-select: none"
        >
          {{ watermarkText }}
        </text>
      </g>
    </svg>
  </div>
</template>

<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { useSettingStore } from "@/store/modules/setting";
import { useUserStore } from "@/store/modules/user";
defineOptions({ name: "ArtWatermark" });
const { t } = useI18n();
const settingStore = useSettingStore();
const userStore = useUserStore();
const { watermarkVisible } = storeToRefs(settingStore);
interface WatermarkProps {
  fontSize?: number;
  fontColor?: string;
  rotate?: number;
  gapX?: number;
  gapY?: number;
  zIndex?: number;
}
const props = withDefaults(defineProps<WatermarkProps>(), {
  fontSize: 14,
  fontColor: "rgba(128, 128, 128, 0.15)",
  rotate: -22,
  gapX: 280,
  gapY: 140,
  zIndex: 3100,
});
const dimensions = ref({
  width: window.innerWidth,
  height: window.innerHeight,
});
const timestamp = ref("");
const username = computed(() => userStore.info?.username || "unknown");
const watermarkText = computed(() => `${username.value} ${timestamp.value}`);
const positions = computed(() => {
  const result: {
    id: number;
    x: number;
    y: number;
  }[] = [];
  const { width, height } = dimensions.value;
  const { gapX, gapY } = props;
  const cols = Math.ceil(width / gapX) + 2;
  const rows = Math.ceil(height / gapY) + 2;
  let id = 0;
  for (let row = -1; row < rows; row++) {
    for (let col = -1; col < cols; col++) {
      result.push({
        id: id++,
        x: col * gapX + (row % 2 === 0 ? 0 : gapX / 2),
        y: row * gapY,
      });
    }
  }
  return result;
});
const updateTimestamp = () => {
  const now = new Date();
  const year = now.getFullYear();
  const month = String(now.getMonth() + 1).padStart(2, "0");
  const day = String(now.getDate()).padStart(2, "0");
  const hours = String(now.getHours()).padStart(2, "0");
  const minutes = String(now.getMinutes()).padStart(2, "0");
  const seconds = String(now.getSeconds()).padStart(2, "0");
  timestamp.value = `${year}/${month}/${day} ${hours}:${minutes}:${seconds}`;
};
const updateDimensions = () => {
  dimensions.value = {
    width: window.innerWidth,
    height: window.innerHeight,
  };
};
let timer: ReturnType<typeof setInterval> | null = null;
watch(
  watermarkVisible,
  (visible) => {
    if (visible) {
      updateTimestamp();
      timer = setInterval(updateTimestamp, 1000);
    } else {
      if (timer) {
        clearInterval(timer);
        timer = null;
      }
    }
  },
  { immediate: true },
);
onMounted(() => {
  window.addEventListener("resize", updateDimensions);
});
onUnmounted(() => {
  window.removeEventListener("resize", updateDimensions);
  if (timer) {
    clearInterval(timer);
    timer = null;
  }
});
</script>

<style scoped lang="scss">
.watermark-container {
  position: fixed;
  top: 0;
  left: 0;
  width: 100vw;
  height: 100vh;
  pointer-events: none;
  background: transparent;
}

.watermark-svg {
  position: absolute;
  top: 0;
  left: 0;
  width: 100%;
  height: 100%;
  overflow: visible;
}
</style>
