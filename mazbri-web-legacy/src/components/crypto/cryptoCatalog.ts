/**
 * 与 mazbri-crypto 闭集对齐的前端唯一算法/套件目录。
 * Workbench（step-12）与租户传输页共用，禁止另写闭集。
 */

export type SecurityClass = "recommended" | "mainstream" | "gm" | "legacy";

export type AlgorithmKind =
  | "aead"
  | "block_cipher"
  | "mac"
  | "digest"
  | "asym_box"
  | "asym_sign";

export interface CryptoSuiteMeta {
  id: string;
  labelKey: string;
  dataAlg: string;
  macAlg: string | null;
  security: SecurityClass;
  gm: boolean;
}

export interface CryptoAlgorithmMeta {
  id: string;
  label: string;
  kind: AlgorithmKind;
  security: SecurityClass;
  /** 非 AEAD 对称在 Profile 层必须外挂 MAC */
  requireMac: boolean;
  legacy: boolean;
  gm: boolean;
}

/** 命名套件（优先选择） */
export const CRYPTO_SUITES: readonly CryptoSuiteMeta[] = [
  {
    id: "suite/tenant-default-2026-11",
    labelKey: "tenant.transportEncryption.suites.tenantDefault",
    dataAlg: "aes-256-gcm",
    macAlg: null,
    security: "recommended",
    gm: false,
  },
  {
    id: "suite/gm-sm4-hmac-sm3",
    labelKey: "tenant.transportEncryption.suites.gmSm4HmacSm3",
    dataAlg: "sm4-cbc-pkcs7",
    macAlg: "hmac-sm3",
    security: "gm",
    gm: true,
  },
  {
    id: "suite/tenant-chacha20",
    labelKey: "tenant.transportEncryption.suites.tenantChacha20",
    dataAlg: "chacha20-poly1305",
    macAlg: null,
    security: "recommended",
    gm: false,
  },
] as const;

const ALGORITHMS: CryptoAlgorithmMeta[] = [
  {
    id: "aes-256-gcm",
    label: "aes-256-gcm",
    kind: "aead",
    security: "recommended",
    requireMac: false,
    legacy: false,
    gm: false,
  },
  {
    id: "aes-128-gcm",
    label: "aes-128-gcm",
    kind: "aead",
    security: "mainstream",
    requireMac: false,
    legacy: false,
    gm: false,
  },
  {
    id: "aes-192-gcm",
    label: "aes-192-gcm",
    kind: "aead",
    security: "mainstream",
    requireMac: false,
    legacy: false,
    gm: false,
  },
  {
    id: "chacha20-poly1305",
    label: "chacha20-poly1305",
    kind: "aead",
    security: "recommended",
    requireMac: false,
    legacy: false,
    gm: false,
  },
  {
    id: "aes-256-cbc",
    label: "aes-256-cbc",
    kind: "block_cipher",
    security: "mainstream",
    requireMac: true,
    legacy: false,
    gm: false,
  },
  {
    id: "aes-128-cbc",
    label: "aes-128-cbc",
    kind: "block_cipher",
    security: "mainstream",
    requireMac: true,
    legacy: false,
    gm: false,
  },
  {
    id: "aes-256-ctr",
    label: "aes-256-ctr",
    kind: "block_cipher",
    security: "mainstream",
    requireMac: true,
    legacy: false,
    gm: false,
  },
  {
    id: "aes-128-ctr",
    label: "aes-128-ctr",
    kind: "block_cipher",
    security: "mainstream",
    requireMac: true,
    legacy: false,
    gm: false,
  },
  {
    id: "aes-256-cfb",
    label: "aes-256-cfb",
    kind: "block_cipher",
    security: "mainstream",
    requireMac: true,
    legacy: false,
    gm: false,
  },
  {
    id: "aes-128-cfb",
    label: "aes-128-cfb",
    kind: "block_cipher",
    security: "mainstream",
    requireMac: true,
    legacy: false,
    gm: false,
  },
  {
    id: "sm4-cbc-pkcs7",
    label: "sm4-cbc-pkcs7",
    kind: "block_cipher",
    security: "gm",
    requireMac: true,
    legacy: false,
    gm: true,
  },
  {
    id: "sm4-ctr",
    label: "sm4-ctr",
    kind: "block_cipher",
    security: "gm",
    requireMac: true,
    legacy: false,
    gm: true,
  },
  {
    id: "hmac-sha256",
    label: "hmac-sha256",
    kind: "mac",
    security: "recommended",
    requireMac: false,
    legacy: false,
    gm: false,
  },
  {
    id: "hmac-sha384",
    label: "hmac-sha384",
    kind: "mac",
    security: "recommended",
    requireMac: false,
    legacy: false,
    gm: false,
  },
  {
    id: "hmac-sha512",
    label: "hmac-sha512",
    kind: "mac",
    security: "recommended",
    requireMac: false,
    legacy: false,
    gm: false,
  },
  {
    id: "hmac-sm3",
    label: "hmac-sm3",
    kind: "mac",
    security: "gm",
    requireMac: false,
    legacy: false,
    gm: true,
  },
  {
    id: "aes-256-ecb",
    label: "aes-256-ecb",
    kind: "block_cipher",
    security: "legacy",
    requireMac: true,
    legacy: true,
    gm: false,
  },
  {
    id: "aes-128-ecb",
    label: "aes-128-ecb",
    kind: "block_cipher",
    security: "legacy",
    requireMac: true,
    legacy: true,
    gm: false,
  },
  {
    id: "sm4-ecb-pkcs7",
    label: "sm4-ecb-pkcs7",
    kind: "block_cipher",
    security: "legacy",
    requireMac: true,
    legacy: true,
    gm: true,
  },
  {
    id: "des-cbc-pkcs5",
    label: "des-cbc-pkcs5",
    kind: "block_cipher",
    security: "legacy",
    requireMac: true,
    legacy: true,
    gm: false,
  },
  {
    id: "hmac-md5",
    label: "hmac-md5",
    kind: "mac",
    security: "legacy",
    requireMac: false,
    legacy: true,
    gm: false,
  },
];

export const CRYPTO_ALGORITHMS: readonly CryptoAlgorithmMeta[] = ALGORITHMS;

export function findAlgorithm(id: string | null | undefined): CryptoAlgorithmMeta | undefined {
  if (!id) return undefined;
  return CRYPTO_ALGORITHMS.find((item) => item.id === id);
}

export function findSuite(id: string | null | undefined): CryptoSuiteMeta | undefined {
  if (!id) return undefined;
  return CRYPTO_SUITES.find((item) => item.id === id);
}

export function dataAlgorithmOptions(allowInsecure: boolean): CryptoAlgorithmMeta[] {
  return CRYPTO_ALGORITHMS.filter(
    (item) =>
      (item.kind === "aead" || item.kind === "block_cipher") &&
      (allowInsecure || !item.legacy),
  );
}

export function macAlgorithmOptions(allowInsecure: boolean): CryptoAlgorithmMeta[] {
  return CRYPTO_ALGORITHMS.filter(
    (item) => item.kind === "mac" && (allowInsecure || !item.legacy),
  );
}

export function securityClassLabelKey(security: SecurityClass): string {
  return `tenant.transportEncryption.securityClass.${security}`;
}
