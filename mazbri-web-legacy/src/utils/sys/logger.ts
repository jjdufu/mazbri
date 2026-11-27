type LogLevel = "debug" | "info" | "warn" | "error";
export interface LogPayload {
  service?: string;
  event?: string;
  severity?: string;
  error_code?: string;
  trace_id?: string;
  [key: string]: unknown;
}
interface Logger {
  debug(message: string, ...args: unknown[]): void;
  info(message: string, ...args: unknown[]): void;
  warn(message: string, payload?: unknown, ...args: unknown[]): void;
  error(message: string, payload?: unknown, ...args: unknown[]): void;
}
function formatMessage(level: LogLevel, message: string): string {
  return `[${level.toUpperCase()}] ${message}`;
}
export const logger: Logger = {
  debug(message: string, ...args: unknown[]): void {
    if (import.meta.env.DEV) {
      console.log(formatMessage("debug", message), ...args);
    }
  },
  info(message: string, ...args: unknown[]): void {
    if (import.meta.env.DEV) {
      console.info(formatMessage("info", message), ...args);
    }
  },
  warn(message: string, payload?: unknown, ...args: unknown[]): void {
    if (payload) {
      console.warn(formatMessage("warn", message), payload, ...args);
    } else {
      console.warn(formatMessage("warn", message), ...args);
    }
  },
  error(message: string, payload?: unknown, ...args: unknown[]): void {
    if (payload) {
      console.error(formatMessage("error", message), payload, ...args);
    } else {
      console.error(formatMessage("error", message), ...args);
    }
  },
};
export type { Logger, LogLevel };
