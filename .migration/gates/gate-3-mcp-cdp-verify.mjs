#!/usr/bin/env node
/**
 * 门禁 3：基于本机 Chrome CDP (127.0.0.1:9224) 对目标站点 (192.168.50.50:8085) 的全自动 UI/UX 验收门禁
 * 严禁模拟滑动或绕过人机验证滑块（迁移期通过 VITE_LOGIN_SLIDER_ENABLED=false 合法关闭纯前端装饰滑块）。
 *
 * 用法：
 *   node .migration/gates/gate-3-mcp-cdp-verify.mjs --stage 3
 *   node .migration/gates/gate-3-mcp-cdp-verify.mjs --full
 */
import fs from "node:fs";
import path from "node:path";
import { execSync } from "node:child_process";

const repoRoot = execSync("git rev-parse --show-toplevel", { encoding: "utf8" }).trim();
const activeFile = path.join(repoRoot, ".migration", "ACTIVE");
if (!fs.existsSync(activeFile)) {
  console.error("[EXPIRED] .migration/ACTIVE 不存在：迁移已结束，门禁 3 已失效。");
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
    out[line.slice(0, eq).trim()] = line.slice(eq + 1).trim().replace(/^["']|["']$/g, "");
  }
  return out;
}

const envLocal = loadEnvLocal();
const CDP_HOST = process.env.CDP_HOST || envLocal.CDP_HOST || "127.0.0.1";
const CDP_PORT = process.env.CDP_PORT || envLocal.CDP_PORT || "9224";
const TARGET_URL = (process.env.TARGET_URL || envLocal.TARGET_URL || "http://192.168.50.50:8085").replace(/\/+$/, "");
const ART_DIR = path.join(repoRoot, ".migration", "artifacts");
fs.mkdirSync(ART_DIR, { recursive: true });

const STAGE_ROUTES = {
  "2": ["/"],
  "3": [
    "/control-panel/users",
    "/control-panel/certificates",
    "/control-panel/system",
    "/control-panel/audit-logs",
    "/control-panel/config-center",
    "/ops/traffic",
    "/ops/alerts",
    "/ops/runtime",
  ],
  "4": [
    "/operations/tenants",
    "/operations/product-authorization",
    "/operations/upstream-supply",
    "/operations/callbacks",
    "/operations/bills",
  ],
  "5": ["/operations/upstream-supply/workbench"],
};
STAGE_ROUTES.full = [...STAGE_ROUTES["3"], ...STAGE_ROUTES["4"], ...STAGE_ROUTES["5"]];

const args = process.argv.slice(2);
const stageIdx = args.indexOf("--stage");
const mode = args.includes("--full") ? "full" : stageIdx !== -1 ? args[stageIdx + 1] : "full";
const routes = STAGE_ROUTES[mode] || STAGE_ROUTES.full;

const targetsResp = await fetch(`http://${CDP_HOST}:${CDP_PORT}/json/list`);
const targets = await targetsResp.json();
const targetHost = new URL(TARGET_URL).host;
const pageTarget =
  targets.find((p) => p.type === "page" && p.url.includes(targetHost) && !p.url.includes("/auth/login")) ||
  targets.find((p) => p.type === "page" && p.url.includes(targetHost));

if (!pageTarget) {
  console.error(`❌ 未在 ${CDP_HOST}:${CDP_PORT} 中找到已打开的 ${TARGET_URL} 页面标签。`);
  process.exit(1);
}

const ws = new WebSocket(pageTarget.webSocketDebuggerUrl);
await new Promise((ok, err) => {
  ws.onopen = ok;
  ws.onerror = err;
});

let seq = 0;
const pending = new Map();
const runtimeErrors = [];
const failedRequests = [];

ws.onmessage = (e) => {
  const m = JSON.parse(e.data);
  if (m.id && pending.has(m.id)) {
    const { ok, err } = pending.get(m.id);
    pending.delete(m.id);
    if (m.error) err(new Error(m.error.message));
    else ok(m.result);
    return;
  }
  if (m.method === "Runtime.exceptionThrown") {
    runtimeErrors.push(m.params?.exceptionDetails?.text || "Uncaught exception");
  }
  if (m.method === "Console.messageAdded" && m.params?.message?.level === "error") {
    runtimeErrors.push(m.params.message.text || "Console error");
  }
  if (m.method === "Network.responseReceived") {
    const resp = m.params?.response;
    if (resp && resp.url.includes("/api/") && resp.status >= 400 && resp.status !== 428) {
      failedRequests.push(`${resp.status} ${resp.url}`);
    }
  }
};

const send = (method, params = {}, timeoutMs = 10000) =>
  new Promise((ok, err) => {
    const id = ++seq;
    const t = setTimeout(() => {
      pending.delete(id);
      err(new Error(`CDP timeout: ${method}`));
    }, timeoutMs);
    pending.set(id, {
      ok: (v) => {
        clearTimeout(t);
        ok(v);
      },
      err: (e) => {
        clearTimeout(t);
        err(e);
      },
    });
    ws.send(JSON.stringify({ id, method, params }));
  });

const ev = async (expr) => {
  const r = await send("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true });
  if (r.exceptionDetails) throw new Error(r.exceptionDetails.text);
  return r.result?.value;
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

await send("Page.bringToFront");
await send("Runtime.enable");
await send("Console.enable");
await send("Network.enable");

const startUrl = await ev("location.href");
const failures = [];

async function saveFailShot(name) {
  try {
    const { data } = await send("Page.captureScreenshot", { format: "png" });
    const file = path.join(ART_DIR, `fail_${name.replace(/[^a-zA-Z0-9_-]/g, "_")}.png`);
    fs.writeFileSync(file, Buffer.from(data, "base64"));
    return file;
  } catch {
    return null;
  }
}

async function auditCurrentView(label) {
  const data = await ev(`(() => {
    const isVis = (el) => {
      const r = el.getBoundingClientRect();
      if (r.width === 0 || r.height === 0) return false;
      const s = getComputedStyle(el);
      return s.display !== 'none' && s.visibility !== 'hidden' && parseFloat(s.opacity) > 0.05;
    };
    const overflow = document.documentElement.scrollWidth > document.documentElement.clientWidth + 2;

    // 1. 裸露 i18n key 检测（如 common.actions.edit / ops.components.statusName.healthy）
    const rawKeys = [];
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    let node;
    while ((node = walker.nextNode())) {
      const txt = (node.nodeValue || '').trim();
      if (!txt || txt.length > 80) continue;
      const el = node.parentElement;
      if (!el || !isVis(el) || el.closest('code, pre')) continue;
      if (/^[a-z][a-zA-Z0-9_-]*(\\.[a-zA-Z0-9_-]+){2,}$/.test(txt) && !/\\.(com|cn|io|net|org|pem|jws|json|yaml|yml|toml|csv)$/.test(txt)) {
        rawKeys.push(txt);
      }
    }

    // 2. 弹窗/抽屉三段式结构与表单标签零重叠校验
    const overlayIssues = [];
    const overlays = Array.from(document.querySelectorAll('[role="dialog"], [data-slot="modal"], [data-slot="drawer"]')).filter(isVis);
    for (const ov of overlays) {
      const r = ov.getBoundingClientRect();
      const labels = Array.from(ov.querySelectorAll('label')).filter(isVis).map(l => ({
        text: l.innerText.trim().slice(0, 30),
        r: l.getBoundingClientRect()
      }));
      for (let i = 0; i < labels.length; i++) {
        for (let j = i + 1; j < labels.length; j++) {
          const a = labels[i].r, b = labels[j].r;
          const overlapX = Math.min(a.right, b.right) - Math.max(a.left, b.left);
          const overlapY = Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top);
          if (overlapX > 2 && overlapY > 2) {
            overlayIssues.push(\`标签重叠: "\${labels[i].text}" vs "\${labels[j].text}"\`);
          }
        }
      }
      if (r.width > 900 && !ov.dataset.allowWide) {
        overlayIssues.push(\`浮层超宽: \${Math.round(r.width)}px\`);
      }
    }

    // 3. 暗色模式下是否存在刺眼纯白 Tooltip
    const isDark = document.documentElement.classList.contains('dark');
    let whiteTooltipInDark = false;
    if (isDark) {
      const tips = Array.from(document.querySelectorAll('[role="tooltip"], [data-slot="tooltip"]')).filter(isVis);
      whiteTooltipInDark = tips.some(t => getComputedStyle(t).backgroundColor === 'rgb(255, 255, 255)');
    }

    // 4. 按钮是否存在小数亚像素圆角
    const badRadii = [];
    for (const b of Array.from(document.querySelectorAll('button')).filter(isVis).slice(0, 80)) {
      const br = getComputedStyle(b).borderRadius;
      if (/\\d+\\.\\d+px/.test(br)) badRadii.push(br);
    }

    return {
      url: location.href,
      isDark,
      overflow,
      rawKeys: [...new Set(rawKeys)],
      overlayIssues,
      whiteTooltipInDark,
      badRadii: [...new Set(badRadii)]
    };
  })()`);

  if (data.overflow) failures.push(`[${label}] 存在水平滚动溢出 (${data.url})`);
  if (data.rawKeys.length > 0) failures.push(`[${label}] 裸露未翻译 i18n Key: ${data.rawKeys.join(", ")}`);
  if (data.overlayIssues.length > 0) failures.push(`[${label}] 浮层结构/重叠缺陷: ${data.overlayIssues.join("; ")}`);
  if (data.whiteTooltipInDark) failures.push(`[${label}] 暗色模式下弹出纯白 Tooltip`);
  if (data.badRadii.length > 0) failures.push(`[${label}] 检测到亚像素小数圆角: ${data.badRadii.join(", ")}`);

  if (data.overflow || data.rawKeys.length || data.overlayIssues.length || data.whiteTooltipInDark || data.badRadii.length) {
    await saveFailShot(label);
  }
}

async function checkRealHover(modeLabel) {
  const targets = await ev(`(() => {
    const out = [];
    const btns = Array.from(document.querySelectorAll('main button')).filter(b => {
      const r = b.getBoundingClientRect();
      return r.width > 24 && r.height >= 24 && !b.disabled;
    });
    for (const b of btns.slice(0, 4)) {
      const r = b.getBoundingClientRect();
      out.push({ text: b.innerText.trim().slice(0, 20) || 'icon', x: Math.round(r.left + r.width / 2), y: Math.round(r.top + r.height / 2) });
    }
    return out;
  })()`);
  let changedAny = 0;
  for (const t of targets) {
    await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: 4, y: 4 });
    await sleep(120);
    const before = await ev(`(() => { const el = document.elementFromPoint(${t.x}, ${t.y})?.closest('button'); if (!el) return null; const s = getComputedStyle(el); return s.backgroundColor + '|' + s.borderColor + '|' + s.color; })()`);
    await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: t.x, y: t.y });
    await sleep(220);
    const after = await ev(`(() => { const el = document.elementFromPoint(${t.x}, ${t.y})?.closest('button'); if (!el) return null; const s = getComputedStyle(el); return s.backgroundColor + '|' + s.borderColor + '|' + s.color; })()`);
    if (before && after && before !== after) changedAny++;
  }
  await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: 4, y: 4 });
  if (targets.length > 0 && changedAny === 0) {
    failures.push(`[${modeLabel}] 实测 ${targets.length} 个主区域按钮 Hover 无任何视觉样式变化`);
  }
}

