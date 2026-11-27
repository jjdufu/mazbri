<template>
  <ArtException
    :data="{
      title: '403',
      desc: t('exceptionPage.403'),
      btnText: t('exceptionPage.gohome'),
      imgUrl,
    }"
  >
    <template v-if="identityMeta" #meta>
      <ElTag type="info" effect="plain" size="small">
        {{ identityMeta }}
      </ElTag>
    </template>
    <template #extra-actions>
      <ElButton size="large" @click="goBack">
        {{ t("common.actions.back") }}
      </ElButton>
      <ElButton size="large" plain @click="switchAccount">
        {{ t("exceptionPage.switchAccount") }}
      </ElButton>
    </template>
  </ArtException>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";
import imgUrl from "@imgs/svg/403.svg";
import { useUserStore } from "@/store/modules/user";
import { useCommon } from "@/hooks/core/useCommon";
import { getRoleLabel } from "@/utils/display-labels";

defineOptions({ name: "Exception403" });

const { t } = useI18n();
const router = useRouter();
const userStore = useUserStore();
const { homePath } = useCommon();

const identityMeta = computed(() => {
  const username = userStore.info?.username;
  const role = userStore.info?.role;
  if (!username && !role) return "";
  return [username, role ? getRoleLabel(role) : ""].filter(Boolean).join(" · ");
});

function goBack() {
  const back = window.history.state?.back;
  if (typeof back === "string" && !back.includes("/exception/")) {
    router.back();
    return;
  }
  void router.push(homePath.value);
}

function switchAccount() {
  userStore.logOut();
}
</script>
