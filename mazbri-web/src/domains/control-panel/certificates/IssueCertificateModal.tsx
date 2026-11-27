import { useEffect, useMemo, useState } from "react";
import { issueCertificate } from "@/api/certificate";
import { fetchCanonicalServiceNames } from "@/api/service-names";
import {
  AuthorizationTooltipButton,
  FormField,
  FormModal,
  Select,
} from "@/components/ui";
import { useT } from "@/locales";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { logger } from "@/utils/sys/logger";
import {
  showErrorToast,
  showSuccessToast,
  showWarningToast,
} from "@/utils/ui/message-guard";

const VALIDITY_OPTIONS = [90, 180, 365, 730] as const;

export interface IssueCertificateModalProps {
  open: boolean;
  issuedSubjects: readonly string[];
  authorizationDisabled: boolean;
  authorizationDisabledReason: string;
  onClose: () => void;
  onSuccess: () => void;
}

export function IssueCertificateModal({
  open,
  issuedSubjects,
  authorizationDisabled,
  authorizationDisabledReason,
  onClose,
  onSuccess,
}: IssueCertificateModalProps) {
  const { t } = useT();
  const [services, setServices] = useState<string[]>([]);
  const [subject, setSubject] = useState("");
  const [validityDays, setValidityDays] = useState<number>(365);
  const [subjectError, setSubjectError] = useState<string | undefined>(
    undefined,
  );
  const [submitting, setSubmitting] = useState(false);
  const [batchSubmitting, setBatchSubmitting] = useState(false);

  useEffect(() => {
    if (!open) {
      setSubject("");
      setValidityDays(365);
      setSubjectError(undefined);
      return;
    }
    let active = true;
    fetchCanonicalServiceNames()
      .then((data) => {
        if (active) setServices(data.mtls_services ?? []);
      })
      .catch((error) => {
        logger.error("Failed to load service names:", error);
        if (active) setServices([]);
      });
    return () => {
      active = false;
    };
  }, [open]);

  const issuedSet = useMemo(() => new Set(issuedSubjects), [issuedSubjects]);
  const unissuedServices = useMemo(
    () => services.filter((s) => !issuedSet.has(s)),
    [services, issuedSet],
  );

  const serviceOptions = useMemo(
    () =>
      services.map((name) => {
        const isIssued = issuedSet.has(name);
        return {
          value: name,
          disabled: isIssued,
          text: isIssued
            ? `${name} (${t("certificate.issueDialog.issued")})`
            : name,
          label: isIssued ? (
            <span className="inline-flex items-center gap-1.5">
              <span className="line-through text-muted-foreground">{name}</span>
              <span className="text-[11px] text-danger-fg">
                ({t("certificate.issueDialog.issued")})
              </span>
            </span>
          ) : (
            name
          ),
        };
      }),
    [services, issuedSet, t],
  );

  const validityOptions = useMemo(
    () =>
      VALIDITY_OPTIONS.map((days) => {
        const label =
          days === 90
            ? t("certificate.issueDialog.validityOptions.90")
            : days === 180
              ? t("certificate.issueDialog.validityOptions.180")
              : days === 365
                ? t("certificate.issueDialog.validityOptions.365")
                : t("certificate.issueDialog.validityOptions.730");
        return { value: String(days), label, text: label };
      }),
    [t],
  );

  const handleSubmit = async () => {
    if (authorizationDisabled) {
      showWarningToast(authorizationDisabledReason);
      return;
    }
    if (!subject) {
      setSubjectError(t("certificate.issueDialog.validation.serviceRequired"));
      return;
    }
    setSubmitting(true);
    try {
      await issueCertificate(
        { subject, validity_days: validityDays },
        { showSuccessMessage: false },
      );
      showSuccessToast(t("certificate.messages.issueSuccess"));
      onClose();
      onSuccess();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to issue certificate:", error);
      }
    } finally {
      setSubmitting(false);
    }
  };

  const handleBatchIssue = async () => {
    if (authorizationDisabled) {
      showWarningToast(authorizationDisabledReason);
      return;
    }
    if (unissuedServices.length === 0) {
      showWarningToast(t("certificate.messages.noServiceToIssue"));
      return;
    }
    setBatchSubmitting(true);
    let successCount = 0;
    let failCount = 0;
    let cancelled = false;
    try {
      for (const target of unissuedServices) {
        try {
          await issueCertificate(
            { subject: target, validity_days: validityDays },
            { showSuccessMessage: false },
          );
          successCount += 1;
        } catch (error) {
          if (isMfaStepUpCancelled(error)) {
            cancelled = true;
            break;
          }
          failCount += 1;
          logger.error(`Failed to issue certificate for ${target}:`, error);
        }
      }
      if (!cancelled || successCount > 0) {
        if (failCount === 0 && successCount > 0) {
          showSuccessToast(
            t("certificate.messages.batchIssueSuccess", {
              count: successCount,
            }),
          );
        } else if (successCount === 0 && failCount > 0) {
          showErrorToast(t("errorPolicy.certificate"));
        } else if (successCount > 0 && failCount > 0) {
          showWarningToast(
            t("certificate.messages.batchIssuePartial", {
              successCount,
              failCount,
            }),
          );
        }
        onClose();
        onSuccess();
      }
    } finally {
      setBatchSubmitting(false);
    }
  };

  return (
    <FormModal
      open={open}
      onClose={onClose}
      onSubmit={handleSubmit}
      title={t("certificate.issueDialog.title")}
      size="md"
      submitLabel={t("certificate.issueDialog.issue")}
      cancelLabel={t("common.cancel")}
      submitting={submitting}
      submitDisabled={authorizationDisabled || batchSubmitting}
      testId="issue-certificate-modal"
      extraFooter={
        <div className="mr-auto">
          <AuthorizationTooltipButton
            type="button"
            variant="outline"
            loading={batchSubmitting}
            disabled={
              authorizationDisabled ||
              submitting ||
              unissuedServices.length === 0
            }
            disabledReason={
              authorizationDisabled ? authorizationDisabledReason : undefined
            }
            onClick={() => void handleBatchIssue()}
          >
            {t("certificate.issueDialog.issueAllUnissued", {
              count: unissuedServices.length,
            })}
          </AuthorizationTooltipButton>
        </div>
      }
    >
      <FormField
        label={t("certificate.issueDialog.service")}
        htmlFor="issue-cert-service"
        required
        error={subjectError}
        hint={t("certificate.issueDialog.sanAutoTip")}
      >
        <Select
          id="issue-cert-service"
          value={subject}
          onChange={(e) => {
            setSubject(e.target.value);
            setSubjectError(undefined);
          }}
          searchable
          placeholder={t("certificate.issueDialog.service")}
          options={serviceOptions}
          className="w-full"
        />
      </FormField>

      <FormField
        label={t("certificate.issueDialog.validity")}
        htmlFor="issue-cert-validity"
      >
        <Select
          id="issue-cert-validity"
          value={String(validityDays)}
          onChange={(e) => setValidityDays(Number(e.target.value))}
          options={validityOptions}
          className="w-56"
        />
      </FormField>
    </FormModal>
  );
}
