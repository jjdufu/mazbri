import assert from "node:assert/strict";
import {
  AUTH_PICKER_IDS,
  applyPickerDefaults,
  authFieldGroup,
  authPickerFromState,
  authProfileFromPicker,
  ensureCredentialKeys,
  sanitizeAuthSpec,
} from "../authMatrix.ts";


const groups: Record<string, string> = {
  none: "empty",
  inherit: "empty",
  basic: "basic",
  api_key: "api_key",
  oauth_cc_form: "ticket",
  gettoken: "ticket",
  sign_form: "sign_form",
  grpc_metadata: "grpc_metadata",
  bearer: "bearer",
  jwt: "jwt",
  digest: "digest",
  aws_sigv4: "aws_sigv4",
  hawk: "hawk",
};

for (const id of AUTH_PICKER_IDS) {
  assert.equal(authFieldGroup(id), groups[id], id);
}

assert.equal(authProfileFromPicker("inherit"), "none");
assert.equal(authProfileFromPicker("basic"), "basic");
assert.equal(authPickerFromState("none", true), "inherit");
assert.equal(authPickerFromState("none", false), "none");
assert.equal(authPickerFromState("basic", true), "basic");

const flow = {
  enabled: false,
  profile: "",
  token_url: "",
  token_path: "",
  method: "",
  headers: [],
  request_body_tpl: "",
  credentials: [] as { key: string; value: string }[],
  token_jsonpath: "",
  expiry_jsonpath: "",
  inject_to: "",
  inject_key: "",
  inject_value_format: "",
  jwt: { algorithm: "HS256", ttl_secs: 300, claims_tpl: "{}", kid: "" },
  digest: { algorithm: "MD5", realm: "" },
  aws: { region: "", service: "" },
  hawk: { algorithm: "sha256", ext: "" },
};
flow.credentials = [
  { key: "username", value: "u1" },
  { key: "token_url_keep", value: "x" },
];
flow.token_url = "https://idp.example/token";
applyPickerDefaults("basic", flow);
applyPickerDefaults("oauth_cc_form", flow);
assert.equal(
  flow.credentials.find((c) => c.key === "username")?.value,
  "u1",
);
assert.equal(flow.token_url, "https://idp.example/token");
ensureCredentialKeys(flow, ["username", "password"]);
assert.ok(flow.credentials.some((c) => c.key === "password"));
assert.equal(flow.credentials.find((c) => c.key === "username")?.value, "u1");

const sanitizedBasic = sanitizeAuthSpec({
  profile: "basic",
  token_url: "https://idp.example/token",
  inject: { to: "header", key: "Authorization", format: "Bearer {}" },
});
assert.equal(sanitizedBasic?.profile, "basic");
assert.equal(sanitizedBasic?.token_url, undefined);
assert.equal(sanitizedBasic?.inject, undefined);

assert.equal(sanitizeAuthSpec({ profile: "none", token_url: "https://x" }), null);

const oauth = sanitizeAuthSpec({
  profile: "oauth_cc_form",
  token_url: "https://idp.example/token",
  request_body_tpl: "{\"a\":1}",
});
assert.equal(oauth?.token_url, "https://idp.example/token");
assert.ok(oauth?.request_body_tpl);

console.log("workbenchHelpers authMatrix ok");
