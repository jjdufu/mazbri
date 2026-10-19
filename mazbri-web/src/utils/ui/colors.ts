import { useSettingStore } from "@/store/modules/setting";
import { $t } from "@/locales";
import { SETTING_DEFAULT_CONFIG } from "@/config/setting";
interface RgbaResult {
  red: number;
  green: number;
  blue: number;
  rgba: string;
}
export function getCssVar(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name);
}
function isValidHexColor(hex: string | null | undefined): hex is string {
  if (typeof hex !== "string") {
    return false;
  }
  const cleanHex = hex.trim().replace(/^#/, "");
  return /^[0-9A-Fa-f]{3}$|^[0-9A-Fa-f]{6}$/.test(cleanHex);
}
export function normalizeThemeColor(color?: string | null): string {
  return isValidHexColor(color)
    ? color.trim()
    : SETTING_DEFAULT_CONFIG.systemThemeColor;
}
function isValidRgbValue(r: number, g: number, b: number): boolean {
  const isValid = (value: number) =>
    Number.isInteger(value) && value >= 0 && value <= 255;
  return isValid(r) && isValid(g) && isValid(b);
}
export function hexToRgba(hex: string, opacity: number): RgbaResult {
  if (!isValidHexColor(hex)) {
    throw new Error("Invalid hex color format");
  }
  let cleanHex = hex.trim().replace(/^#/, "").toUpperCase();
  if (cleanHex.length === 3) {
    cleanHex = cleanHex
      .split("")
      .map((char) => char.repeat(2))
      .join("");
  }
  const [red, green, blue] = cleanHex
    .match(/\w\w/g)!
    .map((x) => parseInt(x, 16));
  const validOpacity = Math.max(0, Math.min(1, opacity));
  const rgba = `rgba(${red}, ${green}, ${blue}, ${validOpacity.toFixed(2)})`;
  return { red, green, blue, rgba };
}
export function hexToRgb(hexColor: string): number[] {
  if (!isValidHexColor(hexColor)) {
    ElMessage.warning($t("feedback.validation.invalidHexColor"));
    throw new Error("Invalid hex color format");
  }
  const cleanHex = hexColor.replace(/^#/, "");
  let hex = cleanHex;
  if (hex.length === 3) {
    hex = hex
      .split("")
      .map((char) => char.repeat(2))
      .join("");
  }
  const hexPairs = hex.match(/../g);
  if (!hexPairs) {
    throw new Error("Invalid hex color format");
  }
  return hexPairs.map((hexPair) => parseInt(hexPair, 16));
}
export function rgbToHex(r: number, g: number, b: number): string {
  if (!isValidRgbValue(r, g, b)) {
    ElMessage.warning($t("feedback.validation.invalidRgbColor"));
    throw new Error("Invalid RGB color values");
  }
  const toHex = (value: number) => {
    const hex = value.toString(16);
    return hex.length === 1 ? `0${hex}` : hex;
  };
  return `#${toHex(r)}${toHex(g)}${toHex(b)}`;
}
export function colourBlend(
  color1: string,
  color2: string,
  ratio: number,
): string {
  const validRatio = Math.max(0, Math.min(1, Number(ratio)));
  const rgb1 = hexToRgb(color1);
  const rgb2 = hexToRgb(color2);
  const blendedRgb = rgb1.map((value1, index) => {
    const value2 = rgb2[index];
    return Math.round(value1 * (1 - validRatio) + value2 * validRatio);
  });
  return rgbToHex(blendedRgb[0], blendedRgb[1], blendedRgb[2]);
}
export function getLightColor(
  color: string,
  level: number,
  isDark: boolean = false,
): string {
  if (!isValidHexColor(color)) {
    ElMessage.warning($t("feedback.validation.invalidHexColor"));
    throw new Error("Invalid hex color format");
  }
  if (isDark) {
    return getDarkColor(color, level);
  }
  const rgb = hexToRgb(color);
  const lightRgb = rgb.map((value) =>
    Math.floor((255 - value) * level + value),
  );
  return rgbToHex(lightRgb[0], lightRgb[1], lightRgb[2]);
}
export function getDarkColor(color: string, level: number): string {
  if (!isValidHexColor(color)) {
    ElMessage.warning($t("feedback.validation.invalidHexColor"));
    throw new Error("Invalid hex color format");
  }
  const rgb = hexToRgb(color);
  const darkRgb = rgb.map((value) => Math.floor(value * (1 - level)));
  return rgbToHex(darkRgb[0], darkRgb[1], darkRgb[2]);
}
export function handleElementThemeColor(
  theme: string,
  isDark: boolean = false,
): void {
  const normalizedTheme = normalizeThemeColor(theme);
  document.documentElement.style.setProperty(
    "--el-color-primary",
    normalizedTheme,
  );
  for (let i = 1; i <= 9; i++) {
    document.documentElement.style.setProperty(
      `--el-color-primary-light-${i}`,
      getLightColor(normalizedTheme, i / 10, isDark),
    );
  }
  for (let i = 1; i <= 9; i++) {
    document.documentElement.style.setProperty(
      `--el-color-primary-dark-${i}`,
      getDarkColor(normalizedTheme, i / 10),
    );
  }
}
export function setElementThemeColor(color?: string | null): void {
  const normalizedColor = normalizeThemeColor(color);
  const mixColor = "#ffffff";
  const elStyle = document.documentElement.style;
  elStyle.setProperty("--el-color-primary", normalizedColor);
  handleElementThemeColor(normalizedColor, useSettingStore().isDark);
  for (let i = 1; i < 16; i++) {
    const itemColor = colourBlend(normalizedColor, mixColor, i / 16);
    elStyle.setProperty(`--el-color-primary-custom-${i}`, itemColor);
  }
}
