export interface RgbaResult {
  red: number;
  green: number;
  blue: number;
  rgba: string;
}

export function getCssVar(name: string): string {
  if (typeof document === "undefined") return "";
  return getComputedStyle(document.documentElement).getPropertyValue(name);
}

function isValidHexColor(hex: string | null | undefined): hex is string {
  if (typeof hex !== "string") return false;
  const cleanHex = hex.trim().replace(/^#/, "");
  return /^[0-9A-Fa-f]{3}$|^[0-9A-Fa-f]{6}$/.test(cleanHex);
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
    throw new Error("Invalid hex color format");
  }
  let hex = hexColor.replace(/^#/, "");
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
