import { useCallback, useEffect, useState } from "react";
import { Loader2 } from "lucide-react";
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
import type {
  CBProPolicyConfig,
  TenantProduct,
  TenantProductConfig,
} from "@/types/api/tenant";
import { HttpError } from "@/utils/http/error";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { logger } from "@/utils/sys/logger";
import { showSuccessToast } from "@/utils/ui/message-guard";
import { summarizeTenantProductConflict } from "./conflict-summary";

export interface AuthorizationConfigModalProps {
  open: boolean;
  tenantCode: string;
  product: TenantProduct | null;
  onClose: () => void;
  onUpdated: () => void | Promise<void>;
}

export function AuthorizationConfigModal({
  open,
  tenantCode,
  product,
  onClose,
  onUpdated,
}: AuthorizationConfigModalProps) {
  const { t } = useT();
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [currentProduct, setCurrentProduct] = useState<TenantProduct | null>(
    null,
  );
  const [unitPrice, setUnitPrice] = useState("0.001");
  const [cbProFallback, setCbProFallback] = useState(false);
  const [cbpro, setCbpro] = useState<CBProPolicyConfig>(() =>
    createDefaultCBProPolicyConfig(),
  );
  const [conflictSummary, setConflictSummary] = useState<string | null>(null);

  const syncFromProduct = useCallback((item: TenantProduct | null) => {
    if (!item) return;
    const merged = mergeTenantProductConfig(item.config);
    setCurrentProduct({ ...item, config: merged });
    setUnitPrice(item.config.unit_price);
    setCbProFallback(item.cb_pro_enabled ? item.config.cb_pro_fallback : false);
    setCbpro(merged.cbpro);
  }, []);

  const loadCurrentConfig = useCallback(async () => {
    if (!product || !tenantCode) return;
    syncFromProduct(product);
    setLoading(true);
    try {
      const latest = await fetchTenantProductConfig(
        tenantCode,
        product.product_id,
      );
      syncFromProduct(latest);
    } catch (error) {
      logger.error("Failed to load latest tenant product config:", error);
    } finally {
      setLoading(false);
    }
  }, [product, tenantCode, syncFromProduct]);

  useEffect(() => {
    if (open && product) {
      setConflictSummary(null);
      void loadCurrentConfig();
    }
  }, [open, product, loadCurrentConfig]);

  const activeProduct = currentProduct ?? product;
  const title = activeProduct
    ? `${t("authorization.config.title")} · ${activeProduct.name}`
    : t("authorization.config.title");

  const handleSave = async () => {
    const target = currentProduct ?? product;
    if (!target || saving) return;

    const submittedPayload: TenantProductConfig = {
      unit_price: unitPrice.trim(),
      currency: "CNY",
      cb_pro_fallback: target.cb_pro_enabled ? cbProFallback : false,
      cbpro: mergeTenantProductConfig({
        unit_price: unitPrice.trim(),
        currency: "CNY",
        cb_pro_fallback: cbProFallback,
        cbpro,
      }).cbpro,
    };

    setSaving(true);
    try {
      await updateTenantProductConfig(tenantCode, target.product_id, {
        config: submittedPayload,
        expected_updated_at: target.updated_at,
      });
      showSuccessToast(t("authorization.config.saveSuccess"));
      await onUpdated();
      onClose();
    } catch (error) {
      if (isMfaStepUpCancelled(error)) return;
      if (error instanceof HttpError && error.code === 409) {
        try {
          const latest = await fetchTenantProductConfig(
            tenantCode,
            target.product_id,
          );
          syncFromProduct(latest);
          setConflictSummary(
            summarizeTenantProductConflict(
              submittedPayload,
              latest.config,
              latest.updated_at,
            ),
          );
        } catch {
          setConflictSummary(
            summarizeTenantProductConflict(
              submittedPayload,
              target.config,
              target.updated_at,
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
        title={title}
        size="md"
        submitLabel={t("common.actions.save")}
        cancelLabel={t("common.cancel")}
        submitting={saving}
        submitDisabled={!activeProduct || loading}
        testId="authorization-config-modal"
      >
        {loading ? (
          <div className="flex items-center justify-center gap-2 py-8 text-xs text-muted-foreground">
            <Loader2 className="h-4 w-4 animate-spin text-primary" />
          </div>
        ) : (
          <div className="space-y-4">
            <div className="space-y-3">
              <div className="border-b border-border pb-1.5 text-xs font-semibold text-foreground">
                {t("authorization.config.billingSection")}
              </div>
              <FormField
                label={t("authorization.config.unitPrice")}
                htmlFor="auth-config-unit-price"
              >
                <Input
                  id="auth-config-unit-price"
                  inputMode="decimal"
                  value={unitPrice}
                  onChange={(e) => setUnitPrice(e.target.value)}
                  placeholder={t("authorization.config.unitPricePlaceholder")}
                  className="font-mono-tech"
                />
              </FormField>
            </div>

            <div className="space-y-3">
              <div className="border-b border-border pb-1.5 text-xs font-semibold text-foreground">
                {t("authorization.config.fallbackSection")}
              </div>
              <FormField
                label={t("authorization.config.cbProFallback")}
                hint={
                  !activeProduct?.cb_pro_enabled
                    ? t("authorization.config.cbProDisabledHint")
                    : undefined
                }
              >
                <div className="flex items-center h-8">
                  <Switch
                    checked={cbProFallback}
                    onCheckedChange={setCbProFallback}
                    disabled={!activeProduct?.cb_pro_enabled}
                    aria-label={t("authorization.config.cbProFallback")}
                  />
                </div>
              </FormField>
            </div>
          </div>
        )}
      </FormModal>

      <Modal
        open={Boolean(conflictSummary)}
        onClose={() => setConflictSummary(null)}
        title={t("authorization.config.conflictTitle")}
        size="sm"
        footer={
          <Button
            type="button"
            variant="primary"
            onClick={() => setConflictSummary(null)}
          >
            {t("common.confirm")}
          </Button>
        }
      >
        <pre className="whitespace-pre-wrap font-sans text-xs leading-relaxed text-foreground">
          {conflictSummary}
        </pre>
      </Modal>
    </>
  );
}
