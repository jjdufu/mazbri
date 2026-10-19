import { ElMessage } from "element-plus";
import { computed, ref } from "vue";
import { fetchLicenseStatus } from "@/api/authorizations";
import { $t } from "@/locales";
import { logger } from "@/utils/sys";

export function getLicenseDisabledReason(status?: string | null) {
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

export function useLicenseGate() {
  const licenseActive = ref(false);
  const authorizationDisabledReason = ref(getLicenseDisabledReason());
  const authorizationDisabled = computed(() => !licenseActive.value);

  const loadLicenseStatus = async () => {
    try {
      const status = await fetchLicenseStatus();
      licenseActive.value = status.issuanceAllowed === true;
      authorizationDisabledReason.value = licenseActive.value
        ? ""
        : getLicenseDisabledReason(status.status);
    } catch (error) {
      logger.error("Failed to load license status:", error);
      licenseActive.value = false;
      authorizationDisabledReason.value = getLicenseDisabledReason();
    }
    return licenseActive.value;
  };

  const ensureLicenseActive = () => {
    if (licenseActive.value) return true;
    ElMessage.warning(authorizationDisabledReason.value);
    return false;
  };

  return {
    licenseActive,
    authorizationDisabled,
    authorizationDisabledReason,
    loadLicenseStatus,
    ensureLicenseActive,
  };
}
