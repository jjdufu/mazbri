/**
 * UpstreamCryptoProfile ↔ Workbench 端点字段互转与客户端预校验。
 * 算法闭集只引用共享 `@/components/crypto/cryptoCatalog`，禁止分叉拷贝。
 *
 * 职责边界（与 authMatrix 对照）：
 * - auth.profile / sign_form 凭证：会话鉴权、换票、注入头；不决定业务体加解密算法。
 * - egress.crypto（本模块）：业务体 order / sign / crypt / slots；发布时写入 ChannelEgressSpec.crypto。
 */

import {
  CRYPTO_ALGORITHMS,
  findAlgorithm,
  type CryptoAlgorithmMeta,
} from "@/components/crypto/cryptoCatalog";

export const CRYPTO_PROFILE_VERSION = 1;

export const CRYPTO_ORDERS = [
  "sign_then_encrypt",
  "encrypt_then_sign",
  "encrypt_only",
  "sign_only",
  "aead_only",
] as const;

export type CryptoOrderId = (typeof CRYPTO_ORDERS)[number];

export const CANON_STYLES = ["sorted_kv", "concat", "raw", "jws_style"] as const;
export type CanonStyleId = (typeof CANON_STYLES)[number];

export const IV_SOURCES = ["random", "random_prefix", "key_derive", "field"] as const;
export type IvSourceId = (typeof IV_SOURCES)[number];

export const CIPHER_SCOPES = ["whole_body", "named_fields"] as const;
export type CipherScopeId = (typeof CIPHER_SCOPES)[number];

export const SLOT_LOCATIONS = ["header", "query", "form", "json"] as const;
export type SlotLocationId = (typeof SLOT_LOCATIONS)[number];

export const WIRE_ENCODING_KINDS = ["base64", "base64_url", "hex_lower", "hex_upper"] as const;
export type WireEncodingKind = (typeof WIRE_ENCODING_KINDS)[number];

export type WireEncodingJson =
  | "base64"
  | "base64_url"
  | { hex: { case: "lower" | "upper" } };

export interface SlotSpecDraft {
  role: string;
  location: SlotLocationId;
  name: string;
}

export interface SignSpecDraft {
  alg: string;
  encoding?: WireEncodingJson | null;
}

export interface CryptSpecDraft {
  alg: string;
  encoding?: WireEncodingJson | null;
  fields?: string[];
}

/** 与后端 UpstreamCryptoProfile serde 对齐的草稿。 */
export interface UpstreamCryptoProfileDraft {
  version: number;
  order: CryptoOrderId;
  sign?: SignSpecDraft | null;
  crypt?: CryptSpecDraft | null;
  wrap?: { alg: string } | null;
  canon: CanonStyleId;
  encodings: {
    payload: WireEncodingJson;
    signature: WireEncodingJson;
    iv: WireEncodingJson;
  };
  slots: SlotSpecDraft[];
  iv: IvSourceId;
  scope: CipherScopeId;
  response_crypt?: CryptSpecDraft | null;
  allow_insecure: boolean;
}

export type CryptoProfileEditorModel = UpstreamCryptoProfileDraft | null;

export function wireEncodingToKind(enc: WireEncodingJson | null | undefined): WireEncodingKind {
  if (enc == null) return "base64";
  if (enc === "base64") return "base64";
  if (enc === "base64_url") return "base64_url";
  if (typeof enc === "object" && enc.hex) {
    return enc.hex.case === "upper" ? "hex_upper" : "hex_lower";
  }
  return "base64";
}

export function kindToWireEncoding(kind: WireEncodingKind): WireEncodingJson {
  switch (kind) {
    case "base64_url":
      return "base64_url";
    case "hex_lower":
      return { hex: { case: "lower" } };
    case "hex_upper":
      return { hex: { case: "upper" } };
    default:
      return "base64";
  }
}

export function defaultWireEncodings(): UpstreamCryptoProfileDraft["encodings"] {
  return {
    payload: "base64",
    signature: "base64",
    iv: "base64",
  };
}

export function defaultCryptoProfile(
  order: CryptoOrderId = "aead_only",
): UpstreamCryptoProfileDraft {
  const base: UpstreamCryptoProfileDraft = {
    version: CRYPTO_PROFILE_VERSION,
    order,
    canon: "raw",
    encodings: defaultWireEncodings(),
    slots: [],
    iv: "random",
    scope: "whole_body",
    allow_insecure: false,
    sign: null,
    crypt: null,
    wrap: null,
    response_crypt: null,
  };
  applyOrderDefaults(base);
  return base;
}

