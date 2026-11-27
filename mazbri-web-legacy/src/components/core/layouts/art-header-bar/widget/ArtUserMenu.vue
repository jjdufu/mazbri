<template>
  <ElDropdown trigger="click" placement="bottom-end" @command="handleCommand">
    <button
      type="button"
      class="art-user-capsule"
      :aria-label="displayName"
    >
      <ArtSvgIcon icon="ri:user-3-line" class="art-user-capsule__icon" />
      <span class="art-user-capsule__name">{{ displayName }}</span>
      <ElTag
        v-if="roleLabel"
        size="small"
        effect="plain"
        :type="roleTagType"
        class="art-user-capsule__role"
      >
        {{ roleLabel }}
      </ElTag>
      <ArtSvgIcon icon="ri:arrow-down-s-line" class="art-user-capsule__caret" />
    </button>
    <template #dropdown>
      <ElDropdownMenu>
        <ElDropdownItem command="security">
          <ArtSvgIcon icon="ri:shield-user-line" class="mr-2" />
          {{ t("common.accountSecurity") }}
        </ElDropdownItem>
        <ElDropdownItem command="lock">
          <ArtSvgIcon icon="ri:lock-line" class="mr-2" />
          {{ t("topBar.user.lockScreen") }}
        </ElDropdownItem>
        <ElDropdownItem divided command="logout">
          <ArtSvgIcon icon="ri:logout-circle-line" class="mr-2" />
          {{ t("topBar.user.logout") }}
        </ElDropdownItem>
      </ElDropdownMenu>
    </template>
  </ElDropdown>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { ElMessageBox } from "element-plus";
import { useUserStore } from "@/store/modules/user";
import { usePersonalSecurityStore } from "@/store/modules/personal-security";
import { getRoleLabel, getRoleType } from "@/utils/display-labels";
import { mittBus } from "@/utils/sys";
import { triggerLockScreen } from "@/utils/sys/global-ui";

defineOptions({ name: "ArtUserMenu" });

const { t } = useI18n();
const userStore = useUserStore();
const personalSecurityStore = usePersonalSecurityStore();

const displayName = computed(
  () => userStore.info?.display_name || userStore.info?.username || "—",
);
const roleLabel = computed(() =>
  userStore.info?.role ? getRoleLabel(userStore.info.role) : "",
);
const roleTagType = computed(() =>
  userStore.info?.role ? getRoleType(userStore.info.role) : "info",
);

const handleCommand = (command: string): void => {
  if (command === "security") {
    personalSecurityStore.openDrawer("mfa");
  } else if (command === "lock") {
    triggerLockScreen();
    mittBus.emit("openLockScreen");
  } else if (command === "logout") {
    ElMessageBox.confirm(t("common.logOutTips"), t("common.tips"), {
      confirmButtonText: t("common.confirm"),
      cancelButtonText: t("common.cancel"),
      customClass: "login-out-dialog",
    }).then(() => {
      userStore.logOut();
    });
  }
};
</script>

<style scoped lang="scss">
.art-user-capsule {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 32px;
  padding: 0 10px;
  margin-right: 4px;
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 999px;
  background: var(--el-fill-color-blank);
  color: var(--el-text-color-primary);
  font-size: 13px;
  cursor: pointer;
  transition:
    border-color 0.15s ease,
    background-color 0.15s ease;

  &:hover {
    border-color: var(--el-border-color-hover);
    background: var(--el-fill-color-light);
  }
}

.art-user-capsule__icon {
  font-size: 14px;
  color: var(--el-text-color-secondary);
}

.art-user-capsule__name {
  max-width: 120px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-weight: 500;
}

.art-user-capsule__role {
  flex-shrink: 0;
}

.art-user-capsule__caret {
  font-size: 14px;
  color: var(--el-text-color-secondary);
}
</style>
