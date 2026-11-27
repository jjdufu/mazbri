import { create } from "zustand";

export type ThemeMode = "auto" | "light" | "dark";

const THEME_STORAGE_KEY = "mazbri-theme";
const LEGACY_THEME_STORAGE_KEY = "sys-theme";

function normalizeThemeMode(value: unknown): ThemeMode | null {
  if (value === "light" || value === "dark" || value === "auto") {
    return value;
  }
  if (value === "system") {
    return "auto";
  }
  return null;
}

function getSystemPrefersDark(): boolean {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
    return false;
  }
  return window.matchMedia("(prefers-color-scheme: dark)").matches;
}

function resolveIsDark(mode: ThemeMode): boolean {
  if (mode === "dark") return true;
  if (mode === "light") return false;
  return getSystemPrefersDark();
}

function readInitialTheme(): ThemeMode {
  if (typeof localStorage === "undefined") {
    return "auto";
  }
  try {
    const primary = normalizeThemeMode(localStorage.getItem(THEME_STORAGE_KEY));
    if (primary) {
      return primary;
    }
    const legacy = normalizeThemeMode(
      localStorage.getItem(LEGACY_THEME_STORAGE_KEY),
    );
    if (legacy) {
      return legacy;
    }
  } catch {
    // Fallback to auto theme when storage is restricted.
  }
  return "auto";
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
const initialIsDark = resolveIsDark(initialTheme);
syncThemeClass(initialIsDark);

export const useThemeStore = create<ThemeState>((set, get) => ({
  theme: initialTheme,
  isDark: initialIsDark,
  setTheme: (theme: ThemeMode) => {
    const normalized = normalizeThemeMode(theme);
    if (!normalized) return;
    const isDark = resolveIsDark(normalized);
    syncThemeClass(isDark);
    persistTheme(normalized);
    set({ theme: normalized, isDark });
  },
  toggleTheme: () => {
    const cur = get().theme;
    const next: ThemeMode =
      cur === "auto" ? "light" : cur === "light" ? "dark" : "auto";
    get().setTheme(next);
  },
  applyThemeToDom: () => {
    const isDark = resolveIsDark(get().theme);
    syncThemeClass(isDark);
    if (isDark !== get().isDark) {
      set({ isDark });
    }
  },
}));

if (typeof window !== "undefined" && typeof window.matchMedia === "function") {
  const media = window.matchMedia("(prefers-color-scheme: dark)");
  const handleSystemChange = () => {
    const state = useThemeStore.getState();
    if (state.theme === "auto") {
      const nextDark = media.matches;
      syncThemeClass(nextDark);
      useThemeStore.setState({ isDark: nextDark });
    }
  };
  if (typeof media.addEventListener === "function") {
    media.addEventListener("change", handleSystemChange);
  }
}