/** 按 order 清理 / 补齐 sign、crypt 子规格，避免无用字段残留。 */
export function applyOrderDefaults(profile: UpstreamCryptoProfileDraft): void {
  const needsSign =
    profile.order === "sign_only" ||
    profile.order === "sign_then_encrypt" ||
    profile.order === "encrypt_then_sign";
  const needsCrypt =
    profile.order === "encrypt_only" ||
    profile.order === "aead_only" ||
    profile.order === "sign_then_encrypt" ||
    profile.order === "encrypt_then_sign";

  if (!needsSign) {
    profile.sign = null;
  } else if (!profile.sign?.alg) {
    profile.sign = { alg: "hmac-sha256" };
  }

  if (!needsCrypt) {
    profile.crypt = null;
  } else if (!profile.crypt?.alg) {
    profile.crypt = {
      alg: profile.order === "aead_only" ? "aes-256-gcm" : "aes-256-cbc",
      fields: profile.crypt?.fields ?? [],
    };
  } else if (profile.order === "aead_only") {
    const meta = findAlgorithm(profile.crypt.alg);
    if (meta && meta.kind !== "aead") {
      profile.crypt = { ...profile.crypt, alg: "aes-256-gcm" };
    }
  }
}

function asRecord(value: unknown): Record<string, unknown> | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) return null;
  return value as Record<string, unknown>;
}

function parseWireEncoding(raw: unknown): WireEncodingJson | null {
  if (raw === "base64" || raw === "base64_url") return raw;
  const obj = asRecord(raw);
  if (!obj) return null;
  const hex = asRecord(obj.hex);
  if (hex && (hex.case === "lower" || hex.case === "upper")) {
    return { hex: { case: hex.case } };
  }
  return null;
}

function parseSign(raw: unknown): SignSpecDraft | null {
  const obj = asRecord(raw);
  if (!obj) return null;
  const alg = String(obj.alg || "").trim();
  if (!alg) return null;
  const encoding = parseWireEncoding(obj.encoding);
  return encoding ? { alg, encoding } : { alg };
}

function parseCrypt(raw: unknown): CryptSpecDraft | null {
  const obj = asRecord(raw);
  if (!obj) return null;
  const alg = String(obj.alg || "").trim();
  if (!alg) return null;
  const encoding = parseWireEncoding(obj.encoding);
  const fields = Array.isArray(obj.fields)
    ? obj.fields.map((f) => String(f).trim()).filter(Boolean)
    : [];
  const out: CryptSpecDraft = { alg, fields };
  if (encoding) out.encoding = encoding;
  return out;
}

function parseSlots(raw: unknown): SlotSpecDraft[] {
  if (!Array.isArray(raw)) return [];
  const out: SlotSpecDraft[] = [];
  for (const item of raw) {
    const obj = asRecord(item);
    if (!obj) continue;
    const role = String(obj.role || "").trim();
    const name = String(obj.name || "").trim();
    const location = String(obj.location || "").trim() as SlotLocationId;
    if (!role || !name) continue;
    if (!(SLOT_LOCATIONS as readonly string[]).includes(location)) continue;
    out.push({ role, location, name });
  }
  return out;
}

/**
 * 从端点 / egress JSON 解析 Profile；无效或缺省返回 null（表示不做业务密文）。
 */
export function parseCryptoProfile(raw: unknown): UpstreamCryptoProfileDraft | null {
  const obj = asRecord(raw);
  if (!obj) return null;
  const order = String(obj.order || "").trim() as CryptoOrderId;
  if (!(CRYPTO_ORDERS as readonly string[]).includes(order)) return null;
  const canon = String(obj.canon || "raw").trim() as CanonStyleId;
  const iv = String(obj.iv || "random").trim() as IvSourceId;
  const scope = String(obj.scope || "whole_body").trim() as CipherScopeId;
  const encObj = asRecord(obj.encodings) || {};
  const encodings = {
    payload: parseWireEncoding(encObj.payload) || ("base64" as const),
    signature: parseWireEncoding(encObj.signature) || ("base64" as const),
    iv: parseWireEncoding(encObj.iv) || ("base64" as const),
  };
  const version =
    typeof obj.version === "number" && Number.isFinite(obj.version) && obj.version >= 1
      ? Math.floor(obj.version)
      : CRYPTO_PROFILE_VERSION;

  const wrapObj = asRecord(obj.wrap);
  const wrapAlg = wrapObj ? String(wrapObj.alg || "").trim() : "";

  const profile: UpstreamCryptoProfileDraft = {
    version,
    order,
    sign: parseSign(obj.sign),
    crypt: parseCrypt(obj.crypt),
    wrap: wrapAlg ? { alg: wrapAlg } : null,
    canon: (CANON_STYLES as readonly string[]).includes(canon) ? canon : "raw",
    encodings,
    slots: parseSlots(obj.slots),
    iv: (IV_SOURCES as readonly string[]).includes(iv) ? iv : "random",
    scope: (CIPHER_SCOPES as readonly string[]).includes(scope) ? scope : "whole_body",
    response_crypt: parseCrypt(obj.response_crypt),
    allow_insecure: Boolean(obj.allow_insecure),
  };
  return profile;
}

