import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, it } from "node:test";
import { ACTION_LABELS } from "../../audit-logs/labels";
import {
  FALLBACK_CORE_CONFIG_NAMES,
  isCoreProtectedConfigName,
  parseGatewayCors,
  serializeGatewayCors,
} from "../helpers";

describe("config-center Step 15 helpers and contracts", () => {
  it("protects global and all canonical core services while allowing custom extensions", () => {
    for (const coreName of FALLBACK_CORE_CONFIG_NAMES) {
      assert.equal(
        isCoreProtectedConfigName(coreName),
        true,
        `${coreName} must be protected`,
      );
    }
    assert.equal(isCoreProtectedConfigName("  mazbri-gateway "), true);
    assert.equal(isCoreProtectedConfigName("custom-ext-service"), false);
  });

  it("parses and serializes gateway [cors] section without losing surrounding TOML", () => {
    const original = `[server]\nport = 8080\n\n[cors]\nallowed_origins = ["https://console.example.com"]\nallow_credentials = false\n\n[logging]\nlevel = "info"\n`;
    const parsed = parseGatewayCors(original);
    assert.deepEqual(parsed, {
      allowed_origins: ["https://console.example.com"],
      allow_credentials: false,
    });

    const updated = serializeGatewayCors(
      original,
      ["https://console.example.com", "https://api.example.com"],
      true,
    );
    assert.match(updated, /\[server\]\nport = 8080/);
    assert.match(updated, /\[logging\]\nlevel = "info"/);
    assert.deepEqual(parseGatewayCors(updated), {
      allowed_origins: [
        "https://console.example.com",
        "https://api.example.com",
      ],
      allow_credentials: true,
    });
  });

  it("registers sensitive config view actions in audit ACTION_LABELS", () => {
    assert.equal(
      ACTION_LABELS["config_center.view"],
      "auditLogs.explicitActions.configCenterView",
    );
    assert.equal(
      ACTION_LABELS["config_center.view_history"],
      "auditLogs.explicitActions.configCenterViewHistory",
    );
  });

  it("keeps zh.json and en.json configCenter keys isomorphic and uses neutral console.example.com", () => {
    const zhRaw = readFileSync(
      resolve(import.meta.dirname, "../../../../../locales/langs/zh.json"),
      "utf8",
    );
    const enRaw = readFileSync(
      resolve(import.meta.dirname, "../../../../../locales/langs/en.json"),
      "utf8",
    );
    assert.match(zhRaw, /https:\/\/console\.example\.com/);
    assert.match(enRaw, /https:\/\/console\.example\.com/);

    const collectKeys = (obj: Record<string, unknown>, prefix = ""): string[] =>
      Object.entries(obj)
        .sort(([a], [b]) => a.localeCompare(b))
        .flatMap(([k, v]) => {
          const next = prefix ? `${prefix}.${k}` : k;
          return v && typeof v === "object" && !Array.isArray(v)
            ? collectKeys(v as Record<string, unknown>, next)
            : [next];
        });

    const zh = JSON.parse(zhRaw) as { configCenter: Record<string, unknown> };
    const en = JSON.parse(enRaw) as { configCenter: Record<string, unknown> };
    assert.deepEqual(collectKeys(zh.configCenter), collectKeys(en.configCenter));
  });
});
