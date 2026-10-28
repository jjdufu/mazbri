<template>
  <div class="layout-lock-screen">
    <div
      v-if="showDevToolsWarning"
      class="fixed top-0 left-0 z-[999999] flex-cc w-full h-full text-white bg-gradient-to-br from-[#1e1e1e] to-black animate-fade-in"
    >
      <div class="p-5 text-center select-none">
        <div class="mb-7.5 text-5xl">🔒</div>
        <h1 class="m-0 mb-5 text-3xl font-semibold text-danger">
          {{ t("lockScreen.devToolsWarningTitle") }}
        </h1>
        <p class="max-w-125 m-0 text-lg leading-relaxed text-white">
          {{ t("lockScreen.devToolsWarningDesc") }}
        </p>
        <div class="mt-7.5 text-sm text-gray-400">
          {{ t("lockScreen.securityLockActivated") }}
        </div>
      </div>
    </div>

    <div v-if="!isLock">
      <ElDialog
        v-model="visible"
        :width="370"
        :show-close="true"
        @open="handleDialogOpen"
      >
        <div class="flex-c flex-col">
          <div class="mt-7.5 mb-3.5 text-base font-medium">
            {{ userInfo.userName }}
          </div>
          <ElForm
            ref="formRef"
            :model="formData"
            :rules="rules"
            class="w-[90%]"
            @submit.prevent="handleLock"
          >
            <ElFormItem prop="password">
              <ElInput
                ref="lockInputRef"
                v-model="formData.password"
                type="password"
                :placeholder="$t('lockScreen.lock.inputPlaceholder')"
                :show-password="true"
                autocomplete="new-password"
                class="w-full mt-9"
                @keyup.enter="handleLock"
              >
                <template #suffix>
                  <ElIcon class="c-p" @click="handleLock">
                    <Lock />
                  </ElIcon>
                </template>
              </ElInput>
            </ElFormItem>
            <ElButton
              v-ripple
              type="primary"
              class="w-full mt-0.5"
              @click="handleLock"
            >
              {{ $t("lockScreen.lock.btnText") }}
            </ElButton>
          </ElForm>
        </div>
      </ElDialog>
    </div>

    <div v-else class="unlock-content">
      <div class="flex-c flex-col w-80">
        <div class="mt-3 mb-3.5 text-base font-medium">
          {{ userInfo.userName }}
        </div>
        <ElForm
          ref="unlockFormRef"
          :model="unlockForm"
          :rules="rules"
          class="w-full !px-2.5"
          @submit.prevent="handleUnlock"
        >
          <ElFormItem prop="password">
            <ElInput
              ref="unlockInputRef"
              v-model="unlockForm.password"
              type="password"
              :placeholder="$t('lockScreen.unlock.inputPlaceholder')"
              :show-password="true"
              autocomplete="new-password"
              class="mt-5"
            >
              <template #suffix>
                <ElIcon class="c-p" @click="handleUnlock">
                  <Unlock />
                </ElIcon>
              </template>
            </ElInput>
          </ElFormItem>

          <ElButton
            v-ripple
            type="primary"
            class="w-full mt-2"
            @click="handleUnlock"
          >
            {{ $t("lockScreen.unlock.btnText") }}
          </ElButton>
          <div class="w-full text-center">
            <ElButton
              text
              class="mt-2.5 !text-g-600 hover:!text-theme hover:!bg-transparent"
              @click="toLogin"
            >
              {{ $t("lockScreen.unlock.backBtnText") }}
            </ElButton>
          </div>
        </ElForm>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { Lock, Unlock } from "@element-plus/icons-vue";
