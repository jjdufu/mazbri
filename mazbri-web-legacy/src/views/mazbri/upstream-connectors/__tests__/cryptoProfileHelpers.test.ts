import assert from "node:assert/strict";
import { test } from "node:test";
import {
  applyCryptoTemplate,
  applyOrderDefaults,
  defaultCryptoProfile,
  isAuthSignFormProfile,
  kindToWireEncoding,
  parseCryptoProfile,
  roundTripCryptoProfile,
  serializeCryptoProfile,
  validateCryptoProfileClient,
  wireEncodingToKind,
} from "../cryptoProfileHelpers";

test("defaultCryptoProfile defaults verify_response and decrypt_response to false", () => {
  const aead = defaultCryptoProfile("aead_only");
  assert.equal(aead.order, "aead_only");
  assert.equal(aead.crypt?.alg, "aes-256-gcm");
  assert.equal(aead.verify_response, false);
  assert.equal(aead.decrypt_response, false);
  assert.equal(validateCryptoProfileClient(aead).length, 0);

  const serialized = serializeCryptoProfile(aead);
  assert.equal(serialized?.verify_response, false);
  assert.equal(serialized?.decrypt_response, false);

  const rt = roundTripCryptoProfile(aead);
  assert.ok(rt);
  assert.equal(rt?.order, "aead_only");
  assert.equal(rt?.crypt?.alg, "aes-256-gcm");
  assert.equal(rt?.verify_response, false);
  assert.equal(rt?.decrypt_response, false);
});

test("parseCryptoProfile defaults omitted direction flags to false and preserves true", () => {
  const fromBackendOmitted = parseCryptoProfile({
    version: 1,
    order: "sign_then_encrypt",
    sign: { alg: "hmac-sha256" },
    crypt: { alg: "aes-256-cbc" },
    canon: "sorted_kv",
    encodings: { payload: "base64", signature: "base64", iv: "base64" },
    iv: "random",
    scope: "whole_body",
    allow_insecure: false,
  });
  assert.ok(fromBackendOmitted);
  assert.equal(fromBackendOmitted?.verify_response, false);
  assert.equal(fromBackendOmitted?.decrypt_response, false);

  const bidirectional = defaultCryptoProfile("sign_then_encrypt");
  bidirectional.verify_response = true;
  bidirectional.decrypt_response = true;
  const serialized = serializeCryptoProfile(bidirectional);
  assert.equal(serialized?.verify_response, true);
  assert.equal(serialized?.decrypt_response, true);

  const rt = roundTripCryptoProfile(bidirectional);
  assert.equal(rt?.verify_response, true);
  assert.equal(rt?.decrypt_response, true);
});

test("applyOrderDefaults and validateCryptoProfileClient enforce direction invariants", () => {
  const p = defaultCryptoProfile("sign_then_encrypt");
  p.verify_response = true;
  p.decrypt_response = true;

  p.order = "aead_only";
  applyOrderDefaults(p);
  assert.equal(p.sign, null);
  assert.equal(p.verify_response, false);
  assert.equal(p.decrypt_response, true);

  const badSign = defaultCryptoProfile("aead_only");
  badSign.verify_response = true;
  assert.ok(
    validateCryptoProfileClient(badSign).some(
      (i) => i.code === "verify_response_sign",
    ),
  );

  const badDecrypt = defaultCryptoProfile("sign_only");
  badDecrypt.decrypt_response = true;
  assert.ok(
    validateCryptoProfileClient(badDecrypt).some(
      (i) => i.code === "decrypt_response_crypt",
    ),
  );
  badDecrypt.response_crypt = { alg: "aes-256-gcm", fields: [] };
  assert.equal(validateCryptoProfileClient(badDecrypt).length, 0);
});

test("templates, encodings, and legacy gate validation", () => {
  assert.equal(serializeCryptoProfile(null), null);
  assert.equal(parseCryptoProfile(null), null);
  assert.equal(parseCryptoProfile({ order: "nope" }), null);

  const gm = applyCryptoTemplate("gm_sm4_cbc_hmac_sm3");
  assert.equal(gm.order, "encrypt_then_sign");
  assert.equal(gm.sign?.alg, "hmac-sm3");
  assert.equal(gm.crypt?.alg, "sm4-cbc-pkcs7");
  assert.equal(gm.verify_response, false);
  assert.equal(gm.decrypt_response, false);
  assert.ok(!JSON.stringify(gm).toLowerCase().includes("upupt"));

  const legacy = defaultCryptoProfile("sign_only");
  legacy.sign = { alg: "hmac-md5" };
  legacy.allow_insecure = false;
  const issues = validateCryptoProfileClient(legacy);
  assert.ok(issues.some((i) => i.code === "legacy_gate"));
  legacy.allow_insecure = true;
  assert.equal(validateCryptoProfileClient(legacy).length, 0);

  assert.equal(wireEncodingToKind({ hex: { case: "upper" } }), "hex_upper");
  assert.deepEqual(kindToWireEncoding("hex_lower"), { hex: { case: "lower" } });

  assert.equal(isAuthSignFormProfile("sign_form"), true);
  assert.equal(isAuthSignFormProfile("basic"), false);

  const signOnlyMissing = defaultCryptoProfile("sign_only");
  signOnlyMissing.sign = null;
  assert.ok(
    validateCryptoProfileClient(signOnlyMissing).some(
      (i) => i.code === "sign_required",
    ),
  );
});
