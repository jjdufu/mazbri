import assert from "node:assert/strict";
import {
  AUTH_PICKER_IDS,
  applyPickerDefaults,
  authFieldGroup,
  authPickerFromState,
  authProfileFromPicker,
  ensureCredentialKeys,
  sanitizeAuthSpec,
} from "../authMatrix";
import { isAuthSignFormProfile } from "../cryptoProfileHelpers";
import {
  HSPLIT_DEFAULT_LEFT,
  SPLIT_DEFAULT_RATIO,
  SPLIT_DEFAULT_RIGHT,
  VSPLIT_DEFAULT_BOTTOM,
  autoChannelsFromEndpoints,
  detectResponseBodyKind,
  ensureEndpointAdapterFields,
  listMissingAdapterFields,
  parseHSplitState,
  parseSplitLayoutState,
  parseVSplitState,
} from "../workbenchHelpers";
import type { ConnectorEndpoint } from "@/api/upstream-connectors";


const groups: Record<string, string> = {
  none: "empty",
  inherit: "empty",
  skip: "empty",
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
assert.equal(authProfileFromPicker("skip"), "skip");
assert.equal(authProfileFromPicker("basic"), "basic");
assert.equal(authPickerFromState("none", true, "bearer"), "inherit");
assert.equal(authPickerFromState("none", true, "none"), "none");
assert.equal(authPickerFromState("skip", true, "bearer"), "skip");
assert.equal(authPickerFromState("none", false), "none");
assert.equal(authPickerFromState("basic", true, "bearer"), "basic");

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
assert.deepEqual(sanitizeAuthSpec({ profile: "skip", token_url: "https://x" }), {
  profile: "skip",
});

const oauth = sanitizeAuthSpec({
  profile: "oauth_cc_form",
  token_url: "https://idp.example/token",
  request_body_tpl: "{\"a\":1}",
});
assert.equal(oauth?.token_url, "https://idp.example/token");
assert.ok(oauth?.request_body_tpl);

console.log("workbenchHelpers authMatrix ok");

assert.equal(isAuthSignFormProfile("sign_form"), true);
assert.equal(isAuthSignFormProfile("bearer"), false);
console.log("workbenchHelpers auth vs body crypto boundary ok");

assert.deepEqual(parseHSplitState(null), {
  left: HSPLIT_DEFAULT_LEFT,
  collapsed: false,
});
assert.deepEqual(parseHSplitState(""), {
  left: HSPLIT_DEFAULT_LEFT,
  collapsed: false,
});
assert.deepEqual(parseHSplitState("not-json"), {
  left: HSPLIT_DEFAULT_LEFT,
  collapsed: false,
});
assert.deepEqual(parseHSplitState("{}"), {
  left: HSPLIT_DEFAULT_LEFT,
  collapsed: false,
});
assert.deepEqual(parseHSplitState(JSON.stringify({ left: 320, collapsed: true })), {
  left: 320,
  collapsed: true,
});
assert.deepEqual(parseHSplitState(JSON.stringify({ left: 180, collapsed: false })), {
  left: 180,
  collapsed: false,
});
assert.deepEqual(parseHSplitState(JSON.stringify({ left: 200 })), {
  left: 200,
  collapsed: false,
});
assert.deepEqual(parseHSplitState(JSON.stringify({ left: -1, collapsed: true })), {
  left: HSPLIT_DEFAULT_LEFT,
  collapsed: true,
});

// VSplit defaults expanded (collapsed=false when missing storage)
assert.deepEqual(parseVSplitState(null), {
  bottom: VSPLIT_DEFAULT_BOTTOM,
  collapsed: false,
});
assert.equal(
  parseVSplitState(JSON.stringify({ bottom: 300, collapsed: true })).collapsed,
  true,
);
assert.deepEqual(parseSplitLayoutState(null), {
  direction: "horizontal",
  ratio: SPLIT_DEFAULT_RATIO,
  bottom: VSPLIT_DEFAULT_BOTTOM,
  right: SPLIT_DEFAULT_RIGHT,
  collapsed: false,
});
assert.deepEqual(
  parseSplitLayoutState(
    JSON.stringify({ direction: "vertical", ratio: 0.42, right: 480, collapsed: false }),
  ),
  {
    direction: "vertical",
    ratio: 0.42,
    bottom: VSPLIT_DEFAULT_BOTTOM,
    right: 480,
    collapsed: false,
  },
);

assert.equal(detectResponseBodyKind('{"ok":true}'), "json");
assert.equal(detectResponseBodyKind("error sending request for url (...)"), "raw");
assert.equal(detectResponseBodyKind("42"), "raw");
assert.equal(detectResponseBodyKind("42", "http_json", "application/json"), "json");
assert.equal(detectResponseBodyKind("<Envelope/>"), "xml");

const autoCh = autoChannelsFromEndpoints([
  {
    id: "ep-1",
    name: "SM4",
    method: "POST",
    path: "/sm4",
    channel_code: "CH-SM4",
    path_prefix: "/gw/sm4",
    cost_price: "0.025",
    qps_limit: 50,
  },
]);
assert.equal(autoCh[0]?.cost_price, "0.025");
assert.equal(autoCh[0]?.qps_limit, 50);

console.log("workbenchHelpers HSplit/VSplit persist parse ok");


// --- 03 adapter required fields ---
{
  const soap: ConnectorEndpoint = {
    adapter: "soap_xml",
    method: "POST",
    path: "/soap",
  };
  const miss = listMissingAdapterFields(soap).map((m) => m.path);
  assert.ok(miss.includes("operation"), String(miss));
  assert.ok(miss.includes("soap"), String(miss));
  ensureEndpointAdapterFields(soap);
  soap.operation = "RenderPhoto";
  assert.deepEqual(listMissingAdapterFields(soap), []);
}

{
  const env: ConnectorEndpoint = {
    adapter: "http_envelope",
    method: "POST",
    path: "/",
    operation: "pay",
    wrap: "method_biz_content",
  };
  assert.ok(
    listMissingAdapterFields(env).some((m) => m.path === "response.unwrap"),
  );
  env.unwrap = "$.data";
  assert.deepEqual(listMissingAdapterFields(env), []);
}

{
  const ws: ConnectorEndpoint = {
    adapter: "ws_unary",
    method: "POST",
    path: "/ws",
    operation: "ping",
  };
  ensureEndpointAdapterFields(ws);
  // ensure sets wrap + ws.path
  assert.deepEqual(listMissingAdapterFields(ws), []);
  ws.ws = { path: "", auth_in: [] };
  ws.wrap = "";
  const miss = listMissingAdapterFields(ws).map((m) => m.path);
  assert.ok(miss.includes("ws.path"), String(miss));
  assert.ok(miss.includes("request.wrap"), String(miss));
}

{
  const rpc: ConnectorEndpoint = {
    adapter: "jsonrpc",
    method: "POST",
    path: "/rpc",
  };
  assert.ok(listMissingAdapterFields(rpc).some((m) => m.path === "operation"));
  rpc.operation = "sum";
  assert.deepEqual(listMissingAdapterFields(rpc), []);
}

{
  const jp: ConnectorEndpoint = {
    adapter: "job_poll",
    method: "POST",
    path: "/jobs",
    operation: "submit",
    job_poll: { query_path: "", job_id_jsonpath: "" },
  };
  const miss = listMissingAdapterFields(jp).map((m) => m.path);
  assert.ok(miss.includes("job_poll.query_path"), String(miss));
  assert.ok(miss.includes("job_poll.job_id_jsonpath"), String(miss));
  assert.ok(!miss.includes("job_poll.status_jsonpath"), String(miss));
}

console.log("workbenchHelpers adapter required fields ok");
