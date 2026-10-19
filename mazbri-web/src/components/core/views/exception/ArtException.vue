<template>
  <div class="page-content !border-0 !bg-transparent min-h-screen flex-cc">
    <div class="flex-cc max-md:!block max-md:text-center">
      <ThemeSvg :src="data.imgUrl" size="100%" class="!w-100" />
      <div
        class="ml-15 w-75 max-md:mx-auto max-md:mt-10 max-md:w-full max-md:text-center"
      >
        <p class="text-xl leading-7 text-g-600 max-md:text-lg">
          {{ data.desc }}
        </p>
        <ElButton
          v-ripple
          type="primary"
          size="large"
          class="mt-5"
          @click="backHome"
          >{{ data.btnText }}</ElButton
        >
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { useCommon } from "@/hooks/core/useCommon";
const router = useRouter();
interface ExceptionData {
  title: string;
  desc: string;
  btnText: string;
  imgUrl: string;
}
withDefaults(
  defineProps<{
    data: ExceptionData;
  }>(),
  {},
);
const { homePath } = useCommon();
const backHome = () => {
  router.push(homePath.value);
};
</script>
