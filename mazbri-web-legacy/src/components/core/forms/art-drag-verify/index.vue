<template>
  <div
    ref="dragVerify"
    class="drag_verify"
    :style="dragVerifyStyle"
    @mousemove="dragMoving"
    @mouseup="dragFinish"
    @mouseleave="dragFinish"
    @touchmove="dragMoving"
    @touchend="dragFinish"
  >
    <div
      ref="progressBar"
      class="dv_progress_bar"
      :class="{ 'is-resetting': isResetting }"
      :style="progressBarStyle"
    ></div>

    <div ref="messageRef" class="dv_text" :class="{ 'is-passed': value }" :style="textStyle">
      <slot v-if="$slots.textBefore" name="textBefore"></slot>
      {{ message }}
      <slot v-if="$slots.textAfter" name="textAfter"></slot>
    </div>

    <div
      ref="handler"
      class="dv_handler dv_handler_bg"
      :class="{ 'is-resetting': isResetting }"
      :style="handlerStyle"
      @mousedown="dragStart"
      @touchstart="dragStart"
    >
      <ArtSvgIcon
        :icon="value ? successIcon : handlerIcon"
        class="text-g-600"
      ></ArtSvgIcon>
    </div>
  </div>
</template>

<script setup lang="ts">
import { useI18n } from "vue-i18n";
defineOptions({ name: "ArtDragVerify" });
const { t } = useI18n();
const emit = defineEmits(["handlerMove", "update:value", "passCallback"]);
interface PropsType {
  value: boolean;
  width?: number | string;
  height?: number;
  text?: string;
  successText?: string;
  background?: string;
  progressBarBg?: string;
  completedBg?: string;
  circle?: boolean;
  radius?: string;
  handlerIcon?: string;
  successIcon?: string;
  handlerBg?: string;
  textSize?: string;
  textColor?: string;
}
const props = withDefaults(defineProps<PropsType>(), {
  value: false,
  width: "100%",
  height: 40,
  text: undefined,
  successText: undefined,
  background: "#eee",
  progressBarBg: "#1385FF",
  completedBg: "#57D187",
  circle: false,
  radius: "calc(var(--custom-radius) / 3 + 2px)",
  handlerIcon: "solar:double-alt-arrow-right-linear",
  successIcon: "ri:check-fill",
  handlerBg: "#fff",
  textSize: "13px",
  textColor: "#333",
});

const isMoving = ref(false);
const isResetting = ref(false);

const handlerLeft = ref(0);

const progressWidth = ref(0);
const dragStartX = ref(0);

const dragVerify = ref<HTMLElement>();
const messageRef = ref<HTMLElement>();
const handler = ref<HTMLElement>();
const progressBar = ref<HTMLElement>();

let touchStartX = 0;
let touchStartY = 0;
const onTouchStart = (e: TouchEvent) => {
  touchStartX = e.targetTouches[0].pageX;
  touchStartY = e.targetTouches[0].pageY;
};
const onTouchMove = (e: TouchEvent) => {
  const moveX = e.targetTouches[0].pageX;
  const moveY = e.targetTouches[0].pageY;
  if (Math.abs(moveX - touchStartX) > Math.abs(moveY - touchStartY)) {
    e.preventDefault();
  }
};

const getNumericWidth = (): number => {
  if (typeof props.width === "string") {
    return dragVerify.value?.offsetWidth || 260;
  }
  return props.width;
};

const getNumericHeight = (): number => {
  const measured = dragVerify.value?.clientHeight;
  if (typeof measured === "number" && measured > 0) {
    return measured;
  }
  return props.height;
};

const getStyleWidth = (): string => {
  if (typeof props.width === "string") {
    return props.width;
  }
  return props.width + "px";
};

const syncTrackCssVars = () => {
  const numericWidth = getNumericWidth();
  const numericHeight = getNumericHeight();
  dragVerify.value?.style.setProperty(
    "--width",
    Math.floor(numericWidth / 2) + "px",
  );
  dragVerify.value?.style.setProperty(
    "--pwidth",
    -Math.floor(numericWidth / 2) + "px",
  );
  dragVerify.value?.style.setProperty("--handler-size", numericHeight + "px");
  dragVerify.value?.style.setProperty("--textColor", props.textColor);
};


const setProgressToHandler = (left: number, handlerSize: number) => {
  handlerLeft.value = left;
  progressWidth.value = left + handlerSize;
};

const passVerify = () => {
  const numericWidth = getNumericWidth();
  const handlerSize = getNumericHeight();
  handlerLeft.value = Math.max(0, numericWidth - handlerSize);

  progressWidth.value = numericWidth;
  isMoving.value = false;
  emit("update:value", true);
  emit("passCallback");
};

const dragStart = (e: MouseEvent | TouchEvent) => {
  if (props.value) return;
  isResetting.value = false;
  isMoving.value = true;
  const pageX =
    "touches" in e ? e.touches[0].pageX : (e as MouseEvent).pageX;
  dragStartX.value = pageX - handlerLeft.value;
  emit("handlerMove");
};