const viewports = [
  { name: "1440x900", width: 1440, height: 900 },
  { name: "1280x800", width: 1280, height: 800 },
];

for (const vp of viewports) {
  await send("Emulation.setDeviceMetricsOverride", {
    width: vp.width,
    height: vp.height,
    deviceScaleFactor: 2,
    mobile: false,
  });

  for (const theme of ["light", "dark"]) {
    await ev(`document.documentElement.classList.toggle('dark', ${theme === "dark"})`);
    await sleep(200);

    for (const r of routes) {
      const fullUrl = `${TARGET_URL}${r}`;
      await send("Page.navigate", { url: fullUrl });
      await sleep(1100);
      await auditCurrentView(`${vp.name}_${theme}_${r}`);
      if (vp.name === "1440x900" && r === routes[0]) {
        await checkRealHover(`${vp.name}_${theme}_${r}`);
      }
    }
  }
}

// 恢复视口、亮色主题与原地址
await send("Emulation.clearDeviceMetricsOverride");
await ev("document.documentElement.classList.remove('dark')");
await send("Page.navigate", { url: startUrl });
await sleep(400);
ws.close();

if (runtimeErrors.length > 0) failures.push(`运行时 Console/Exception 报错 (${runtimeErrors.length} 条): ${runtimeErrors.slice(0, 5).join(" | ")}`);
if (failedRequests.length > 0) failures.push(`API 异常响应 (${failedRequests.length} 条): ${failedRequests.slice(0, 5).join(" | ")}`);

if (failures.length > 0) {
  console.error(`❌ [GATE-3 FAILED] 实机巡检发现 ${failures.length} 项缺陷（截图见 .migration/artifacts/）：`);
  for (const f of failures) console.error("  - " + f);
  process.exit(1);
}

console.log(`✅ [GATE-3 PASSED] 已完成 ${routes.length} 条路由 × 双分辨率 × 亮/暗双模式实机核验，零报错、零重叠、零裸露 Key。`);
