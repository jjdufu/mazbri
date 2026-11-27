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

  <div class="settings-sticky-footer">
    <span v-if="isDirty" class="settings-dirty-indicator">
      <span class="settings-dirty-dot" />
      {{ $t("systemSettings.unsavedChanges") }}
    </span>
    <span v-else />
    <ElButton type="primary" :loading="saving" @click="savePolicy">
      <template #icon><ArtSvgIcon icon="ri:save-line" /></template>
      {{ $t("common.actions.save") }}
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

const { saving, isDirty, loadAll, savePolicy } = providePlatformSecurityPolicy();

defineExpose({ isDirty });

onMounted(() => {
  void loadAll();
});
</script>

<style scoped lang="scss">
.settings-sticky-footer {
  position: sticky;
  bottom: 0;
  z-index: 10;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-top: 16px;
  padding: 12px 16px;
  background: var(--art-main-bg-color, var(--el-bg-color));
  border-top: 1px solid var(--el-border-color-lighter);
}

.settings-dirty-indicator {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  color: var(--el-color-warning);
}

.settings-dirty-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--el-color-warning);
}
</style>
