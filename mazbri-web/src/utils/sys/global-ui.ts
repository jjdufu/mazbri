import { ref } from "vue";
export const globalUiTriggers = {
  lockScreen: ref(0),
};
export const triggerLockScreen = () => {
  globalUiTriggers.lockScreen.value += 1;
};
