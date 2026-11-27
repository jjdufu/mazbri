import { useCallback, useEffect, useMemo, useState } from "react";
import { Sparkles } from "lucide-react";
import { createCallbackSubscription } from "@/api/callback";
import { fetchTenantList, fetchTenantProducts } from "@/api/tenant";
import {
  Button,
  FormField,
  FormModal,
  Input,
  PasswordInput,
  Select,
  type SelectOptionItem,
} from "@/components/ui";
import { useT } from "@/locales";
import type { Tenant, TenantProduct } from "@/types/api/tenant";
import { getUserFacingErrorMessage } from "@/utils/http/error";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { showErrorToast, showSuccessToast } from "@/utils/ui/message-guard";

const SECRET_ALPHABET =
  "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const SECRET_LENGTH = 32;

function generateRandomSecretValue(): string {
  const chars: string[] = [];
  const maxMultiple =
    Math.floor(256 / SECRET_ALPHABET.length) * SECRET_ALPHABET.length;
  const buf = new Uint8Array(64);
  while (chars.length < SECRET_LENGTH) {
    crypto.getRandomValues(buf);
    for (const byte of buf) {
      if (byte < maxMultiple) {
        chars.push(SECRET_ALPHABET[byte % SECRET_ALPHABET.length]);
        if (chars.length === SECRET_LENGTH) break;
      }
    }
  }
  return chars.join("");
}

export interface CreateCallbackModalProps {
  open: boolean;
  onClose: () => void;
  onSuccess: () => void | Promise<void>;
}