/** 序列化为可写入 egress.crypto 的 JSON 对象；禁用时返回 null。 */
export function serializeCryptoProfile(
  profile: CryptoProfileEditorModel,
): Record<string, unknown> | null {
  if (!profile) return null;
  const needsSign =
    profile.order === "sign_only" ||
    profile.order === "sign_then_encrypt" ||
    profile.order === "encrypt_then_sign";
  const needsCrypt =
    profile.order === "encrypt_only" ||
    profile.order === "aead_only" ||
    profile.order === "sign_then_encrypt" ||
    profile.order === "encrypt_then_sign";

  const out: Record<string, unknown> = {
    version: profile.version || CRYPTO_PROFILE_VERSION,
    order: profile.order,
    canon: profile.canon,
    encodings: {
      payload: profile.encodings.payload,
      signature: profile.encodings.signature,
      iv: profile.encodings.iv,
    },
    iv: profile.iv,
    scope: profile.scope,
    allow_insecure: Boolean(profile.allow_insecure),
  };

  if (needsSign && profile.sign?.alg) {
    const sign: Record<string, unknown> = { alg: profile.sign.alg };
    if (profile.sign.encoding) sign.encoding = profile.sign.encoding;
    out.sign = sign;
  }
  if (needsCrypt && profile.crypt?.alg) {
    const crypt: Record<string, unknown> = { alg: profile.crypt.alg };
    if (profile.crypt.encoding) crypt.encoding = profile.crypt.encoding;
    const fields = (profile.crypt.fields || []).map((f) => f.trim()).filter(Boolean);
    if (fields.length) crypt.fields = fields;
    out.crypt = crypt;
  }
  if (profile.wrap?.alg) {
    out.wrap = { alg: profile.wrap.alg };
  }
  if (profile.slots?.length) {
    out.slots = profile.slots
      .filter((s) => s.role.trim() && s.name.trim())
      .map((s) => ({
        role: s.role.trim(),
        location: s.location,
        name: s.name.trim(),
      }));
  }
  if (profile.response_crypt?.alg) {
    const resp: Record<string, unknown> = { alg: profile.response_crypt.alg };
    if (profile.response_crypt.encoding) resp.encoding = profile.response_crypt.encoding;
    const fields = (profile.response_crypt.fields || [])
      .map((f) => f.trim())
      .filter(Boolean);
    if (fields.length) resp.fields = fields;
    out.response_crypt = resp;
  }
  return out;
}

export type CryptoProfileIssue = {
  code: string;
  message: string;
  /** legacy 门闩类提示，便于 UI 高亮 allow_insecure */
  legacy?: boolean;
};

function orderNeedsSign(order: CryptoOrderId): boolean {
  return (
    order === "sign_only" ||
    order === "sign_then_encrypt" ||
    order === "encrypt_then_sign"
  );
}

function orderNeedsCrypt(order: CryptoOrderId): boolean {
  return (
    order === "encrypt_only" ||
    order === "aead_only" ||
    order === "sign_then_encrypt" ||
    order === "encrypt_then_sign"
  );
}

function checkAlgLegacy(
  algId: string | null | undefined,
  allowInsecure: boolean,
  field: string,
  issues: CryptoProfileIssue[],
): CryptoAlgorithmMeta | undefined {
  if (!algId) return undefined;
  const meta = findAlgorithm(algId);
  if (!meta) {
    issues.push({
      code: "unknown_alg",
      message: `${field} 算法「${algId}」不在闭集中`,
    });
    return undefined;
  }
  if (meta.legacy && !allowInsecure) {
    issues.push({
      code: "legacy_gate",
      message: `${field} 使用 Legacy 算法「${algId}」，须显式开启 allow_insecure`,
      legacy: true,
    });
  }
  return meta;
}

