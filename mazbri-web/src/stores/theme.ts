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
  try {
    if (
      typeof window !== "undefined" &&
      typeof window.matchMedia === "function"
    ) {
      return window.matchMedia("(prefers-color-scheme: dark)").matches;
    }
  } catch {
    // Ignore matchMedia errors in non-DOM test environments.
  }
  return false;
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

function syncThemeDom(isDark: boolean, freezeTransitions = true): void {
  if (typeof document === "undefined" || !document.documentElement) return;
  const docEl = document.documentElement;
  const resolvedScheme = isDark ? "dark" : "light";

  let styleEl: HTMLStyleElement | null = null;
  if (freezeTransitions && document.head) {
    styleEl = document.createElement("style");
    styleEl.setAttribute("data-mazbri-theme-freeze", "1");
    styleEl.textContent =
      "*, *::before, *::after { transition: none !important; animation-duration: 0.001ms !important; }";
    document.head.appendChild(styleEl);
  }

  docEl.classList.toggle("dark", isDark);
  docEl.style.colorScheme = resolvedScheme;

  if (freezeTransitions && document.body && typeof window !== "undefined") {
    void window.getComputedStyle(document.body).backgroundColor;
  }

  if (styleEl) {
    const cleanup = () => {
      styleEl?.remove();
    };
    if (
      typeof window !== "undefined" &&
      typeof window.requestAnimationFrame === "function"
    ) {
      window.requestAnimationFrame(() => {
        window.requestAnimationFrame(cleanup);
      });
    } else {
      cleanup();
    }
  }
}

function persistTheme(theme: ThemeMode): void {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(THEME_STORAGE_KEY, theme);
  } catch {
    // Ignore storage quota or privacy mode errors.
  }
}

export function formatThemeTooltip(theme: ThemeMode, isZh: boolean): string {
  if (theme === "auto") {
    return isZh ? "跟随系统" : "Auto";
  }
  if (theme === "dark") {
    return isZh ? "暗色" : "Dark";
  }
  return isZh ? "亮色" : "Light";
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
syncThemeDom(initialIsDark, false);

export const useThemeStore = create<ThemeState>((set, get) => ({
  theme: initialTheme,
  isDark: initialIsDark,
  setTheme: (theme: ThemeMode) => {
    const normalized = normalizeThemeMode(theme);
    if (!normalized) return;
    const isDark = resolveIsDark(normalized);
    syncThemeDom(isDark, true);
    persistTheme(normalized);
    set({ theme: normalized, isDark });
  },
  toggleTheme: () => {
    const cur = get().theme;
    const sysDark = getSystemPrefersDark();
    let next: ThemeMode;
    if (cur === "auto") {
      next = sysDark ? "light" : "dark";
    } else if (sysDark) {
      next = cur === "light" ? "dark" : "auto";
    } else {
      next = cur === "dark" ? "light" : "auto";
    }
    get().setTheme(next);
  },
  applyThemeToDom: () => {
    const isDark = resolveIsDark(get().theme);
    syncThemeDom(isDark, false);
    if (isDark !== get().isDark) {
      set({ isDark });
    }
  },
}));

if (typeof window !== "undefined" && typeof window.matchMedia === "function") {
  try {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const handleSystemChange = (ev: MediaQueryListEvent) => {
      const state = useThemeStore.getState();
      if (state.theme === "auto") {
        const nextDark = ev.matches;
        if (nextDark !== state.isDark) {
          syncThemeDom(nextDark, true);
          useThemeStore.setState({ isDark: nextDark });
        }
      }
    };
    if (typeof media.addEventListener === "function") {
      media.addEventListener("change", handleSystemChange);
    }
  } catch {
    // Ignore listener setup errors.
  }
}
