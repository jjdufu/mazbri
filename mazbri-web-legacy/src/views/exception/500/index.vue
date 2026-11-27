<template>
  <ArtException
    :data="{
      title: '500',
      desc: t('exceptionPage.500'),
      btnText: t('exceptionPage.gohome'),
      imgUrl,
    }"
  >
    <template #extra-actions>
      <ElButton size="large" @click="retry">
        {{ t("common.actions.retry") }}
      </ElButton>
    </template>
  </ArtException>
</template>

<script setup lang="ts">
import { useI18n } from "vue-i18n";
import imgUrl from "@imgs/svg/500.svg";

defineOptions({ name: "Exception500" });

const { t } = useI18n();

function retry() {
  window.location.reload();
}
</script>
