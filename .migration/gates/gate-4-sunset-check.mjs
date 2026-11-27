#!/usr/bin/env node
/**
 * 门禁 4：Stage 7 迁移终结退场校验
 * 通过后执行 `rm -rf .migration/`，所有迁移期临时铁律即永久物理失效。
 * 用法：node .migration/gates/gate-4-sunset-check.mjs
 */
import fs from "node:fs";
import path from "node:path";
import { execSync } from "node:child_process";

const repoRoot = execSync("git rev-parse --show-toplevel", { encoding: "utf8" }).trim();
const activeFile = path.join(repoRoot, ".migration", "ACTIVE");
if (!fs.existsSync(activeFile)) {
  console.error("[EXPIRED] .migration/ACTIVE 不存在：迁移已结束，门禁 4 已失效。");
  process.exit(2);
}

const failures = [];
const run = (cmd, cwd = repoRoot) => execSync(cmd, { cwd, stdio: "inherit" });

// 1. 确认 legacy 目录已彻底删除
if (fs.existsSync(path.join(repoRoot, "mazbri-web-legacy"))) {
  failures.push("mazbri-web-legacy/ 尚未删除，请先执行彻底清理。");
}

// 2. 确认功能保真账本全部勾选
const ledgerPath = path.join(repoRoot, ".migration", "FEATURE_PARITY_LEDGER.md");
if (fs.existsSync(ledgerPath)) {
  const unchecked = fs
    .readFileSync(ledgerPath, "utf8")
    .split("\n")
    .filter((l) => /^\s*-\s*\[\s\]/.test(l));
  if (unchecked.length > 0) {
    failures.push(`FEATURE_PARITY_LEDGER.md 仍有 ${unchecked.length} 项未核销（首项：${unchecked[0].trim()}）`);
  }
}

// 3. 确认 AGENTS.md 已移除迁移哨兵块并替换为永久版 React 19 UI 规范
const agentsPath = path.join(repoRoot, "AGENTS.md");
const agentsText = fs.readFileSync(agentsPath, "utf8");
if (agentsText.includes("migration-iron-laws:begin") || agentsText.includes(".migration/ACTIVE")) {
  failures.push("AGENTS.md 仍包含迁移期哨兵块，请先移除 <!-- migration-iron-laws:begin/end -->");
}
if (/ArtBreadcrumb|Element Plus|ElDialog|CenteredDialog|AdaptiveDrawer|vue-tsc/.test(agentsText)) {
  failures.push("AGENTS.md 仍残留旧 Vue / Element Plus / Art Design Pro 规范，请替换为永久 React 19 UI 规范");
}
if (!agentsText.includes("React 19") || !agentsText.includes("DataTablePage")) {
  failures.push("AGENTS.md 第三节尚未替换为 POST_MIGRATION_AGENTS_SECTION3.md 永久成稿");
}

if (failures.length > 0) {
  console.error("❌ [GATE-4 PRE-CHECK FAILED]");
  for (const f of failures) console.error("  - " + f);
  process.exit(1);
}

// 4. 终态全量构建、静态检查与双 Workspace Rust Clippy/Test
try {
  console.log("=== [GATE-4] 运行前端静态铁律 + 构建 + 全仓测试 ===");
  run("node .migration/gates/gate-1-static-laws.mjs");
  run("pnpm exec tsc --noEmit && pnpm build && pnpm test", path.join(repoRoot, "mazbri-web"));

  console.log("=== [GATE-4] 运行 Rust 双 Workspace 严格 Clippy + 全量测试 ===");
  run("cargo clippy --release --locked --workspace --all-targets -- -D warnings", path.join(repoRoot, "mazbri-libs"));
  run("cargo test --release --locked --workspace --no-fail-fast", path.join(repoRoot, "mazbri-libs"));
  run("cargo clippy --release --locked --workspace --all-targets -- -D warnings", path.join(repoRoot, "mazbri-bin"));
  run("cargo test --release --locked --workspace --no-fail-fast", path.join(repoRoot, "mazbri-bin"));
  run("git diff --exit-code -- mazbri-web/src/types/api/generated");
} catch (err) {
  console.error("❌ [GATE-4 BUILD/TEST FAILED]", err.message);
  process.exit(1);
}

console.log(`
✅ [GATE-4 PASSED] 全部终结退场门禁 100% 通过！
👉 立即执行以下收尾动作以正式失效并销毁迁移铁律：
   1. rm -rf .migration/
   2. 清理 .gitignore 中 .migration/ 相关临时条目
   3. 执行最终热替换、深度磁盘清理与本地终态 Commit
`);
