<template>
  <SettingsCardGrid>
    <template #left>
      <PasswordComplexityCard />
      <LoginProtectionCard />
    </template>
    <template #right>
      <PasswordExpiryCard />
      <GlobalMfaCard />
    </template>
  </SettingsCardGrid>

  <div class="tab-save-bar">
    <ElButton type="primary" :loading="saving" @click="savePolicy">
      <template #icon><ArtSvgIcon icon="ri:save-line" /></template>
      {{ $t("securityPolicy.actions.savePolicy") }}
    </ElButton>
  </div>
</template>

<script setup lang="ts">
import { onMounted } from "vue";
import SettingsCardGrid from "@/components/common/settings-card-grid/index.vue";
import PasswordComplexityCard from "./PasswordComplexityCard.vue";
import LoginProtectionCard from "./LoginProtectionCard.vue";
import PasswordExpiryCard from "./PasswordExpiryCard.vue";
import GlobalMfaCard from "./GlobalMfaCard.vue";
import { providePlatformSecurityPolicy } from "./usePlatformSecurityPolicy";

defineOptions({ name: "SecurityPolicyTab" });

const { saving, loadAll, savePolicy } = providePlatformSecurityPolicy();

onMounted(() => {
  void loadAll();
});
</script>

<style scoped lang="scss">
.tab-save-bar {
  display: flex;
  justify-content: flex-end;
  padding-top: 4px;
}
</style>