import type { FormInstance, FormRules } from "element-plus";
import { useI18n } from "vue-i18n";
import { ErrorCodes } from "@/constants/error-codes";
import { useUserStore } from "@/store/modules/user";
import { useMfaStepUpStore } from "@/store/modules/mfa-step-up";
import { logger, mittBus } from "@/utils/sys";
import { globalUiTriggers } from "@/utils/sys/global-ui";
const { t } = useI18n();
const userStore = useUserStore();
const { info: userInfo, lockPassword, isLock } = storeToRefs(userStore);
const visible = ref<boolean>(false);
const lockInputRef = ref<any>(null);
const unlockInputRef = ref<any>(null);
const showDevToolsWarning = ref<boolean>(false);
const formRef = ref<FormInstance>();
const unlockFormRef = ref<FormInstance>();
const formData = reactive({
  password: "",
});
const unlockForm = reactive({
  password: "",
});
const rules = computed<FormRules>(() => ({
  password: [
    {
      required: true,
      message: t("lockScreen.lock.inputPlaceholder"),
      trigger: "blur",
    },
  ],
}));
const isMobile = () => {
  return /Android|webOS|iPhone|iPad|iPod|BlackBerry|IEMobile|Opera Mini/i.test(
    navigator.userAgent,
  );
};
const disableDevTools = () => {
  const handleContextMenu = (e: Event) => {
    if (isLock.value) {
      e.preventDefault();
      e.stopPropagation();
      return false;
    }
  };
  document.addEventListener("contextmenu", handleContextMenu, true);
  const handleKeyDown = (e: KeyboardEvent) => {
    if (!isLock.value) return;
    if (e.key === "F12") {
      e.preventDefault();
      e.stopPropagation();
      return false;
    }
    if (e.ctrlKey && e.shiftKey) {
      const key = e.key.toLowerCase();
      if (["i", "j", "c", "k"].includes(key)) {
        e.preventDefault();
        e.stopPropagation();
        return false;
      }
    }
    if (e.ctrlKey && e.key.toLowerCase() === "u") {
      e.preventDefault();
      e.stopPropagation();
      return false;
    }
    if (e.ctrlKey && e.key.toLowerCase() === "s") {
      e.preventDefault();
      e.stopPropagation();
      return false;
    }
    if (e.ctrlKey && e.key.toLowerCase() === "a") {
      e.preventDefault();
      e.stopPropagation();
      return false;
    }
    if (e.ctrlKey && e.key.toLowerCase() === "p") {
      e.preventDefault();
      e.stopPropagation();
      return false;
    }
    if (e.ctrlKey && e.key.toLowerCase() === "f") {
      e.preventDefault();
      e.stopPropagation();
      return false;
    }
    if (e.altKey && e.key === "Tab") {
      e.preventDefault();
      e.stopPropagation();
      return false;
    }
    if (e.ctrlKey && e.key === "Tab") {
      e.preventDefault();
      e.stopPropagation();
      return false;
    }
    if (e.ctrlKey && e.key.toLowerCase() === "w") {
      e.preventDefault();
      e.stopPropagation();
      return false;
    }
    if ((e.ctrlKey && e.key.toLowerCase() === "r") || e.key === "F5") {
      e.preventDefault();
      e.stopPropagation();
      return false;
    }
    if (e.ctrlKey && e.shiftKey && e.key.toLowerCase() === "r") {
      e.preventDefault();
      e.stopPropagation();
      return false;
    }
  };
  document.addEventListener("keydown", handleKeyDown, true);
  const handleSelectStart = (e: Event) => {
    if (isLock.value) {
      e.preventDefault();
      return false;
    }
  };
  document.addEventListener("selectstart", handleSelectStart, true);
  const handleDragStart = (e: Event) => {
    if (isLock.value) {
      e.preventDefault();
      return false;
    }
  };
  document.addEventListener("dragstart", handleDragStart, true);
  let devtools = { open: false };
  const threshold = 160;
  let devToolsInterval: ReturnType<typeof setInterval> | null = null;
  const checkDevTools = () => {
    if (!isLock.value || isMobile()) return;
    const isDevToolsOpen =
      window.outerHeight - window.innerHeight > threshold ||
      window.outerWidth - window.innerWidth > threshold;
    if (isDevToolsOpen && !devtools.open) {
      devtools.open = true;
      showDevToolsWarning.value = true;
    } else if (!isDevToolsOpen && devtools.open) {
      devtools.open = false;
      showDevToolsWarning.value = false;
    }
  };
  if (!isMobile()) {
    devToolsInterval = setInterval(checkDevTools, 500);
  }
  return () => {
    document.removeEventListener("contextmenu", handleContextMenu, true);
    document.removeEventListener("keydown", handleKeyDown, true);
    document.removeEventListener("selectstart", handleSelectStart, true);
    document.removeEventListener("dragstart", handleDragStart, true);
    if (devToolsInterval) {
      clearInterval(devToolsInterval);
    }
  };
};
const verifyPassword = async (inputPassword: string): Promise<boolean> => {
  return await userStore.isLockPasswordMatch(inputPassword);
};
const handleKeydown = (event: KeyboardEvent) => {
  if (event.altKey && event.key.toLowerCase() === "¬") {
    event.preventDefault();
    visible.value = true;
  }
};
const handleDialogOpen = () => {
  setTimeout(() => {
    lockInputRef.value?.input?.focus();
  }, 100);
};
const handleLock = async () => {
  if (!formRef.value) return;
  const valid = await formRef.value.validate().catch((fields) => {
    logger.warn("Lock form validation failed", {
      service: "mazbri-mgmt-web",
      event: "screen_lock_form_validation_failed",
      severity: "warn",
      error_code: ErrorCodes.VALIDATION_ERROR,
      fields,
    });
    return false;
  });
  if (!valid) return;
  try {
    await userStore.setLockPassword(formData.password);
    userStore.setLockStatus(true);
    visible.value = false;
    formData.password = "";
  } catch (error) {
    logger.error("Lock password initialization failed", {
      service: "mazbri-mgmt-web",
      event: "screen_lock_password_init_failed",
      severity: "error",
      error_code: ErrorCodes.INTERNAL_ERROR,
      error,
    });
    ElMessage.error(t("lockScreen.initFailed"));
    return;
  }
  try {
    await useMfaStepUpStore().lock();
  } catch (error) {
    logger.warn("Failed to end MFA step-up when locking screen", {
      service: "mazbri-mgmt-web",
      event: "screen_lock_mfa_step_up_end_failed",
      severity: "warn",
      error_code: ErrorCodes.INTERNAL_ERROR,
      error,
    });
  }
};
const handleUnlock = async () => {
  if (!unlockFormRef.value) return;
  const valid = await unlockFormRef.value.validate().catch((fields) => {
    logger.warn("Unlock form validation failed", {
      service: "mazbri-mgmt-web",
      event: "screen_unlock_form_validation_failed",
      severity: "warn",
      error_code: ErrorCodes.VALIDATION_ERROR,
      fields,
    });
    return false;
  });
  if (!valid) return;
  const isValid = await verifyPassword(unlockForm.password);
  if (isValid) {
    try {
      userStore.setLockStatus(false);
      await userStore.setLockPassword("");
      unlockForm.password = "";
      visible.value = false;
      showDevToolsWarning.value = false;
    } catch (error) {
      logger.error("Unlock store update failed", {
        service: "mazbri-mgmt-web",
        event: "screen_lock_store_update_failed",
        severity: "error",
        error_code: ErrorCodes.INTERNAL_ERROR,
        error,
      });
    }
    return;
  }
  const inputElement = unlockInputRef.value?.$el;
  if (inputElement) {
    inputElement.classList.add("shake-animation");
    setTimeout(() => {
      inputElement.classList.remove("shake-animation");
    }, 300);
  }
  ElMessage.error(t("lockScreen.pwdError"));
  unlockForm.password = "";
};
const toLogin = () => {
  userStore.logOut();
};
const openLockScreen = () => {
  visible.value = true;
};
watch(isLock, (newValue) => {
  if (newValue) {
    document.body.style.overflow = "hidden";
    setTimeout(() => {
      unlockInputRef.value?.input?.focus();
    }, 100);
  } else {
    document.body.style.overflow = "auto";
    showDevToolsWarning.value = false;
  }
});
let cleanupDevTools: (() => void) | null = null;
onMounted(() => {
  mittBus.on("openLockScreen", openLockScreen);
  document.addEventListener("keydown", handleKeydown);
  if (isLock.value) {
    visible.value = true;
    setTimeout(() => {
      unlockInputRef.value?.input?.focus();
    }, 100);
  }
  cleanupDevTools = disableDevTools();
});
watch(
  () => globalUiTriggers.lockScreen.value,
  () => {
    openLockScreen();
  },
);
onUnmounted(() => {
  document.removeEventListener("keydown", handleKeydown);
  document.body.style.overflow = "auto";
  if (cleanupDevTools) {
    cleanupDevTools();
    cleanupDevTools = null;
  }
});
</script>

<style lang="scss" scoped>
.layout-lock-screen :deep(.el-dialog) {
  border-radius: 10px;
}

.unlock-content {
  position: fixed;
  inset: 0;
  z-index: 2500;
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
  background-color: #fff;
  background-image: url("@imgs/lock/bg_light.webp");
  background-size: cover;
  transition: transform 0.3s ease-in-out;
}

.dark {
  .unlock-content {
    background-image: url("@imgs/lock/bg_dark.webp");
  }
}

@keyframes fade-in {
  from {
    opacity: 0;
    transform: scale(0.9);
  }

  to {
    opacity: 1;
    transform: scale(1);
  }
}

.animate-fade-in {
  animation: fade-in 0.3s ease-in-out;
}

@keyframes shake {
  0%,
  100% {
    transform: translateX(0);
  }

  10%,
  30%,
  50%,
  70%,
  90% {
    transform: translateX(-10px);
  }

  20%,
  40%,
  60%,
  80% {
    transform: translateX(10px);
  }
}

.shake-animation {
  animation: shake 0.5s ease-in-out;
}
</style>
