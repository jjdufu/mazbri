import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  CERT_EXPIRY_WARN_DAYS,
  CERT_LIST_PATH,
  shouldShowCertExpiryBadge,
} from "../cert-badge";

describe("cert-badge", () => {
  it("shows below warn threshold including expired", () => {
    assert.equal(shouldShowCertExpiryBadge(29), true);
    assert.equal(shouldShowCertExpiryBadge(0), true);
    assert.equal(shouldShowCertExpiryBadge(-1), true);
    assert.equal(shouldShowCertExpiryBadge(CERT_EXPIRY_WARN_DAYS), false);
    assert.equal(shouldShowCertExpiryBadge(31), false);
  });

  it("rejects non-finite", () => {
    assert.equal(shouldShowCertExpiryBadge(null), false);
    assert.equal(shouldShowCertExpiryBadge(undefined), false);
    assert.equal(shouldShowCertExpiryBadge(Number.NaN), false);
    assert.equal(shouldShowCertExpiryBadge(Number.POSITIVE_INFINITY), false);
  });

  it("cert list path is control-panel", () => {
    assert.equal(CERT_LIST_PATH, "/control-panel/certificates");
  });
});
