import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { relative, resolve } from "node:path";

const SRC_ROOT = resolve(import.meta.dirname, "..");

function walkFiles(dir: string, exts: string[]): string[] {
  const results: string[] = [];
  for (const entry of readdirSync(dir)) {
    const full = resolve(dir, entry);
    const st = statSync(full);
    if (st.isDirectory()) {
      results.push(...walkFiles(full, exts));
    } else if (exts.some((ext) => entry.endsWith(ext))) {
      results.push(full);
    }
  }
  return results;
}

function stripComments(source: string): string {
  const noHtml = source.replace(/<!--[\s\S]*?-->/g, "");
  const noBlock = noHtml.replace(/\/\*[\s\S]*?\*\//g, "");
  return noBlock
    .split("\n")
    .map((line) => {
      let idx = line.indexOf("//");
      while (idx !== -1) {
        if (idx > 0 && line[idx - 1] === ":") {
          idx = line.indexOf("//", idx + 2);
        } else {
          return line.slice(0, idx);
        }
      }
      return line;
    })
    .join("\n");
}

function flattenJson(
  obj: Record<string, unknown>,
  prefix = "",
): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [k, v] of Object.entries(obj)) {
    const key = prefix ? `${prefix}.${k}` : k;
    if (v && typeof v === "object" && !Array.isArray(v)) {
      Object.assign(out, flattenJson(v as Record<string, unknown>, key));
    } else {
      out[key] = String(v ?? "");
    }
  }
  return out;
}

function extractPlaceholders(text: string): string[] {
  return Array.from(new Set([...text.matchAll(/\{(\w+)\}/g)].map((m) => m[1]))).sort();
}

describe("architecture-fitness gates", () => {
  it("Gate 1: zh.json and en.json have symmetric leaf keys, matching placeholders, and clean English", () => {
    const zh = flattenJson(
      JSON.parse(readFileSync(resolve(SRC_ROOT, "locales/langs/zh.json"), "utf8")),
    );
    const en = flattenJson(
      JSON.parse(readFileSync(resolve(SRC_ROOT, "locales/langs/en.json"), "utf8")),
    );

    const zhKeys = new Set(Object.keys(zh));
    const enKeys = new Set(Object.keys(en));
    const missingInEn = [...zhKeys].filter((k) => !enKeys.has(k));
    const missingInZh = [...enKeys].filter((k) => !zhKeys.has(k));
    assert.deepEqual(missingInEn, [], `Keys missing in en.json: ${missingInEn.join(", ")}`);
    assert.deepEqual(missingInZh, [], `Keys missing in zh.json: ${missingInZh.join(", ")}`);

    const cjkRe = /[\u4e00-\u9fff]/;
    const badSuffixRe = /\}\s*ed\b/;
    const enCjk: string[] = [];
    const enBadSuffix: string[] = [];
    const placeholderDiffs: string[] = [];

    for (const key of zhKeys) {
      if (cjkRe.test(en[key])) enCjk.push(key);
      if (badSuffixRe.test(en[key])) enBadSuffix.push(`${key}: ${en[key]}`);
      const pZh = extractPlaceholders(zh[key]);
      const pEn = extractPlaceholders(en[key]);
      if (JSON.stringify(pZh) !== JSON.stringify(pEn)) {
        placeholderDiffs.push(`${key}: zh=[${pZh}] en=[${pEn}]`);
      }
    }

    assert.deepEqual(enCjk, [], `CJK characters found in en.json: ${enCjk.join(", ")}`);
    assert.deepEqual(enBadSuffix, [], `Broken "{x}ed" suffix in en.json: ${enBadSuffix.join(", ")}`);
    assert.deepEqual(placeholderDiffs, [], `Placeholder mismatch: ${placeholderDiffs.join("; ")}`);
  });

  it("Gate 2: zero hardcoded non-comment CJK string literals in src/**/*.{vue,ts}", () => {
    const cjkRe = /[\u4e00-\u9fff]/;
    const allowlist = new Set([
      "utils/list-query/parse-clause.ts",
      "utils/list-query/suggest.ts",
    ]);
    const violations: string[] = [];

    for (const file of walkFiles(SRC_ROOT, [".vue", ".ts"])) {
      const rel = relative(SRC_ROOT, file).replace(/\\/g, "/");
      if (rel.startsWith("locales/") || rel.endsWith(".test.ts") || allowlist.has(rel)) {
        continue;
      }
      const clean = stripComments(readFileSync(file, "utf8"));
      if (cjkRe.test(clean)) {
        violations.push(rel);
      }
    }

    assert.deepEqual(
      violations,
      [],
      `Hardcoded CJK found in: ${violations.join(", ")}`,
    );
  });

  it("Gate 3: single Asia/Shanghai clock SSOT (no local getHours/getMinutes/toLocaleTimeString)", () => {
    const forbiddenRe = /\.(getHours|getMinutes|toLocaleTimeString)\s*\(/;
    const violations: string[] = [];

    for (const file of walkFiles(SRC_ROOT, [".vue", ".ts"])) {
      const rel = relative(SRC_ROOT, file).replace(/\\/g, "/");
      if (rel.endsWith(".test.ts") || rel === "utils/format.ts") continue;
      const clean = stripComments(readFileSync(file, "utf8"));
      if (forbiddenRe.test(clean)) {
        violations.push(rel);
      }
    }

    assert.deepEqual(
      violations,
      [],
      `Direct local clock calls found in: ${violations.join(", ")}`,
    );
  });

  it("Gate 4: single theme pipeline (setGlopTheme restricted to theme/setting/useTheme)", () => {
    const allowed = new Set([
      "store/modules/theme.ts",
      "store/modules/setting.ts",
      "hooks/core/useTheme.ts",
    ]);
    const violations: string[] = [];

    for (const file of walkFiles(SRC_ROOT, [".vue", ".ts"])) {
      const rel = relative(SRC_ROOT, file).replace(/\\/g, "/");
      if (rel.endsWith(".test.ts") || allowed.has(rel)) continue;
      const clean = stripComments(readFileSync(file, "utf8"));
      if (clean.includes("setGlopTheme")) {
        violations.push(rel);
      }
    }

    assert.deepEqual(
      violations,
      [],
      `Unauthorized setGlopTheme usage in: ${violations.join(", ")}`,
    );
  });

  it("Gate 5: no raw ElDialog/ElDrawer in src/views/**/*.vue", () => {
    const rawOverlayRe = /<(ElDialog|el-dialog|ElDrawer|el-drawer)\b/;
    const violations: string[] = [];
    const viewsRoot = resolve(SRC_ROOT, "views");

    for (const file of walkFiles(viewsRoot, [".vue"])) {
      const rel = relative(SRC_ROOT, file).replace(/\\/g, "/");
      const clean = stripComments(readFileSync(file, "utf8"));
      if (rawOverlayRe.test(clean)) {
        violations.push(rel);
      }
    }

    assert.deepEqual(
      violations,
      [],
      `Raw ElDialog/ElDrawer found in views: ${violations.join(", ")}`,
    );
  });
});
