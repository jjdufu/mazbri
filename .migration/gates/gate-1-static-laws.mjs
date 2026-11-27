#!/usr/bin/env node
/**
 * 门禁 1：前端 React 19 + xBlueNode 静态迁移铁律全量扫描
 * 用法：
 *   node .migration/gates/gate-1-static-laws.mjs
 *   node .migration/gates/gate-1-static-laws.mjs --scope src/domains/tenants,src/domains/billing
 */
import fs from "node:fs";
import path from "node:path";
import { execSync } from "node:child_process";

const repoRoot = execSync("git rev-parse --show-toplevel", { encoding: "utf8" }).trim();
const activeFile = path.join(repoRoot, ".migration", "ACTIVE");
if (!fs.existsSync(activeFile)) {
  console.error("[EXPIRED] .migration/ACTIVE 不存在：迁移已结束，门禁 1 已失效。");
  process.exit(2);
}

const webDir = path.join(repoRoot, "mazbri-web");
const srcDir = path.join(webDir, "src");
const errors = [];

function walk(dir) {
  const out = [];
  if (!fs.existsSync(dir)) return out;
  for (const ent of fs.readdirSync(dir, { withFileTypes: true })) {
    if (ent.name === "node_modules" || ent.name === "dist" || ent.name.startsWith(".")) continue;
    const p = path.join(dir, ent.name);
    if (ent.isDirectory()) out.push(...walk(p));
    else out.push(p);
  }
  return out;
}

// 1. 技术栈洁净度检查（package.json）
const pkgPath = path.join(webDir, "package.json");
if (!fs.existsSync(pkgPath)) {
  errors.push("缺少 mazbri-web/package.json");
} else {
  const pkg = JSON.parse(fs.readFileSync(pkgPath, "utf8"));
  const allDeps = { ...(pkg.dependencies || {}), ...(pkg.devDependencies || {}) };
  const bannedDeps = [
    "vue",
    "vue-router",
    "pinia",
    "vue-i18n",
    "element-plus",
    "@element-plus/icons-vue",
    "unplugin-element-plus",
    "unplugin-vue-components",
    "@vitejs/plugin-vue",
    "sass",
    "antd",
    "naive-ui",
    "@arco-design/web-react",
  ];
  for (const dep of bannedDeps) {
    if (dep in allDeps) {
      errors.push(`package.json 包含违禁依赖：${dep}`);
    }
  }
  if (!allDeps.react || !allDeps["react-dom"]) {
    errors.push("package.json 必须使用 React 19 (react / react-dom)");
  }
  if (!allDeps["@tailwindcss/vite"] && !allDeps.tailwindcss) {
    errors.push("package.json 必须启用 Tailwind CSS v4");
  }
}

// 2. 源码级铁律扫描
const files = walk(srcDir);
const tKeysUsed = new Set();

