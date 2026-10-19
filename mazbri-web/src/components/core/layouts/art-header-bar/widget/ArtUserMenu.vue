<template>
  <ArtIconButton
    icon="ri:logout-circle-line"
    class="logout-btn !mr-0"
    @click="loginOut"
  />
</template>

<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { ElMessageBox } from "element-plus";
import { useUserStore } from "@/store/modules/user";
defineOptions({ name: "ArtUserMenu" });
const { t } = useI18n();
const userStore = useUserStore();
const loginOut = (): void => {
  ElMessageBox.confirm(t("common.logOutTips"), t("common.tips"), {
    confirmButtonText: t("common.confirm"),
    cancelButtonText: t("common.cancel"),
    customClass: "login-out-dialog",
  }).then(() => {
    userStore.logOut();
  });
};
</script>

<style scoped></style>
