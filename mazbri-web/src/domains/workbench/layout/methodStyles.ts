import { methodTone } from "../engine/workbenchHelpers";

/**
 * Semantic HTTP method badge color classes compatible with light and dark modes.
 */
export function methodBadgeClasses(method?: string): string {
  switch (methodTone(method)) {
    case "get":
      return "text-emerald-700 bg-emerald-500/12 border-emerald-500/25 dark:text-emerald-400 dark:bg-emerald-400/15 dark:border-emerald-400/30";
    case "post":
      return "text-amber-700 bg-amber-500/12 border-amber-500/25 dark:text-amber-400 dark:bg-amber-400/15 dark:border-amber-400/30";
    case "put":
      return "text-blue-700 bg-blue-500/12 border-blue-500/25 dark:text-blue-400 dark:bg-blue-400/15 dark:border-blue-400/30";
    case "patch":
      return "text-violet-700 bg-violet-500/12 border-violet-500/25 dark:text-violet-400 dark:bg-violet-400/15 dark:border-violet-400/30";
    case "delete":
      return "text-red-700 bg-red-500/12 border-red-500/25 dark:text-red-400 dark:bg-red-400/15 dark:border-red-400/30";
    case "head":
      return "text-teal-700 bg-teal-500/12 border-teal-500/25 dark:text-teal-400 dark:bg-teal-400/15 dark:border-teal-400/30";
    case "options":
      return "text-indigo-700 bg-indigo-500/12 border-indigo-500/25 dark:text-indigo-400 dark:bg-indigo-400/15 dark:border-indigo-400/30";
    default:
      return "text-muted-foreground bg-muted border-border";
  }
}

export function methodTextClasses(method?: string): string {
  switch (methodTone(method)) {
    case "get":
      return "text-emerald-700 dark:text-emerald-400";
    case "post":
      return "text-amber-700 dark:text-amber-400";
    case "put":
      return "text-blue-700 dark:text-blue-400";
    case "patch":
      return "text-violet-700 dark:text-violet-400";
    case "delete":
      return "text-red-700 dark:text-red-400";
    case "head":
      return "text-teal-700 dark:text-teal-400";
    case "options":
      return "text-indigo-700 dark:text-indigo-400";
    default:
      return "text-muted-foreground";
  }
}
