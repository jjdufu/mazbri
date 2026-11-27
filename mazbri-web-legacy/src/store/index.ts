import type { App } from "vue";
import { createPinia } from "pinia";
import { createPersistedState } from "pinia-plugin-persistedstate";
import { StorageKeyManager } from "@/utils/storage/storage-key-manager";
export const store = createPinia();
const storageKeyManager = new StorageKeyManager();
store.use(
  createPersistedState({
    key: (storeId: string) => storageKeyManager.getStorageKey(storeId),
    storage: localStorage,
    serializer: {
      serialize: JSON.stringify,
      deserialize: JSON.parse,
    },
  }),
);
export function initStore(app: App<Element>): void {
  app.use(store);
}
export { useThemeStore } from "./modules/theme";
export { useLayoutStore } from "./modules/layout";
export { useDisplayStore } from "./modules/display";
