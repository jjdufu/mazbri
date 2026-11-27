<template>
  <div class="form">
    <h3 class="title">{{ t("login.title") }}</h3>
    <p class="sub-title">{{ t("login.subTitle") }}</p>
    <ElForm
      ref="formRef"
      :key="formKey"
      :model="formData"
      :rules="rules"
      style="margin-top: 25px"
      @submit.prevent
      @keyup.enter="handleSubmit"
    >
      <ElFormItem prop="username">
        <ElInput
          v-model.trim="formData.username"
          class="custom-height"
          :placeholder="t('login.placeholder.username')"
          :prefix-icon="User"
        />
      </ElFormItem>
      <ElFormItem prop="password">
        <ElInput
          v-model.trim="formData.password"
          class="custom-height"
          :placeholder="t('login.placeholder.password')"
          type="password"
          autocomplete="off"
          show-password
          :prefix-icon="Lock"
        />
      </ElFormItem>

      <div class="relative pb-5 mt-6">
        <div
          class="drag-verify-shell relative z-[2] overflow-hidden select-none tad-300"
          :class="{ 'is-error': !isPassing && isClickPass }"
        >
          <ArtDragVerify
            ref="dragVerifyRef"
            v-model:value="isPassingLocal"
            :text="t('login.sliderText')"
            text-color="var(--art-gray-700)"
            :success-text="t('login.sliderSuccessText')"
            progress-bar-bg="var(--main-color)"
            completed-bg="var(--el-color-success)"
            :background="isDark ? '#26272F' : '#F1F1F4'"
            handler-bg="var(--default-box-color)"
            radius="var(--auth-control-radius)"
          />
        </div>
        <p
          class="absolute top-0 z-[1] px-px mt-2 text-xs text-[#f56c6c] tad-300"
          :class="{ 'translate-y-10': !isPassing && isClickPass }"
        >
          {{ t("login.placeholder.slider") }}
        </p>
      </div>

      <div class="flex-cb mt-2 text-sm">
        <ElCheckbox v-model="formData.rememberPassword">{{
          t("login.rememberPwd")
        }}</ElCheckbox>
      </div>

      <div style="margin-top: 30px">
        <ElButton
          v-ripple
          class="w-full custom-height"
          type="primary"
          :loading="loading"
          @click="handleSubmit"
        >
          {{ t("login.btnText") }}
        </ElButton>
      </div>
    </ElForm>
  </div>
</template>

<script setup lang="ts">
import { ref, reactive, computed } from "vue";
import { useI18n } from "vue-i18n";
import type { FormInstance, FormRules } from "element-plus";
import { User, Lock } from "@element-plus/icons-vue";
interface Props {
  loading: boolean;
  isPassing: boolean;
  isDark: boolean;
}
interface Emits {
  (
    e: "submit",
    credentials: {
      username: string;
      password: string;
    },
  ): void;
  (e: "update:isPassing", value: boolean): void;
}
const props = defineProps<Props>();
const emit = defineEmits<Emits>();
const { t } = useI18n();
const formRef = ref<FormInstance>();
const formKey = ref(0);
const dragVerifyRef = ref();
const isPassingLocal = computed({
  get: () => props.isPassing,
  set: (value: boolean) => emit("update:isPassing", value),
});
const isClickPass = ref(false);
const formData = reactive({
  username: "",
  password: "",
  rememberPassword: true,
});
const rules = computed<FormRules>(() => ({
  username: [
    {
      required: true,
      message: t("login.placeholder.username"),
      trigger: "blur",
    },
  ],
  password: [
    {
      required: true,
      message: t("login.placeholder.password"),
      trigger: "blur",
    },
  ],
}));
const handleSubmit = async () => {
  if (!formRef.value) return;
  try {
    const valid = await formRef.value.validate();
    if (!valid) return;
    if (!props.isPassing) {
      isClickPass.value = true;
      return;
    }
    emit("submit", {
      username: formData.username,
      password: formData.password,
    });
  } catch {
    return;
  }
};
const resetDragVerify = () => {
  dragVerifyRef.value?.reset();
};
defineExpose({
  formRef,
  formData,
  resetDragVerify,
  isClickPass,
});
</script>

<style scoped lang="scss">
.form {
  .drag-verify-shell {
    box-sizing: border-box;
    height: 40px;
    border: 1px solid transparent;
    border-radius: var(--auth-control-radius);
    transition: border-color 0.3s;

    &.is-error {
      border-color: #ff4e4f;
    }
  }


  :deep(.drag_verify) {
    width: 100%;
    height: 100%;
    border: none;
    border-radius: inherit;
  }
}
</style>