/**
 * 客户端预校验（与后端 UpstreamCryptoProfile::validate 规则对齐的可提示子集）。
 * 返回空数组表示可通过；非空时不应发布。
 */
export function validateCryptoProfileClient(
  profile: CryptoProfileEditorModel,
): CryptoProfileIssue[] {
  if (!profile) return [];
  const issues: CryptoProfileIssue[] = [];
  if (!profile.version || profile.version < 1) {
    issues.push({ code: "version", message: "version 必须 ≥ 1" });
  }
  if (!(CRYPTO_ORDERS as readonly string[]).includes(profile.order)) {
    issues.push({ code: "order", message: `未知 order：${profile.order}` });
    return issues;
  }

  const needsSign = orderNeedsSign(profile.order);
  const needsCrypt = orderNeedsCrypt(profile.order);

  if (needsSign) {
    if (!profile.sign?.alg) {
      issues.push({ code: "sign_required", message: `order=${profile.order} 需要 sign.alg` });
    } else {
      const meta = checkAlgLegacy(
        profile.sign.alg,
        profile.allow_insecure,
        "sign",
        issues,
      );
      if (meta && meta.kind !== "mac" && meta.kind !== "asym_sign") {
        issues.push({
          code: "sign_kind",
          message: `sign.alg「${profile.sign.alg}」须为 mac 或 asym_sign`,
        });
      }
    }
  } else if (profile.sign?.alg) {
    issues.push({
      code: "sign_forbidden",
      message: `order=${profile.order} 不得设置 sign`,
    });
  }

  if (needsCrypt) {
    if (!profile.crypt?.alg) {
      issues.push({ code: "crypt_required", message: `order=${profile.order} 需要 crypt.alg` });
    } else {
      const meta = checkAlgLegacy(
        profile.crypt.alg,
        profile.allow_insecure,
        "crypt",
        issues,
      );
      if (meta && meta.kind !== "aead" && meta.kind !== "block_cipher") {
        issues.push({
          code: "crypt_kind",
          message: `crypt.alg「${profile.crypt.alg}」须为 aead 或 block_cipher`,
        });
      }
      if (profile.order === "aead_only" && meta && meta.kind !== "aead") {
        issues.push({
          code: "aead_only",
          message: `order=aead_only 要求 AEAD crypt.alg，当前为「${profile.crypt.alg}」`,
        });
      }
      if (
        profile.order === "encrypt_only" &&
        meta &&
        meta.kind === "block_cipher"
      ) {
        issues.push({
          code: "encrypt_only_integrity",
          message: `order=encrypt_only 禁止无完整性保护的分组密码「${profile.crypt.alg}」（改用 AEAD 或带 sign 的 order）`,
        });
      }
    }
  } else if (profile.crypt?.alg) {
    issues.push({
      code: "crypt_forbidden",
      message: `order=${profile.order} 不得设置 crypt`,
    });
  }

  if (profile.response_crypt?.alg) {
    const meta = checkAlgLegacy(
      profile.response_crypt.alg,
      profile.allow_insecure,
      "response_crypt",
      issues,
    );
    if (profile.order === "aead_only" && meta && meta.kind !== "aead") {
      issues.push({
        code: "response_aead",
        message: `order=aead_only 时 response_crypt.alg 须为 AEAD`,
      });
    }
    if (
      profile.order === "encrypt_only" &&
      meta &&
      meta.kind === "block_cipher"
    ) {
      issues.push({
        code: "response_integrity",
        message: `order=encrypt_only 时 response_crypt 不可使用无完整性的分组密码`,
      });
    }
  }

  if (profile.scope === "named_fields") {
    if (!profile.crypt?.alg && !profile.response_crypt?.alg) {
      issues.push({
        code: "named_fields_crypt",
        message: "scope=named_fields 需要 crypt 或 response_crypt",
      });
    }
    if (profile.crypt?.alg && !(profile.crypt.fields || []).some((f) => f.trim())) {
      issues.push({
        code: "named_fields_req",
        message: "scope=named_fields 需要 crypt.fields",
      });
    }
    if (
      profile.response_crypt?.alg &&
      !(profile.response_crypt.fields || []).some((f) => f.trim())
    ) {
      issues.push({
        code: "named_fields_resp",
        message: "scope=named_fields 需要 response_crypt.fields",
      });
    }
  }

  return issues;
}

