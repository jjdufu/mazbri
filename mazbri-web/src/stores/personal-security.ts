import { create } from "zustand";

export type PersonalSecuritySection = "mfa" | "password";

export interface PersonalSecurityState {
  open: boolean;
  focusSection: PersonalSecuritySection;
  openDrawer: (section?: PersonalSecuritySection) => void;
  closeDrawer: () => void;
}

export const usePersonalSecurityStore = create<PersonalSecurityState>(
  (set) => ({
    open: false,
    focusSection: "mfa",
    openDrawer: (section: PersonalSecuritySection = "mfa") => {
      set({ open: true, focusSection: section });
    },
    closeDrawer: () => {
      set({ open: false });
    },
  }),
);
