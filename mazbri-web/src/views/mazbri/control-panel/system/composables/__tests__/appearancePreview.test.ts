import assert from "node:assert/strict";
import { nextTick, reactive } from "vue";
import {
  createAppearancePreview,
  isSameAppearance,
} from "../appearancePreview";

interface Appearance {
  systemThemeColor: string;
  containerWidth: string;
  showWorkTab: boolean;
  menuOpenWidth: number;
  tabStyle: string;
}

const server: Appearance = {
  systemThemeColor: "#5D87FF",
  containerWidth: "100%",
  showWorkTab: false,
  menuOpenWidth: 200,
  tabStyle: "tab-default",
};

async function main() {
  assert.ok(isSameAppearance(server, { ...server }));
  assert.ok(!isSameAppearance(server, { ...server, menuOpenWidth: 210 }));

  // In-memory "store" that the preview writes into (no server involved).
  let store: Appearance = { ...server };
  let applyCount = 0;
  let active = true;
  const local = reactive<Appearance>({ ...server });
  const preview = createAppearancePreview(local, {
    read: () => ({ ...store }),
    apply: (values) => {
      applyCount += 1;
      store = { ...values };
    },
    isActive: () => active,
  });

  // 1. Every field previews immediately (after the watcher flush).
  local.containerWidth = "1200px";
  local.showWorkTab = true;
  local.systemThemeColor = "#FF0000";
  await nextTick();
  assert.equal(store.containerWidth, "1200px");
  assert.equal(store.showWorkTab, true);
  assert.equal(store.systemThemeColor, "#FF0000");
  assert.equal(applyCount, 1, "batched into a single apply");
  assert.ok(preview.isDirty());
  assert.deepEqual(preview.getBaseline(), server, "baseline untouched");

  // 2. No apply when local already matches the store (no loops).
  Object.assign(local, store);
  await nextTick();
  assert.equal(applyCount, 1);

  // 3. Leaving without saving reverts store + local to the baseline.
  preview.revert();
  assert.deepEqual(store, server);
  assert.deepEqual({ ...local }, server);
  assert.ok(!preview.isDirty());
  await nextTick();
  assert.equal(applyCount, 2, "revert applied once, watcher did not re-apply");

  // 4. Save: store now holds the saved values -> commit makes them baseline.
  local.menuOpenWidth = 240;
  local.tabStyle = "tab-card";
  await nextTick();
  assert.equal(store.menuOpenWidth, 240);
  preview.commit();
  assert.ok(!preview.isDirty());
  local.menuOpenWidth = 260;
  await nextTick();
  assert.equal(store.menuOpenWidth, 260);
  preview.revert();
  assert.equal(store.menuOpenWidth, 240, "reverts to last saved, not server");
  assert.equal(store.tabStyle, "tab-card");
  assert.equal(local.menuOpenWidth, 240);

  // 5. Inactive page: local changes are not previewed.
  active = false;
  const before = applyCount;
  local.showWorkTab = true;
  await nextTick();
  assert.equal(applyCount, before);
  assert.equal(store.showWorkTab, false);

  // 6. Stopping the watcher stops previews.
  active = true;
  preview.stop();
  local.containerWidth = "1200px";
  await nextTick();
  assert.equal(store.containerWidth, "100%");

  console.log("appearancePreview tests passed");
}

void main();