export function CreateCallbackModal({
  open,
  onClose,
  onSuccess,
}: CreateCallbackModalProps) {
  const { t } = useT();
  const [submitting, setSubmitting] = useState(false);
  const [tenants, setTenants] = useState<Tenant[]>([]);
  const [products, setProducts] = useState<TenantProduct[]>([]);
  const [productLoading, setProductLoading] = useState(false);

  const [tenantCode, setTenantCode] = useState("");
  const [productCode, setProductCode] = useState("");
  const [callbackUrl, setCallbackUrl] = useState("");
  const [secretKey, setSecretKey] = useState("");
  const [errors, setErrors] = useState<Record<string, string>>({});

  const loadTenants = useCallback(async () => {
    try {
      const res = await fetchTenantList({ page: 1, page_size: 100 });
      setTenants(res.tenants ?? []);
    } catch {
      setTenants([]);
    }
  }, []);

  useEffect(() => {
    if (!open) return;
    setTenantCode("");
    setProductCode("");
    setCallbackUrl("");
    setSecretKey("");
    setProducts([]);
    setErrors({});
    void loadTenants();
  }, [open, loadTenants]);

  const handleTenantChange = async (nextCode: string) => {
    setTenantCode(nextCode);
    setProductCode("");
    setProducts([]);
    if (errors.tenant_code || errors.product_code) {
      setErrors((prev) => ({
        ...prev,
        tenant_code: "",
        product_code: "",
      }));
    }
    if (!nextCode) return;
    setProductLoading(true);
    try {
      const res = await fetchTenantProducts(nextCode, {
        page: 1,
        page_size: 100,
      });
      setProducts(res.products ?? []);
    } catch {
      setProducts([]);
    } finally {
      setProductLoading(false);
    }
  };

  const tenantSelectOptions = useMemo<SelectOptionItem[]>(
    () =>
      tenants.map((item) => ({
        value: item.tenant_code,
        label: `${item.name} (${item.tenant_code})`,
        text: `${item.name} ${item.tenant_code}`,
      })),
    [tenants],
  );

  const productSelectOptions = useMemo<SelectOptionItem[]>(
    () =>
      products.map((item) => ({
        value: item.product_id,
        label: `${item.name} (${item.product_id})`,
        text: `${item.name} ${item.product_id}`,
      })),
    [products],
  );

  const validate = (): boolean => {
    const next: Record<string, string> = {};
    if (!tenantCode.trim()) {
      next.tenant_code = t("callbacks.validation.tenantCodeRequired");
    }
    if (!productCode.trim()) {
      next.product_code = t("callbacks.validation.productCodeRequired");
    }
    if (!callbackUrl.trim()) {
      next.callback_url = t("callbacks.validation.callbackUrlRequired");
    }
    if (!secretKey.trim()) {
      next.secret_key = t("callbacks.validation.secretKeyRequired");
    }
    setErrors(next);
    return Object.keys(next).length === 0;
  };

  const handleSubmit = async () => {
    if (!validate() || submitting) return;
    setSubmitting(true);
    try {
      await createCallbackSubscription(
        {
          tenant_code: tenantCode.trim(),
          product_code: productCode.trim(),
          callback_url: callbackUrl.trim(),
          secret_key: secretKey.trim(),
        },
        { showSuccessMessage: false },
      );
      showSuccessToast(t("common.feedback.createSuccess"));
      onClose();
      await onSuccess();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        showErrorToast(
          getUserFacingErrorMessage(error, t("callbacks.messages.createFailed")),
        );
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
      title={t("callbacks.fields.addTitle")}
      size="md"
      submitLabel={t("common.confirm")}
      cancelLabel={t("common.cancel")}
      submitting={submitting}
      testId="create-callback-modal"
    >
      <FormField
        label={t("callbacks.fields.tenant")}
        required
        error={errors.tenant_code}
      >
        <Select
          value={tenantCode}
          options={tenantSelectOptions}
          searchable
          placeholder={t("callbacks.placeholders.tenantCode")}
          onChange={(e) => void handleTenantChange(e.target.value)}
          className="w-full"
          data-testid="callback-tenant-select"
        />
      </FormField>

      <FormField
        label={t("callbacks.fields.product")}
        required
        error={errors.product_code}
        hint={
          tenantCode && !productLoading && products.length === 0
            ? t("callbacks.messages.noAuthorizedProducts")
            : undefined
        }
      >
        <Select
          value={productCode}
          options={productSelectOptions}
          searchable
          disabled={!tenantCode || productLoading}
          placeholder={
            tenantCode
              ? t("callbacks.placeholders.productCode")
              : t("callbacks.placeholders.selectTenantFirst")
          }
          onChange={(e) => {
            setProductCode(e.target.value);
            if (errors.product_code) {
              setErrors((prev) => ({ ...prev, product_code: "" }));
            }
          }}
          className="w-full"
          data-testid="callback-product-select"
        />
      </FormField>

      <FormField
        label={t("callbacks.fields.url")}
        htmlFor="callback-url-input"
        required
        error={errors.callback_url}
      >
        <Input
          id="callback-url-input"
          value={callbackUrl}
          onChange={(e) => {
            setCallbackUrl(e.target.value);
            if (errors.callback_url) {
              setErrors((prev) => ({ ...prev, callback_url: "" }));
            }
          }}
          placeholder={t("callbacks.placeholders.callbackUrl")}
          className="font-mono-tech"
          data-testid="callback-url-input"
        />
      </FormField>

      <FormField
        label={t("callbacks.fields.secret")}
        htmlFor="callback-secret-input"
        required
        error={errors.secret_key}
      >
        <div className="flex items-center gap-2">
          <div className="flex-1 min-w-0">
            <PasswordInput
              id="callback-secret-input"
              value={secretKey}
              onChange={(e) => {
                setSecretKey(e.target.value);
                if (errors.secret_key) {
                  setErrors((prev) => ({ ...prev, secret_key: "" }));
                }
              }}
              placeholder={t("callbacks.placeholders.secretKey")}
              className="font-mono-tech"
              data-testid="callback-secret-input"
            />
          </div>
          <Button
            type="button"
            variant="outline"
            size="md"
            onClick={() => {
              setSecretKey(generateRandomSecretValue());
              if (errors.secret_key) {
                setErrors((prev) => ({ ...prev, secret_key: "" }));
              }
            }}
            data-testid="callback-generate-secret-btn"
          >
            <Sparkles className="h-3.5 w-3.5" />
            {t("callbacks.actions.generateSecret")}
          </Button>
        </div>
      </FormField>
    </FormModal>
  );
}
