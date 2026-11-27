import { useCallback, useEffect, useState } from "react";
import { AlertTriangle } from "lucide-react";
import {
  createDefaultCBProPolicyConfig,
  fetchTenantProductConfig,
  mergeTenantProductConfig,
  updateTenantProductConfig,
} from "@/api/tenant";
import {
  Button,
  FormField,
  FormModal,
  Input,
  Modal,
  Switch,
} from "@/components/ui";
import { useT } from "@/locales";
import type { TenantProduct, TenantProductConfig } from "@/types/api/tenant";
import { HttpError } from "@/utils/http/error";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { logger } from "@/utils/sys/logger";
import { showSuccessToast } from "@/utils/ui/message-guard";
import { summarizeTenantProductConflict } from "../helpers/conflict-summary";

export interface ProductConfigModalProps {
  open: boolean;
  tenantCode: string;
  product: TenantProduct | null;
  onClose: () => void;
  onUpdated: () => void;
}

const UNIT_PRICE_RE = /^(?:0|[1-9]\d{0,11})(?:\.\d{1,6})?$/;

export function ProductConfigModal({
  open,
  tenantCode,
  product,
  onClose,
  onUpdated,
}: ProductConfigModalProps) {
  const { t } = useT();
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [currentProduct, setCurrentProduct] = useState<TenantProduct | null>(
    null,
  );
  const [form, setForm] = useState<TenantProductConfig>({
    unit_price: "0.001",
    currency: "CNY",
    cb_pro_fallback: false,
    cbpro: createDefaultCBProPolicyConfig(),
  });
  const [priceError, setPriceError] = useState<string | undefined>(undefined);
  const [conflictText, setConflictText] = useState<string | null>(null);

  const syncFromProduct = useCallback((item: TenantProduct) => {
    const merged = mergeTenantProductConfig(item.config);
    setCurrentProduct({ ...item, config: merged });
    setForm({
      unit_price: merged.unit_price,
      currency: "CNY",
      cb_pro_fallback: item.cb_pro_enabled ? merged.cb_pro_fallback : false,
      cbpro: merged.cbpro,
    });
    setPriceError(undefined);
  }, []);

  const loadLatest = useCallback(
    async (target: TenantProduct): Promise<TenantProduct | null> => {
      setLoading(true);
      try {
        const latest = await fetchTenantProductConfig(
          tenantCode,
          target.product_id,
        );
        syncFromProduct(latest);
        return latest;
      } catch (error) {
        logger.error("Failed to load latest tenant product config:", error);
        return null;
      } finally {
        setLoading(false);
      }
    },
    [tenantCode, syncFromProduct],
  );

  useEffect(() => {
    if (open && product) {
      syncFromProduct(product);
      void loadLatest(product);
    }
  }, [open, product, syncFromProduct, loadLatest]);

  const activeProduct = currentProduct ?? product;

  const handleSave = async () => {
    if (!activeProduct || saving) return;
    const trimmedPrice = form.unit_price.trim();
    if (!UNIT_PRICE_RE.test(trimmedPrice)) {
      setPriceError(t("authorization.config.unitPricePlaceholder"));
      return;
    }
    setPriceError(undefined);

    const submittedPayload: TenantProductConfig = {
      ...form,
      unit_price: trimmedPrice,
      currency: "CNY",
      cb_pro_fallback: activeProduct.cb_pro_enabled
        ? form.cb_pro_fallback
        : false,
      cbpro: mergeTenantProductConfig(form).cbpro,
    };

    setSaving(true);
    try {
      await updateTenantProductConfig(tenantCode, activeProduct.product_id, {
        config: submittedPayload,
        expected_updated_at: activeProduct.updated_at,
      });
      showSuccessToast(t("authorization.config.saveSuccess"));
      onUpdated();
      onClose();
    } catch (error) {
      if (isMfaStepUpCancelled(error)) return;
      if (error instanceof HttpError && error.code === 409) {
        const latest = await loadLatest(activeProduct);
        if (latest) {
          setConflictText(
            summarizeTenantProductConflict(
              submittedPayload,
              latest.config,
              latest.updated_at,
            ),
          );
        }
      }
      logger.error("Failed to update tenant product config:", error);
    } finally {
      setSaving(false);
    }
  };

  return (
    <>
      <FormModal
        open={open}
        onClose={onClose}
        onSubmit={handleSave}
        title={
          activeProduct
            ? `${t("authorization.config.title")} · ${activeProduct.name}`
            : t("authorization.config.title")
        }
        size="md"
        submitting={saving}
        submitDisabled={!activeProduct || loading}
        testId="tenant-product-config-modal"
      >
        <div className="space-y-4">
          <div className="space-y-2">
            <div className="text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
              {t("authorization.config.billingSection")}
            </div>
            <FormField
              label={t("authorization.config.unitPrice")}
              htmlFor="tenant-product-unit-price"
              required
              error={priceError}
            >
              <div className="flex items-center gap-2">
                <Input
                  id="tenant-product-unit-price"
                  value={form.unit_price}
                  disabled={loading}
                  onChange={(e) => {
                    setForm((prev) => ({
                      ...prev,
                      unit_price: e.target.value,
                    }));
                    if (priceError) setPriceError(undefined);
                  }}
                  placeholder={t("authorization.config.unitPricePlaceholder")}
                  className="font-mono-tech"
                />
                <span className="shrink-0 whitespace-nowrap text-xs text-muted-foreground">
                  {t("common.currencies.cny")}
                </span>
              </div>
            </FormField>
          </div>

          <div className="space-y-2 border-t border-border pt-3">
            <div className="text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
              {t("authorization.config.fallbackSection")}
            </div>
            <div className="flex items-center justify-between gap-4 rounded-md border border-border-subtle bg-muted/25 px-3 py-2.5">
              <span className="text-xs font-medium text-foreground">
                {t("authorization.config.cbProFallback")}
              </span>
              <Switch
                checked={form.cb_pro_fallback}
                disabled={loading || !activeProduct?.cb_pro_enabled}
                onCheckedChange={(checked) =>
                  setForm((prev) => ({ ...prev, cb_pro_fallback: checked }))
                }
                aria-label={t("authorization.config.cbProFallback")}
              />
            </div>
          </div>
        </div>
      </FormModal>

      <Modal
        open={Boolean(conflictText)}
        onClose={() => setConflictText(null)}
        title={
          <span className="inline-flex items-center gap-1.5">
            <AlertTriangle className="h-4 w-4 text-warning-fg" />
            <span>{t("authorization.config.conflictTitle")}</span>
          </span>
        }
        size="sm"
        footer={
          <Button
            type="button"
            variant="primary"
            onClick={() => setConflictText(null)}
          >
            {t("common.confirm")}
          </Button>
        }
      >
        <pre className="whitespace-pre-wrap rounded-md border border-border bg-muted/35 p-3 font-mono-tech text-xs text-foreground">
          {conflictText}
        </pre>
      </Modal>
    </>
  );
}
