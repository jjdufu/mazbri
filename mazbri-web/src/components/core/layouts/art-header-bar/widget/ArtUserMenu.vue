<template>
  <ArtMfaStepUpPill />
  <ElTooltip :content="t('common.accountSecurity')" placement="bottom">
    <ArtIconButton
      icon="ri:shield-user-line"
      class="account-security-btn"
      :aria-label="t('common.accountSecurity')"
      @click="goToAccountSecurity"
    />
  </ElTooltip>
  <ArtIconButton
    icon="ri:logout-circle-line"
    class="logout-btn !mr-0"
    @click="loginOut"
  />
</template>

<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";
import { ElMessageBox } from "element-plus";
import { useUserStore } from "@/store/modules/user";
import { ACCOUNT_SECURITY_PATH } from "@/utils/http/mfa-dialog-service";
import ArtMfaStepUpPill from "./ArtMfaStepUpPill.vue";
defineOptions({ name: "ArtUserMenu" });
const { t } = useI18n();
const router = useRouter();
const userStore = useUserStore();
const goToAccountSecurity = (): void => {
  void router.push(ACCOUNT_SECURITY_PATH);
};
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
