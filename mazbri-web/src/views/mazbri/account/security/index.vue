<template>
  <ControlPanelPageShell
    :title="$t('accountSecurity.page.title')"
    icon="ri:shield-user-line"
  >
    <template v-if="isAdmin" #actions>
      <ElButton type="primary" :loading="policySaving" @click="policy.saveAll">
        <template #icon><ArtSvgIcon icon="ri:save-line" /></template>
        {{ $t("securityPolicy.actions.savePolicy") }}
      </ElButton>
    </template>

    <SettingsCardGrid>
      <template #left>
        <MfaCard />
        <PasswordComplexityCard v-if="isAdmin" />
      </template>

      <template #right>
        <template v-if="isAdmin">
          <LoginProtectionCard />
          <PasswordExpiryCard />
        </template>
        <PasswordCard />
      </template>
    </SettingsCardGrid>
  </ControlPanelPageShell>
</template>

<script setup lang="ts">
import { nextTick, onActivated, onMounted, watch } from "vue";
import { useRoute } from "vue-router";
import ControlPanelPageShell from "@/components/common/control-panel-page-shell/index.vue";
import SettingsCardGrid from "@/components/common/settings-card-grid/index.vue";
import { useOpsRole } from "@/composables/ops/useOpsRole";
import MfaCard from "./modules/MfaCard.vue";
import PasswordCard from "./modules/PasswordCard.vue";
import LoginProtectionCard from "./modules/LoginProtectionCard.vue";
import PasswordComplexityCard from "./modules/PasswordComplexityCard.vue";
import PasswordExpiryCard from "./modules/PasswordExpiryCard.vue";
import {
  PLATFORM_POLICY_ANCHOR,
  providePlatformSecurityPolicy,
} from "./modules/usePlatformSecurityPolicy";

defineOptions({ name: "AccountSecurity" });

const route = useRoute();
const { isAdmin } = useOpsRole();
const policy = providePlatformSecurityPolicy();
const policySaving = policy.saving;

function loadPolicyIfAdmin(): void {
  if (isAdmin.value) void policy.loadAll();
}

function shouldFocusPlatformPolicy(): boolean {
  if (!isAdmin.value) return false;
  if (route.hash === `#${PLATFORM_POLICY_ANCHOR}`) return true;
  const section = route.query.section;
  return (
    section === "platform" ||
    (Array.isArray(section) && section[0] === "platform")
  );
}

function scrollToPlatformPolicy(): void {
  if (!shouldFocusPlatformPolicy()) return;
  void nextTick(() => {
    document
      .getElementById(PLATFORM_POLICY_ANCHOR)
      ?.scrollIntoView({ behavior: "smooth", block: "start" });
  });
}

onMounted(() => {
  loadPolicyIfAdmin();
  scrollToPlatformPolicy();
});

onActivated(() => {
  loadPolicyIfAdmin();
});

watch(isAdmin, (admin, wasAdmin) => {
  if (admin && !wasAdmin) void policy.loadAll();
});

watch(
  () => [route.hash, route.query.section, isAdmin.value] as const,
  () => {
    scrollToPlatformPolicy();
  },
);
</script>
