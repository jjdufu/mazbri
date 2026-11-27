import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

function expandHex(hex: string): [number, number, number] {
  const clean = hex.replace(/^#/, "");
  const full =
    clean.length === 3
      ? clean
          .split("")
          .map((c) => c + c)
          .join("")
      : clean;
  return [
    parseInt(full.slice(0, 2), 16),
    parseInt(full.slice(2, 4), 16),
    parseInt(full.slice(4, 6), 16),
  ];
}

function relativeLuminance(hex: string): number {
  const [r, g, b] = expandHex(hex).map((v) => {
    const s = v / 255;
    return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrastRatio(fg: string, bg: string): number {
  const l1 = relativeLuminance(fg);
  const l2 = relativeLuminance(bg);
  const lighter = Math.max(l1, l2);
  const darker = Math.min(l1, l2);
  return (lighter + 0.05) / (darker + 0.05);
}

function parseTokens(block: string): Record<string, string> {
  const result: Record<string, string> = {};
  for (const match of block.matchAll(/(--wb-[a-z-]+)\s*:\s*(#[0-9a-fA-F]{3,6})\s*;/g)) {
    result[match[1]] = match[2];
  }
  return result;
}

describe("workbench-tokens WCAG 2.1 AA contrast", () => {
  const cssPath = resolve(
    import.meta.dirname,
    "../workbench-tokens.scss",
  );
  const css = readFileSync(cssPath, "utf8");
  const lightMatch = css.match(/^\.connector-workbench-page\s*\{([\s\S]*?)^\}/m);
  const darkMatch = css.match(/^html\.dark\s+\.connector-workbench-page\s*\{([\s\S]*?)^\}/m);

  it("satisfies >= 4.5:1 contrast for --wb-text-faint on all light surfaces", () => {
    assert.ok(lightMatch, "light token block found");
    const tokens = parseTokens(lightMatch[1]);
    const fg = tokens["--wb-text-faint"];
    assert.ok(fg, "--wb-text-faint defined in light block");
    for (const surface of [
      "--wb-bg",
      "--wb-surface",
      "--wb-surface-input",
      "--wb-surface-hover",
    ]) {
      const bg = tokens[surface];
      assert.ok(bg, `${surface} defined in light block`);
      const ratio = contrastRatio(fg, bg);
      assert.ok(
        ratio >= 4.5,
        `light ${fg} on ${surface}(${bg}) ratio ${ratio.toFixed(2)} < 4.5`,
      );
    }
  });

  it("satisfies >= 4.5:1 contrast for --wb-text-faint on all dark surfaces", () => {
    assert.ok(darkMatch, "dark token block found");
    const tokens = parseTokens(darkMatch[1]);
    const fg = tokens["--wb-text-faint"];
    assert.ok(fg, "--wb-text-faint defined in dark block");
    for (const surface of [
      "--wb-bg",
      "--wb-surface",
      "--wb-surface-input",
      "--wb-surface-hover",
    ]) {
      const bg = tokens[surface];
      assert.ok(bg, `${surface} defined in dark block`);
      const ratio = contrastRatio(fg, bg);
      assert.ok(
        ratio >= 4.5,
        `dark ${fg} on ${surface}(${bg}) ratio ${ratio.toFixed(2)} < 4.5`,
      );
    }
  });
});