/** 通用模板（无厂商品牌文案）。 */
export type CryptoProfileTemplateId =
  | "aead_aes256_gcm"
  | "gm_sm4_cbc_hmac_sm3"
  | "sign_only_hmac_sha256"
  | "encrypt_then_sign_aes_cbc_hmac";

export interface CryptoProfileTemplateMeta {
  id: CryptoProfileTemplateId;
  /** i18n key under upstreamConnector.cryptoTemplates.* */
  labelKey: string;
  build: () => UpstreamCryptoProfileDraft;
}

export const CRYPTO_PROFILE_TEMPLATES: readonly CryptoProfileTemplateMeta[] = [
  {
    id: "aead_aes256_gcm",
    labelKey: "upstreamConnector.cryptoTemplates.aeadAes256Gcm",
    build: () => {
      const p = defaultCryptoProfile("aead_only");
      p.crypt = { alg: "aes-256-gcm" };
      p.canon = "raw";
      p.slots = [
        { role: "cipher", location: "json", name: "biz_content" },
      ];
      return p;
    },
  },
  {
    id: "gm_sm4_cbc_hmac_sm3",
    labelKey: "upstreamConnector.cryptoTemplates.gmSm4CbcHmacSm3",
    build: () => {
      const p = defaultCryptoProfile("encrypt_then_sign");
      p.sign = { alg: "hmac-sm3", encoding: { hex: { case: "lower" } } };
      p.crypt = { alg: "sm4-cbc-pkcs7", encoding: { hex: { case: "lower" } } };
      p.canon = "sorted_kv";
      p.encodings = {
        payload: { hex: { case: "lower" } },
        signature: "base64",
        iv: "base64",
      };
      p.slots = [
        { role: "sign", location: "form", name: "sign" },
        { role: "cipher", location: "form", name: "biz_content" },
      ];
      return p;
    },
  },
  {
    id: "sign_only_hmac_sha256",
    labelKey: "upstreamConnector.cryptoTemplates.signOnlyHmacSha256",
    build: () => {
      const p = defaultCryptoProfile("sign_only");
      p.sign = { alg: "hmac-sha256" };
      p.canon = "sorted_kv";
      p.slots = [{ role: "sign", location: "form", name: "sign" }];
      return p;
    },
  },
  {
    id: "encrypt_then_sign_aes_cbc_hmac",
    labelKey: "upstreamConnector.cryptoTemplates.encryptThenSignAesCbc",
    build: () => {
      const p = defaultCryptoProfile("encrypt_then_sign");
      p.sign = { alg: "hmac-sha256" };
      p.crypt = { alg: "aes-256-cbc" };
      p.canon = "sorted_kv";
      p.slots = [
        { role: "sign", location: "json", name: "sign" },
        { role: "cipher", location: "json", name: "cipher" },
      ];
      return p;
    },
  },
] as const;

export function applyCryptoTemplate(
  id: CryptoProfileTemplateId,
): UpstreamCryptoProfileDraft {
  const tpl = CRYPTO_PROFILE_TEMPLATES.find((t) => t.id === id);
  if (!tpl) return defaultCryptoProfile();
  return structuredClone(tpl.build());
}

/** 往返：draft → JSON → draft，用于单测与发布前规范化。 */
export function roundTripCryptoProfile(
  profile: UpstreamCryptoProfileDraft,
): UpstreamCryptoProfileDraft | null {
  return parseCryptoProfile(serializeCryptoProfile(profile));
}

export function macOptionsForProfile(allowInsecure: boolean): CryptoAlgorithmMeta[] {
  return CRYPTO_ALGORITHMS.filter(
    (item) => item.kind === "mac" && (allowInsecure || !item.legacy),
  );
}

export function dataOptionsForProfile(
  allowInsecure: boolean,
  aeadOnly: boolean,
): CryptoAlgorithmMeta[] {
  return CRYPTO_ALGORITHMS.filter((item) => {
    if (item.kind !== "aead" && item.kind !== "block_cipher") return false;
    if (aeadOnly && item.kind !== "aead") return false;
    if (!allowInsecure && item.legacy) return false;
    return true;
  });
}

/** auth vs body crypto：authMatrix 的 sign_form 仅管凭证，不算第二套算法 UI。 */
export function isAuthSignFormProfile(authProfile: string | null | undefined): boolean {
  return String(authProfile || "").trim() === "sign_form";
}
