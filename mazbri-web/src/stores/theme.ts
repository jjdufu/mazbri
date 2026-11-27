import { create } from "zustand";

export type ThemeMode = "light" | "dark";

const THEME_STORAGE_KEY = "mazbri-theme";
const LEGACY_THEME_STORAGE_KEY = "sys-theme";

function isThemeMode(value: unknown): value is ThemeMode {
  return value === "light" || value === "dark";
}

function readInitialTheme(): ThemeMode {
  if (typeof localStorage === "undefined") {
    return "light";
  }
  try {
    const primary = localStorage.getItem(THEME_STORAGE_KEY);
    if (isThemeMode(primary)) {
      return primary;
    }
    const legacy = localStorage.getItem(LEGACY_THEME_STORAGE_KEY);
    if (isThemeMode(legacy)) {
      return legacy;
    }
  } catch {
    // Fallback to light theme when storage is restricted.
  }
  return "light";
}

function syncThemeClass(isDark: boolean): void {
  if (typeof document === "undefined") return;
  document.documentElement.classList.toggle("dark", isDark);
}

function persistTheme(theme: ThemeMode): void {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(THEME_STORAGE_KEY, theme);
  } catch {
    // Ignore storage quota or privacy mode errors.
  }
}

export interface ThemeState {
  theme: ThemeMode;
  isDark: boolean;
  setTheme: (theme: ThemeMode) => void;
  toggleTheme: () => void;
  applyThemeToDom: () => void;
}

const initialTheme = readInitialTheme();
syncThemeClass(initialTheme === "dark");

export const useThemeStore = create<ThemeState>((set, get) => ({
  theme: initialTheme,
  isDark: initialTheme === "dark",
  setTheme: (theme: ThemeMode) => {
    if (!isThemeMode(theme)) return;
    const isDark = theme === "dark";
    syncThemeClass(isDark);
    persistTheme(theme);
    set({ theme, isDark });
  },
  toggleTheme: () => {
    const next: ThemeMode = get().isDark ? "light" : "dark";
    get().setTheme(next);
  },
  applyThemeToDom: () => {
    syncThemeClass(get().isDark);
  },
}));
