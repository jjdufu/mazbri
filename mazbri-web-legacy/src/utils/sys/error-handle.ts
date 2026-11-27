import type { App } from "vue";
import { logger } from "./logger";
export function vueErrorHandler(err: unknown, instance: unknown, info: string) {
  logger.error("[VueError]", err, info, instance);
}
export function scriptErrorHandler(
  message: Event | string,
  source?: string,
  lineno?: number,
  colno?: number,
  error?: Error,
): boolean {
  logger.error("[ScriptError]", { message, source, lineno, colno, error });
  return true;
}
export function registerPromiseErrorHandler() {
  window.addEventListener("unhandledrejection", (event) => {
    logger.error("[PromiseError]", event.reason);
  });
}
export function registerResourceErrorHandler() {
  window.addEventListener(
    "error",
    (event: Event) => {
      const target = event.target as HTMLElement;
      if (
        target &&
        (target.tagName === "IMG" ||
          target.tagName === "SCRIPT" ||
          target.tagName === "LINK")
      ) {
        logger.error("[ResourceError]", {
          tagName: target.tagName,
          src:
            (target as HTMLImageElement).src ||
            (target as HTMLScriptElement).src ||
            (target as HTMLLinkElement).href,
        });
      }
    },
    true,
  );
}
export function setupErrorHandle(app: App) {
  app.config.errorHandler = vueErrorHandler;
  window.onerror = scriptErrorHandler;
  registerPromiseErrorHandler();
  registerResourceErrorHandler();
}
