import App from "./App.vue";
import { createApp, getCurrentInstance } from "vue";
import { initStore } from "./store";
import { initRouter } from "./router";
import language from "./locales";
import "@styles/core/tailwind.css";
import "@styles/index.scss";
import "@utils/sys/console.ts";
import { setupGlobDirectives } from "./directives";
import { setupErrorHandle } from "./utils/sys/error-handle";
import { registerOfflineIconCollections } from "./utils/ui/iconify-loader";
import { ElForm } from "element-plus";

const isInsideDialog = (inst: any): boolean => {
  let p = inst.parent;
  while (p) {
    const name = p.type.name || p.type.__name || "";
    if (
      name === "ElDialog" ||
      name === "CenteredDialog" ||
      name === "EditDialog" ||
      name.toLowerCase().includes("dialog")
    ) {
      return true;
    }
    p = p.parent;
  }
  return false;
};

const formProps = ElForm.props as any;
const originalLabelPositionDefault = formProps.labelPosition.default;
formProps.labelPosition.default = function () {
  const inst = getCurrentInstance();
  if (inst && isInsideDialog(inst)) {
    return "left";
  }
  return typeof originalLabelPositionDefault === "function"
    ? originalLabelPositionDefault()
    : originalLabelPositionDefault;
};

const originalRequireAsteriskPositionDefault =
  formProps.requireAsteriskPosition.default;
formProps.requireAsteriskPosition.default = function () {
  const inst = getCurrentInstance();
  if (inst && isInsideDialog(inst)) {
    return "right";
  }
  return typeof originalRequireAsteriskPositionDefault === "function"
    ? originalRequireAsteriskPositionDefault()
    : originalRequireAsteriskPositionDefault;
};

document.addEventListener("touchstart", function () {}, { passive: false });
registerOfflineIconCollections();
const app = createApp(App);
initStore(app);
initRouter(app);
setupGlobDirectives(app);
setupErrorHandle(app);
app.use(language);
app.mount("#app");
