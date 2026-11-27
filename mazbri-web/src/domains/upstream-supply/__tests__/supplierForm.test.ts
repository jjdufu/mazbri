import assert from "node:assert/strict";
import { describe, it } from "node:test";
import type { Supplier } from "../../../api/supplier";
import {
  createEmptyInvalidation,
  invalidationFromApi,
  invalidationToApi,
  validateInvalidation,
} from "../authInvalidation";
import {
  buildSupplierUpdatePayload,
  resolveUpstreamType,
  supplierToForm,
} from "../supplier-form";

describe("supplier-form and authInvalidation helpers", () => {
  it("resolves upstream type and round-trips microservice supplier", () => {
    const supplier: Supplier = {
      id: "sup-1",
      supplier_code: "PARTNER-A",
      name: "Partner A",
      upstream_url: "http://127.0.0.1:8080",
      status: "active",
      config: { upstream_type: "microservice", contract_id: "c-001" },
      created_at: "2026-01-01T00:00:00Z",
      updated_at: "2026-01-01T00:00:00Z",
    };

    assert.equal(resolveUpstreamType(supplier), "microservice");
    const form = supplierToForm(supplier);
    assert.equal(form.upstream_url, "127.0.0.1:8080");
    assert.equal(form.config.contract_id, "c-001");

    const payload = buildSupplierUpdatePayload(form);
    assert.equal(payload.supplier_code, "PARTNER-A");
    assert.equal(payload.config.contract_id, "c-001");
    assert.equal(payload.config.auth_flow, null);
  });

  it("preserves vault-only credentials when HTTP auth flow is disabled", () => {
    const supplier: Supplier = {
      id: "sup-2",
      supplier_code: "PARTNER-B",
      name: "Partner B",
      upstream_url: "https://api.example.com",
      status: "active",
      config: {
        upstream_type: "transparent",
        auth_flow: {
          enabled: false,
          profile: "none",
          credentials: { api_key: "••••••••" },
        },
      },
      created_at: "2026-01-01T00:00:00Z",
      updated_at: "2026-01-01T00:00:00Z",
    };

    assert.equal(resolveUpstreamType(supplier), "transparent");
    const form = supplierToForm(supplier);
    assert.equal(form.config.auth_flow.enabled, false);
    assert.deepEqual(form.config.auth_flow.credentials, [
      { key: "api_key", value: "••••••••" },
    ]);

    const payload = buildSupplierUpdatePayload(form);
    assert.deepEqual(payload.config.auth_flow, {
      enabled: false,
      profile: "none",
      credentials: { api_key: "••••••••" },
    });
  });

  it("validates and serializes auth invalidation rules", () => {
    const empty = createEmptyInvalidation();
    assert.equal(invalidationToApi(empty), undefined);
    assert.equal(validateInvalidation(empty), null);

    const parsed = invalidationFromApi({
      http_status: [401, 403],
      body_match: [{ path: "$.code", equals: ["EXPIRED", 40101] }],
    });
    assert.equal(parsed.enabled, true);
    assert.equal(validateInvalidation(parsed), null);
    assert.deepEqual(invalidationToApi(parsed), {
      http_status: [401, 403],
      body_match: [{ path: "$.code", equals: ["EXPIRED", 40101] }],
    });

    const bad = { ...parsed, http_status: [200] };
    assert.deepEqual(validateInvalidation(bad), {
      kind: "status_range",
      value: 200,
    });
  });
});
