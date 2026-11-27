




import { addCollection } from "@iconify/vue/offline";
import { offlineIconCollections } from "./iconify-offline-data";

let isOfflineIconCollectionsRegistered = false;

export function registerOfflineIconCollections(): void {
  if (isOfflineIconCollectionsRegistered) return;
  offlineIconCollections.forEach((collection) => {
    addCollection(collection as unknown as Parameters<typeof addCollection>[0]);
  });
  isOfflineIconCollectionsRegistered = true;
}
