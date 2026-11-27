import { defineStore } from "pinia";
import { ref } from "vue";

export type PersonalSecuritySection = "mfa" | "password";

export const usePersonalSecurityStore = defineStore("personalSecurityStore", () => {
  const drawerVisible = ref(false);
  const focusSection = ref<PersonalSecuritySection>("mfa");

  function openDrawer(section: PersonalSecuritySection = "mfa"): void {
    focusSection.value = section;
    drawerVisible.value = true;
  }

  function closeDrawer(): void {
    drawerVisible.value = false;
  }

  return {
    drawerVisible,
    focusSection,
    openDrawer,
    closeDrawer,
  };
});
