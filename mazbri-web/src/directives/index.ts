import type { App } from "vue";
import { setupAuthDirective } from "./core/auth";
import { setupHighlightDirective } from "./business/highlight";
import { setupRippleDirective } from "./business/ripple";
import { setupRolesDirective } from "./core/roles";
export function setupGlobDirectives(app: App) {
  setupAuthDirective(app);
  setupRolesDirective(app);
  setupHighlightDirective(app);
  setupRippleDirective(app);
}
