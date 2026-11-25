import assert from "node:assert/strict";
import { test } from "node:test";
import {
  canonicalOrigin,
  clearMaskedKvEntries,
  hasOriginChanged,
  isMaskedSecretValue,
} from "../originPinning";

test("canonicalOrigin normalizes scheme, host case, default ports, and ignores paths", () => {
  assert.equal(
    canonicalOrigin("HTTPS://API.Example.COM/v1/orders?foo=1"),
    "https://api.example.com:443",
  );
  assert.equal(
    canonicalOrigin("https://api.example.com:443/v2/users"),
    "https://api.example.com:443",
  );
  assert.equal(
    canonicalOrigin("http://192.168.50.50:80/test"),
    "http://192.168.50.50:80",
  );
  assert.equal(
    canonicalOrigin("127.0.0.1:8088", { defaultScheme: "http" }),
    "http://127.0.0.1:8088",
  );
  assert.equal(
    canonicalOrigin("http://[::1]:9000/health"),
    "http://[::1]:9000",
  );
  assert.equal(canonicalOrigin("   "), "");
});

test("hasOriginChanged detects scheme, host, or port mutation while ignoring path edits", () => {
  assert.equal(
    hasOriginChanged(
      "https://api.vendor.com/v1",
      "https://API.vendor.com:443/v2/new-path",
    ),
    false,
  );
  assert.equal(
    hasOriginChanged("https://api.vendor.com/v1", "http://api.vendor.com/v1"),
    true,
  );
  assert.equal(
    hasOriginChanged("https://api.vendor.com/v1", "https://evil.example.com/v1"),
    true,
  );
  assert.equal(
    hasOriginChanged(
      "https://api.vendor.com:8443/v1",
      "https://api.vendor.com:9443/v1",
    ),
    true,
  );
  assert.equal(
    hasOriginChanged("", "https://api.vendor.com/oauth/token", {
      fallbackInitialOrigin: "https://api.vendor.com:443",
    }),
    false,
  );
  assert.equal(
    hasOriginChanged("", "https://attacker.example.com/oauth/token", {
      fallbackInitialOrigin: "https://api.vendor.com:443",
    }),
    true,
  );
});

test("clearMaskedKvEntries clears only masked values and returns affected keys", () => {
  assert.equal(isMaskedSecretValue("••••••••"), true);
  assert.equal(isMaskedSecretValue("********"), true);
  assert.equal(isMaskedSecretValue("real-secret-123"), false);

  const creds = [
    { key: "client_id", value: "app-001" },
    { key: "client_secret", value: "••••••••" },
    { key: "api_key", value: "********" },
  ];
  const cleared = clearMaskedKvEntries(creds);
  assert.deepEqual(cleared, ["client_secret", "api_key"]);
  assert.equal(creds[0]?.value, "app-001");
  assert.equal(creds[1]?.value, "");
  assert.equal(creds[2]?.value, "");
});
