import assert from "node:assert/strict";
import {
  applyCryptoTemplate,
  defaultCryptoProfile,
  isAuthSignFormProfile,
  parseCryptoProfile,
  roundTripCryptoProfile,
  serializeCryptoProfile,
  validateCryptoProfileClient,
  wireEncodingToKind,
  kindToWireEncoding,
} from "../cryptoProfileHelpers";

const aead = defaultCryptoProfile("aead_only");
assert.equal(aead.order, "aead_only");
assert.equal(aead.crypt?.alg, "aes-256-gcm");
assert.equal(validateCryptoProfileClient(aead).length, 0);

const rt = roundTripCryptoProfile(aead);
assert.ok(rt);
assert.equal(rt?.order, "aead_only");
assert.equal(rt?.crypt?.alg, "aes-256-gcm");

const disabled = serializeCryptoProfile(null);
assert.equal(disabled, null);
assert.equal(parseCryptoProfile(null), null);
assert.equal(parseCryptoProfile({ order: "nope" }), null);

const gm = applyCryptoTemplate("gm_sm4_cbc_hmac_sm3");
assert.equal(gm.order, "encrypt_then_sign");
assert.equal(gm.sign?.alg, "hmac-sm3");
assert.equal(gm.crypt?.alg, "sm4-cbc-pkcs7");
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
  validateCryptoProfileClient(signOnlyMissing).some((i) => i.code === "sign_required"),
);

console.log("cryptoProfileHelpers ok");
