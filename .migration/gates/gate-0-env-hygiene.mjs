#!/usr/bin/env node
/**
 * 门禁 0：敏感凭据防入库、发版元数据保护与 Git 卫生校验
 * 用法：node .migration/gates/gate-0-env-hygiene.mjs
 */
import fs from "node:fs";
import path from "node:path";
import { execSync } from "node:child_process";

const repoRoot = execSync("git rev-parse --show-toplevel", { encoding: "utf8" }).trim();
const activeFile = path.join(repoRoot, ".migration", "ACTIVE");
if (!fs.existsSync(activeFile)) {
  console.error("[EXPIRED] .migration/ACTIVE 不存在：迁移已结束，门禁 0 已失效。");
  process.exit(2);
}

const failures = [];
const run = (cmd) => execSync(cmd, { cwd: repoRoot, encoding: "utf8" }).trim();

const statusLines = run("git status --porcelain").split("\n").filter(Boolean);
const trackedMigrationSecrets = run("git ls-files .migration/.env.local").split("\n").filter(Boolean);
if (trackedMigrationSecrets.length > 0) {
  failures.push("严禁将 .migration/.env.local 纳入 Git 追踪！");
}

const forbiddenStagedPaths = [
  /^\.migration\/\.env\.local$/,
  /^\.migration\/artifacts\//,
  /\.DS_Store$/,
  /^VERSION$/,
  /^\.release-please-manifest\.json$/,
  /^CHANGELOG\.md$/,
];

for (const line of statusLines) {
  const file = line.slice(3).trim().replace(/^"|"$/g, "");
  for (const re of forbiddenStagedPaths) {
    if (re.test(file)) {
      failures.push(`检测到禁止修改或入库的文件变动：${file}`);
    }
  }
}

// 检查已暂存的文本文件是否误含真实 TOTP Secret 或密码赋值
const staged = run("git diff --cached --name-only").split("\n").filter(Boolean);
const secretPatterns = [
  /TEST_[A-Z]+_TOTP_SECRET\s*=\s*[A-Z2-7]{16,}/,
  /TEST_[A-Z]+_PASSWORD\s*=\s*\S{6,}/,
  /-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----\r?\n[A-Za-z0-9+/=\s]{32,}/,
];

for (const rel of staged) {
  if (rel.endsWith(".example") || rel.includes("reference/")) continue;
  const abs = path.join(repoRoot, rel);
  if (!fs.existsSync(abs) || fs.statSync(abs).isDirectory()) continue;
  const text = fs.readFileSync(abs, "utf8");
  for (const re of secretPatterns) {
    if (re.test(text)) {
      failures.push(`暂存文件 ${rel} 命中敏感密钥模式 ${re}`);
    }
  }
}

// 检查 gitignore 是否覆盖 .migration/.env.local 与 .migration/artifacts/
const gitignore = fs.existsSync(path.join(repoRoot, ".gitignore"))
  ? fs.readFileSync(path.join(repoRoot, ".gitignore"), "utf8")
  : "";
if (!gitignore.includes(".migration/.env.local")) {
  failures.push(".gitignore 缺少 .migration/.env.local 保护条目");
}

if (failures.length > 0) {
  console.error("❌ [GATE-0 FAILED]");
  for (const f of failures) console.error("  - " + f);
  process.exit(1);
}

console.log("✅ [GATE-0 PASSED] 环境卫生、敏感凭据防泄漏与发版元数据保护校验通过。");