const dragMoving = (e: MouseEvent | TouchEvent) => {
  if (!isMoving.value || props.value) return;
  const numericWidth = getNumericWidth();
  const handlerSize = getNumericHeight();
  const pageX =
    "touches" in e
      ? e.touches[0].pageX
      : (e as MouseEvent).pageX;
  let nextLeft = pageX - dragStartX.value;
  const maxLeft = numericWidth - handlerSize;
  if (nextLeft <= 0) {
    setProgressToHandler(0, handlerSize);
    return;
  }
  if (nextLeft >= maxLeft) {
    setProgressToHandler(maxLeft, handlerSize);
    passVerify();
    return;
  }
  setProgressToHandler(nextLeft, handlerSize);
};

const dragFinish = (e: MouseEvent | TouchEvent) => {
  if (!isMoving.value || props.value) return;
  const numericWidth = getNumericWidth();
  const handlerSize = getNumericHeight();
  const pageX =
    "changedTouches" in e
      ? e.changedTouches[0].pageX
      : (e as MouseEvent).pageX;
  const nextLeft = pageX - dragStartX.value;
  const maxLeft = numericWidth - handlerSize;
  if (nextLeft < maxLeft) {

    isResetting.value = true;
    handlerLeft.value = 0;
    progressWidth.value = 0;
    isMoving.value = false;

    window.setTimeout(() => {
      isResetting.value = false;
    }, 220);
  } else {
    setProgressToHandler(maxLeft, handlerSize);
    passVerify();
  }
};

const reset = () => {
  isMoving.value = false;
  isResetting.value = false;
  handlerLeft.value = 0;
  progressWidth.value = 0;
  dragStartX.value = 0;
  emit("update:value", false);
};

const handlerStyle = computed(() => ({
  left: handlerLeft.value + "px",
  width: "var(--handler-size, " + props.height + "px)",
  height: "100%",
  background: props.handlerBg,
}));

const dragVerifyStyle = computed(() => ({
  width: getStyleWidth(),
  height: props.height + "px",
  lineHeight: props.height + "px",
  background: props.background,
  borderRadius: props.circle ? props.height / 2 + "px" : props.radius,
}));

const progressBarStyle = computed(() => ({

  background: props.value ? props.completedBg : props.progressBarBg,
  width: progressWidth.value + "px",
  height: "100%",
}));

const textStyle = computed(() => ({
  fontSize: props.textSize,
}));

const resolvedText = computed(() => props.text ?? t("login.sliderText"));
const resolvedSuccessText = computed(
  () => props.successText ?? t("login.sliderSuccessText"),
);
const message = computed(() => {
  return props.value ? resolvedSuccessText.value : resolvedText.value;
});

onMounted(() => {
  nextTick(() => {
    syncTrackCssVars();
  });
  document.addEventListener("touchstart", onTouchStart, { passive: true });
  document.addEventListener("touchmove", onTouchMove, { passive: false });
});
onBeforeUnmount(() => {
  document.removeEventListener("touchstart", onTouchStart);
  document.removeEventListener("touchmove", onTouchMove);
});

defineExpose({
  reset,
});
</script>

<style lang="scss" scoped>
.drag_verify {
  position: relative;
  box-sizing: border-box;
  overflow: hidden;
  text-align: center;

  border: none;

  .dv_handler {
    position: absolute;
    top: 0;
    bottom: 0;
    left: 0;
    z-index: 2;
    box-sizing: border-box;
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: move;

    box-shadow: 1px 0 0 rgba(15, 23, 42, 0.06);

    &.is-resetting {
      transition: left 0.2s ease;
    }

    i {
      padding-left: 0;
      font-size: 14px;
      color: #999;
    }

    .el-icon-circle-check {
      margin-top: 9px;
      color: #6c6;
    }
  }

  .dv_progress_bar {
    position: absolute;
    top: 0;
    left: 0;
    z-index: 0;
    width: 0;
    height: 100%;

    border-radius: 0;

    &.is-resetting {
      transition: width 0.2s ease;
    }
  }

  .dv_text {
    position: absolute;
    inset: 0;
    z-index: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    user-select: none;
    text-size-adjust: none;
    pointer-events: none;

    &:not(.is-passed) {
      background: linear-gradient(
        to right,
        var(--textColor) 0%,
        var(--textColor) 40%,
        #fff 50%,
        var(--textColor) 60%,
        var(--textColor) 100%
      );
      background-size: 200% 100%;
      -webkit-background-clip: text;
      background-clip: text;
      animation: slidetounlock 2s cubic-bezier(0, 0.2, 1, 1) infinite;
    }

    &.is-passed {
      color: #fff;
      -webkit-text-fill-color: #fff;
      animation: none;
      background: none;
    }

    * {
      -webkit-text-fill-color: inherit;
    }
  }
}
</style>

<style lang="scss">
@keyframes slidetounlock {
  0% {
    background-position: var(--pwidth, -130px) 0;
  }

  100% {
    background-position: var(--width, 130px) 0;
  }
}
</style>
