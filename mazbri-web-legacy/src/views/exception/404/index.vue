<template>
  <ArtException
    :data="{
      title: '404',
      desc: t('exceptionPage.404'),
      btnText: t('exceptionPage.gohome'),
      imgUrl,
    }"
  >
    <template #extra-actions>
      <ElButton size="large" @click="goBack">
        {{ t("common.actions.back") }}
      </ElButton>
    </template>
  </ArtException>
</template>

<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";
import imgUrl from "@imgs/svg/404.svg";
import { useCommon } from "@/hooks/core/useCommon";

defineOptions({ name: "Exception404" });

const { t } = useI18n();
const router = useRouter();
const { homePath } = useCommon();

function goBack() {
  const back = window.history.state?.back;
  if (typeof back === "string" && !back.includes("/exception/")) {
    router.back();
    return;
  }
  void router.push(homePath.value);
}
</script>
