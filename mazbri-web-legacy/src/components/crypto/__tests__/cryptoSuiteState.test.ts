import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  DEFAULT_CRYPTO_SUITE_ID,
  applySelectionModeChange,
  detectLegacyAlgorithms,
  resolveInitialSelectionMode,
  type CryptoSuiteSelectValue,
} from "../cryptoSuiteState";

describe("cryptoSuiteState", () => {
  it("initializes to suite mode when suite is present or empty without custom alg", () => {
    assert.equal(
      resolveInitialSelectionMode({
        suite: "suite/tenant-default-2026-11",
        dataAlg: null,
        macAlg: null,
      }),
      "suite",
    );
    assert.equal(
      resolveInitialSelectionMode({ suite: null, dataAlg: null, macAlg: null }),
      "suite",
    );
  });

  it("initializes to custom mode for legacy des-cbc-pkcs5 without suite", () => {
    assert.equal(
      resolveInitialSelectionMode({
        suite: null,
        dataAlg: "des-cbc-pkcs5",
        macAlg: "hmac-sha256",
      }),
      "custom",
    );
  });

  it("switches to custom without bouncing back and restores suite cleanly", () => {
    const start: CryptoSuiteSelectValue = {
      suite: "suite/tenant-default-2026-11",
      dataAlg: null,
      macAlg: null,
      allowInsecure: false,
    };
    const customState = applySelectionModeChange(start, "custom");
    assert.equal(customState.suite, null);
    assert.equal(customState.dataAlg, null);

    const backToSuite = applySelectionModeChange(
      { ...customState, dataAlg: "aes-256-cbc-pkcs7", macAlg: "hmac-sha256" },
      "suite",
    );
    assert.equal(backToSuite.suite, DEFAULT_CRYPTO_SUITE_ID);
    assert.equal(backToSuite.dataAlg, null);
    assert.equal(backToSuite.macAlg, null);
  });

  it("flags all 5 legacy algorithms (4 block ciphers + hmac-md5)", () => {
    for (const alg of [
      "aes-256-ecb",
      "aes-128-ecb",
      "sm4-ecb-pkcs7",
      "des-cbc-pkcs5",
    ]) {
      const detected = detectLegacyAlgorithms({ dataAlg: alg, macAlg: null });
      assert.equal(detected.length, 1, `expected ${alg} to be flagged legacy`);
      assert.equal(detected[0].id, alg);
    }

    const macLegacy = detectLegacyAlgorithms({
      dataAlg: "aes-256-cbc-pkcs7",
      macAlg: "hmac-md5",
    });
    assert.equal(macLegacy.length, 1);
    assert.equal(macLegacy[0].id, "hmac-md5");

    const bothLegacy = detectLegacyAlgorithms({
      dataAlg: "des-cbc-pkcs5",
      macAlg: "hmac-md5",
    });
    assert.equal(bothLegacy.length, 2);

    const safe = detectLegacyAlgorithms({
      dataAlg: "aes-256-gcm",
      macAlg: "hmac-sha256",
    });
    assert.equal(safe.length, 0);
  });
});
