import { useEffect, useState } from "react";
import { createProduct } from "@/api/product";
import {
  Badge,
  FormField,
  FormModal,
  Input,
  NumberInput,
  Switch,
} from "@/components/ui";
import { useT } from "@/locales";
import {
  PRODUCT_ID_PATTERN,
  type ProductIdReservation,
} from "@/types/api/product";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { logger } from "@/utils/sys/logger";

export interface CreateProductModalProps {
  open: boolean;
  reservation: ProductIdReservation | null;
  onClose: () => void;
  onSuccess: () => void | Promise<void>;
}

interface FormErrors {
  product_id?: string;
  name?: string;
  timeout_secs?: string;
}

export function CreateProductModal({
  open,
  reservation,
  onClose,
  onSuccess,
}: CreateProductModalProps) {
  const { t } = useT();
  const [name, setName] = useState("");
  const [timeoutSecs, setTimeoutSecs] = useState(30);
  const [enabled, setEnabled] = useState(true);
  const [cbProEnabled, setCbProEnabled] = useState(false);
  const [errors, setErrors] = useState<FormErrors>({});
  const [submitting, setSubmitting] = useState(false);

  const productId = reservation?.product_id ?? "";

  useEffect(() => {
    if (open) {
      setName("");
      setTimeoutSecs(30);
      setEnabled(true);
      setCbProEnabled(false);
      setErrors({});
    }
  }, [open, reservation?.id]);

  const validate = (): boolean => {
    const next: FormErrors = {};
    if (!reservation?.id || !productId) {
      next.product_id = t(
        "product.createDialog.validation.productIdReservationFailed",
      );
    } else if (!PRODUCT_ID_PATTERN.test(productId)) {
      next.product_id = t("product.createDialog.validation.productIdPattern");
    }

    const trimmedName = name.trim();
    if (!trimmedName) {
      next.name = t("product.createDialog.validation.nameRequired");
    } else if (trimmedName.length < 2 || trimmedName.length > 100) {
      next.name = t("product.createDialog.validation.nameLength");
    }

    if (
      !Number.isFinite(timeoutSecs) ||
      timeoutSecs < 1 ||
      timeoutSecs > 300
    ) {
      next.timeout_secs = t("product.createDialog.validation.timeoutSecsRange");
    }

    setErrors(next);
    return Object.keys(next).length === 0;
  };

  const handleSubmit = async () => {
    if (!validate() || !reservation?.id || submitting) return;
    setSubmitting(true);
    try {
      await createProduct({
        reservation_id: reservation.id,
        name: name.trim(),
        enabled,
        timeout_secs: timeoutSecs,
        cb_pro_enabled: cbProEnabled,
      });
      await onSuccess();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to create product:", error);
      }
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <FormModal
      open={open}
      onClose={onClose}
      onSubmit={handleSubmit}
      title={t("product.createDialog.title")}
      size="md"
      submitLabel={t("common.confirm")}
      cancelLabel={t("common.cancel")}
      submitting={submitting}
      testId="create-product-modal"
    >
      <FormField
        label={t("product.fields.productId")}
        required
        error={errors.product_id}
        hint={t("product.createDialog.hints.productId")}
      >
        <div className="relative flex items-center">
          <Input
            value={productId}
            disabled
            className="font-mono-tech pr-28"
            data-testid="create-product-id-input"
          />
          <Badge
            variant="neutral"
            className="pointer-events-none absolute right-2"
          >
            {t("product.createDialog.autoAssignedBadge")}
          </Badge>
        </div>
      </FormField>

      <FormField
        label={t("product.fields.name")}
        htmlFor="create-product-name"
        required
        error={errors.name}
      >
        <Input
          id="create-product-name"
          value={name}
          onChange={(e) => {
            setName(e.target.value);
            if (errors.name) setErrors((prev) => ({ ...prev, name: undefined }));
          }}
          placeholder={t("product.createDialog.placeholders.name")}
          data-testid="create-product-name-input"
        />
      </FormField>

      <FormField
        label={t("product.fields.timeout")}
        required
        error={errors.timeout_secs}
      >
        <div className="flex items-center gap-2">
          <NumberInput
            value={timeoutSecs}
            onChange={(val) => {
              setTimeoutSecs(val);
              if (errors.timeout_secs) {
                setErrors((prev) => ({ ...prev, timeout_secs: undefined }));
              }
            }}
            min={1}
            max={300}
            aria-label={t("product.fields.timeout")}
            data-testid="create-product-timeout-input"
          />
          <span className="text-xs text-muted-foreground shrink-0">
            {t("ops.units.s")}
          </span>
        </div>
      </FormField>

      <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 pt-1">
        <FormField label={t("product.fields.enabled")}>
          <div className="flex items-center h-8">
            <Switch
              checked={enabled}
              onCheckedChange={setEnabled}
              aria-label={t("product.fields.enabled")}
              data-testid="create-product-enabled-switch"
            />
          </div>
        </FormField>

        <FormField label={t("product.fields.cbProEnabled")}>
          <div className="flex items-center h-8">
            <Switch
              checked={cbProEnabled}
              onCheckedChange={setCbProEnabled}
              aria-label={t("product.fields.cbProEnabled")}
              data-testid="create-product-cbpro-switch"
            />
          </div>
        </FormField>
      </div>
    </FormModal>
  );
}
