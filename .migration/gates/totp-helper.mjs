#!/usr/bin/env node
/**
 * RFC 6238 TOTP 动态码生成器（仅限迁移自测阶段读取本地未入库 .migration/.env.local）
 * 用法：
 *   node .migration/gates/totp-helper.mjs --role admin
 *   node .migration/gates/totp-helper.mjs --role admin --json
 */
import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { execSync } from "node:child_process";

const repoRoot = execSync("git rev-parse --show-toplevel", { encoding: "utf8" }).trim();
const activeFile = path.join(repoRoot, ".migration", "ACTIVE");
if (!fs.existsSync(activeFile)) {
  console.error("[EXPIRED] .migration/ACTIVE 不存在：迁移已结束，本工具已失效。");
  process.exit(2);
}

function loadEnvLocal() {
  const envPath = path.join(repoRoot, ".migration", ".env.local");
  const out = {};
  if (!fs.existsSync(envPath)) return out;
  for (const raw of fs.readFileSync(envPath, "utf8").split(/\r?\n/)) {
    const line = raw.trim();
    if (!line || line.startsWith("#")) continue;
    const eq = line.indexOf("=");
    if (eq === -1) continue;
    const k = line.slice(0, eq).trim();
    const v = line.slice(eq + 1).trim().replace(/^["']|["']$/g, "");
    out[k] = v;
  }
  return out;
}

function base32Decode(input) {
  const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
  const cleaned = input.toUpperCase().replace(/[^A-Z2-7]/g, "");
  let bits = 0;
  let value = 0;
  const bytes = [];
  for (const ch of cleaned) {
    const idx = alphabet.indexOf(ch);
    if (idx === -1) continue;
    value = (value << 5) | idx;
    bits += 5;
    if (bits >= 8) {
      bytes.push((value >>> (bits - 8)) & 0xff);
      bits -= 8;
    }
  }
  return Buffer.from(bytes);
}

function generateTotp(secretBase32, digits = 6, stepSec = 30, nowMs = Date.now()) {
  const key = base32Decode(secretBase32);
  const counter = Math.floor(nowMs / 1000 / stepSec);
  const buf = Buffer.alloc(8);
  buf.writeBigUInt64BE(BigInt(counter));
  const hmac = crypto.createHmac("sha1", key).update(buf).digest();
  const offset = hmac[hmac.length - 1] & 0x0f;
  const binary =
    ((hmac[offset] & 0x7f) << 24) |
    ((hmac[offset + 1] & 0xff) << 16) |
    ((hmac[offset + 2] & 0xff) << 8) |
    (hmac[offset + 3] & 0xff);
  const code = String(binary % 10 ** digits).padStart(digits, "0");
  const remainingSec = stepSec - (Math.floor(nowMs / 1000) % stepSec);
  return { code, remainingSec };
}

const args = process.argv.slice(2);
const roleIdx = args.indexOf("--role");
const role = (roleIdx !== -1 ? args[roleIdx + 1] : "admin").toUpperCase();
const asJson = args.includes("--json");
const envFile = loadEnvLocal();
const secret =
  process.env.TOTP_SECRET ||
  process.env[`TEST_${role}_TOTP_SECRET`] ||
  envFile[`TEST_${role}_TOTP_SECRET`];

if (!secret) {
  console.error(
    `[ERR] 未找到 TEST_${role}_TOTP_SECRET。请在 .migration/.env.local（不入库）或环境变量中配置。`,
  );
  process.exit(1);
}

let res = generateTotp(secret);
// 若距离当前 30s 窗口到期不足 3 秒，等待进入下一个新窗口，避免填入瞬间过期
if (res.remainingSec <= 3) {
  await new Promise((r) => setTimeout(r, (res.remainingSec + 0.5) * 1000));
  res = generateTotp(secret);
}

if (asJson) {
  console.log(JSON.stringify({ role, code: res.code, remainingSec: res.remainingSec }));
} else {
  console.log(res.code);
}
