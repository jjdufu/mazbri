import type { App } from "vue";
import { setupRippleDirective } from "./business/ripple";
import { setupRolesDirective } from "./core/roles";
export function setupGlobDirectives(app: App) {
  setupRolesDirective(app);
  setupRippleDirective(app);
}
