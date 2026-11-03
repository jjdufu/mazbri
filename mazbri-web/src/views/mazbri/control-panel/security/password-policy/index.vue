<template>
  <ControlPanelPageShell
    :title="$t('securityPolicy.page.title')"
    :subtitle="$t('accountSecurity.platformPolicy.description')"
    icon="ri:lock-password-line"
  >
    <PlatformSecurityPolicyCards />
  </ControlPanelPageShell>
</template>

<script setup lang="ts">
/**
 * step-18 过渡：表单逻辑已迁入账号安全共用组件。
 * step-19 将删除本路由与本文件，不做长期兼容壳。
 */
import ControlPanelPageShell from "@/components/common/control-panel-page-shell/index.vue";
import PlatformSecurityPolicyCards from "@/views/mazbri/account/security/modules/PlatformSecurityPolicyCards.vue";

defineOptions({ name: "PasswordPolicy" });
</script>
