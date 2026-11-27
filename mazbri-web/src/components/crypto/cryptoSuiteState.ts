import { CRYPTO_SUITES, findAlgorithm, type CryptoAlgorithmMeta } from "./cryptoCatalog";

export type CryptoSelectionMode = "suite" | "custom";

export interface CryptoSuiteSelectValue {
  suite: string | null;
  dataAlg: string | null;
  macAlg: string | null;
  allowInsecure: boolean;
}

export const DEFAULT_CRYPTO_SUITE_ID =
  CRYPTO_SUITES[0]?.id ?? "suite/tenant-default-2026-11";

/**
 * Determine initial selection mode from an external model value.
 * Only values with no suite and an explicit custom dataAlg/macAlg start in "custom".
 */
export function resolveInitialSelectionMode(
  value: Pick<CryptoSuiteSelectValue, "suite" | "dataAlg" | "macAlg">,
): CryptoSelectionMode {
  if (!value.suite && (value.dataAlg || value.macAlg)) {
    return "custom";
  }
  return "suite";
}

/**
 * Apply an explicit user mode switch without causing bounce-back loops.
 * Switching to "suite" restores a default suite and clears custom overrides;
 * switching to "custom" clears suite while preserving any chosen custom algorithms.
 */
export function applySelectionModeChange(
  current: CryptoSuiteSelectValue,
  nextMode: CryptoSelectionMode,
  fallbackSuiteId: string = DEFAULT_CRYPTO_SUITE_ID,
): CryptoSuiteSelectValue {
  if (nextMode === "suite") {
    return {
      ...current,
      suite: current.suite || fallbackSuiteId,
      dataAlg: null,
      macAlg: null,
    };
  }
  return {
    ...current,
    suite: null,
  };
}

/**
 * Return all currently selected algorithms flagged `legacy: true`
 * (covers aes-256-ecb, aes-128-ecb, sm4-ecb-pkcs7, des-cbc-pkcs5, hmac-md5).
 */
export function detectLegacyAlgorithms(
  value: Pick<CryptoSuiteSelectValue, "dataAlg" | "macAlg">,
): CryptoAlgorithmMeta[] {
  const legacy: CryptoAlgorithmMeta[] = [];
  const dataMeta = findAlgorithm(value.dataAlg);
  if (dataMeta?.legacy) legacy.push(dataMeta);
  const macMeta = findAlgorithm(value.macAlg);
  if (macMeta?.legacy) legacy.push(macMeta);
  return legacy;
}
