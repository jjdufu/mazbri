import { useCallback, useState } from "react";
import { fetchLicenseStatus } from "@/api/authorizations";
import { $t } from "@/locales";
import { logger } from "@/utils/sys/logger";
import { showWarningToast } from "@/utils/ui/message-guard";

export function getLicenseDisabledReason(status?: string | null): string {
  if (status === "expired" || status === "grace") {
    return $t("certificate.authorizationManagement.license.disabled.expired");
  }
  if (status === "pending") {
    return $t("certificate.authorizationManagement.license.disabled.pending");
  }
  if (status === "invalid") {
    return $t("certificate.authorizationManagement.license.disabled.invalid");
  }
  return $t("certificate.authorizationManagement.license.disabled.unconfigured");
}

export interface LicenseGateState {
  licenseActive: boolean;
  authorizationDisabled: boolean;
  authorizationDisabledReason: string;
  loadLicenseStatus: () => Promise<boolean>;
  ensureLicenseActive: () => boolean;
}

export function useLicenseGate(): LicenseGateState {
  const [licenseActive, setLicenseActive] = useState(false);
  const [authorizationDisabledReason, setAuthorizationDisabledReason] = useState(
    () => getLicenseDisabledReason(),
  );

  const loadLicenseStatus = useCallback(async (): Promise<boolean> => {
    try {
      const status = await fetchLicenseStatus();
      const active = status.issuanceAllowed === true;
      setLicenseActive(active);
      setAuthorizationDisabledReason(
        active ? "" : getLicenseDisabledReason(status.status),
      );
      return active;
    } catch (error) {
      logger.error("Failed to load license status:", error);
      setLicenseActive(false);
      setAuthorizationDisabledReason(getLicenseDisabledReason());
      return false;
    }
  }, []);

  const ensureLicenseActive = useCallback((): boolean => {
    if (licenseActive) return true;
    showWarningToast(authorizationDisabledReason);
    return false;
  }, [licenseActive, authorizationDisabledReason]);

  return {
    licenseActive,
    authorizationDisabled: !licenseActive,
    authorizationDisabledReason,
    loadLicenseStatus,
    ensureLicenseActive,
  };
}