for (const abs of files) {
  const rel = path.relative(webDir, abs);
  if (rel.endsWith(".vue") || rel.endsWith(".scss")) {
    errors.push(`违禁旧格式文件残留：${rel}`);
    continue;
  }
  if (!/\.(ts|tsx|css)$/.test(rel)) continue;
  const text = fs.readFileSync(abs, "utf8");
  const lines = text.split("\n");

  // 禁止压制告警/报错
  lines.forEach((line, idx) => {
    const ln = idx + 1;
    if (/@ts-ignore|@ts-expect-error|eslint-disable/.test(line)) {
      errors.push(`${rel}:${ln} 禁止使用 @ts-ignore / @ts-expect-error / eslint-disable`);
    }
    if (/\b(?:el-[a-z]+|ElDialog|ElDrawer|ElButton|ArtBreadcrumb|ArtDragVerify)\b/.test(line) && !rel.includes("test")) {
      errors.push(`${rel}:${ln} 检测到旧框架 Element Plus / Art Design Pro 残留标识`);
    }
    if (/calc\([^)]*--custom-radius\s*\/\s*3/.test(line)) {
      errors.push(`${rel}:${ln} 禁止亚像素圆角除法公式`);
    }
  });

  // 业务域（src/domains/）专项样式与原语铁律
  if (rel.startsWith("src/domains/") && rel.endsWith(".tsx")) {
    lines.forEach((line, idx) => {
      const ln = idx + 1;
      if (/\bbg-white\b|bg-\[#fff(?:fff)?\]|bg-\[#f5f7fa\]|bg-\[#f4f6f9\]/i.test(line)) {
        errors.push(`${rel}:${ln} 业务域严禁硬编码亮色背景（${line.trim().slice(0, 60)}），请改用 bg-card / bg-canvas-bg / bg-muted`);
      }
      if (/<dialog\b/.test(line)) {
        errors.push(`${rel}:${ln} 业务域严禁裸写 <dialog>，必须复用 src/components/ui/ 中的 Modal / FormModal`);
      }
      if (/\b(rounded-2xl|rounded-3xl)\b/.test(line) && /Button|Input|Select|button|input/.test(line)) {
        errors.push(`${rel}:${ln} 按钮/输入控件圆角超出白名单（仅允许 rounded-md）`);
      }
    });
  }

  // 收集静态字面量 t("a.b.c") 调用以核验词条完整性
  if (/\.(ts|tsx)$/.test(rel) && !rel.endsWith(".test.ts")) {
    for (const m of text.matchAll(/\bt\(\s*["']([a-zA-Z0-9_.-]+)["']/g)) {
      const k = m[1];
      if (k.includes(".") && !k.endsWith(".")) tKeysUsed.add(k);
    }
  }
}

// 3. i18n 双语字典全量存在性核验
function flattenJson(obj, prefix = "", out = new Set()) {
  if (!obj || typeof obj !== "object") return out;
  for (const [k, v] of Object.entries(obj)) {
    const key = prefix ? `${prefix}.${k}` : k;
    if (v && typeof v === "object" && !Array.isArray(v)) flattenJson(v, key, out);
    else out.add(key);
  }
  return out;
}

const localeDir = path.join(srcDir, "locales");
const zhFiles = walk(localeDir).filter((f) => /zh.*\.json$/i.test(f));
const enFiles = walk(localeDir).filter((f) => /en.*\.json$/i.test(f));
if (tKeysUsed.size > 0 && (zhFiles.length > 0 || enFiles.length > 0)) {
  const zhKeys = new Set();
  const enKeys = new Set();
  for (const f of zhFiles) flattenJson(JSON.parse(fs.readFileSync(f, "utf8")), "", zhKeys);
  for (const f of enFiles) flattenJson(JSON.parse(fs.readFileSync(f, "utf8")), "", enKeys);

  for (const k of tKeysUsed) {
    if (zhKeys.size > 0 && !zhKeys.has(k)) errors.push(`中文词条缺失：${k}`);
    if (enKeys.size > 0 && !enKeys.has(k)) errors.push(`英文词条缺失：${k}`);
  }
}

// 4. 可选 Subagent 目录越界校验（--scope dir1,dir2）
const args = process.argv.slice(2);
const scopeIdx = args.indexOf("--scope");
if (scopeIdx !== -1 && args[scopeIdx + 1]) {
  const allowed = args[scopeIdx + 1]
    .split(",")
    .map((s) => s.trim().replace(/^\/+|\/+$/g, ""))
    .filter(Boolean);
  const changed = execSync("git status --porcelain", { cwd: repoRoot, encoding: "utf8" })
    .split("\n")
    .filter(Boolean)
    .map((l) => l.slice(3).trim().replace(/^"|"$/g, ""));
  for (const f of changed) {
    const ok = allowed.some((prefix) => f === prefix || f.startsWith(prefix + "/") || f.startsWith("mazbri-web/" + prefix + "/"));
    if (!ok) {
      errors.push(`Subagent 文件所有权越界：修改了非授权路径 ${f}（授权范围：${allowed.join(", ")}）`);
    }
  }
}

if (errors.length > 0) {
  console.error(`❌ [GATE-1 FAILED] 共发现 ${errors.length} 项迁移铁律违规：`);
  for (const e of errors.slice(0, 60)) console.error("  - " + e);
  if (errors.length > 60) console.error(`  ... 其余 ${errors.length - 60} 项已省略`);
  process.exit(1);
}

console.log(`✅ [GATE-1 PASSED] 已扫描 ${files.length} 个源码文件、核验 ${tKeysUsed.size} 个静态 i18n Key，零违规。`);
