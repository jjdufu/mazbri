import { useCallback, useEffect, useMemo, useState } from "react";
import { fetchProductList } from "@/api/product";
import {
  createDefaultCBProPolicyConfig,
  fetchTenantList,
  grantProduct,
} from "@/api/tenant";
import {
  FormField,
  FormModal,
  Input,
  Select,
  Switch,
  type SelectOptionItem,
} from "@/components/ui";
import { useT } from "@/locales";
import type { Product } from "@/types/api/product";
import type { Tenant } from "@/types/api/tenant";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { logger } from "@/utils/sys/logger";
import { showSuccessToast } from "@/utils/ui/message-guard";

export interface CreateAuthorizationModalProps {
  open: boolean;
  lockedProduct?: Product | null;
  onClose: () => void;
  onSuccess: () => void | Promise<void>;
}

interface FormErrors {
  tenantCode?: string;
  productId?: string;
  unitPrice?: string;
}

export function CreateAuthorizationModal({
  open,
  lockedProduct = null,
  onClose,
  onSuccess,
}: CreateAuthorizationModalProps) {
  const { t } = useT();
  const [tenants, setTenants] = useState<Tenant[]>([]);
  const [products, setProducts] = useState<Product[]>([]);
  const [tenantCode, setTenantCode] = useState("");
  const [productId, setProductId] = useState("");
  const [unitPrice, setUnitPrice] = useState("0.001");
  const [cbProFallback, setCbProFallback] = useState(false);
  const [errors, setErrors] = useState<FormErrors>({});
  const [saving, setSaving] = useState(false);

  const loadOptions = useCallback(async () => {
    try {
      const [tenantRes, productRes] = await Promise.all([
        fetchTenantList({ page: 1, page_size: 100 }),
        lockedProduct
          ? Promise.resolve(null)
          : fetchProductList({ page: 1, page_size: 100 }),
      ]);
      setTenants(tenantRes.tenants ?? []);
      if (productRes) {
        setProducts(productRes.products ?? []);
      } else if (lockedProduct) {
        setProducts([lockedProduct]);
      }
    } catch (error) {
      logger.error("Failed to load authorization options:", error);
    }
  }, [lockedProduct]);

  useEffect(() => {
    if (!open) return;
    setTenantCode("");
    setProductId(lockedProduct?.product_id ?? "");
    setUnitPrice("0.001");
    setCbProFallback(false);
    setErrors({});
    void loadOptions();
  }, [open, lockedProduct, loadOptions]);

  const selectedProduct = useMemo(
    () =>
      lockedProduct ??
      products.find((item) => item.product_id === productId) ??
      null,
    [lockedProduct, products, productId],
  );

  useEffect(() => {
    if (!selectedProduct?.cb_pro_enabled) {
      setCbProFallback(false);
    }
  }, [selectedProduct?.cb_pro_enabled]);

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
    const next: FormErrors = {};
    if (!tenantCode.trim()) {
      next.tenantCode = t("authorization.grantDialog.selectTenant");
    }
    if (!productId.trim()) {
      next.productId = t("authorization.grantDialog.selectProduct");
    }
    const priceTrimmed = unitPrice.trim();
    const parsed = Number(priceTrimmed);
    if (!priceTrimmed || !Number.isFinite(parsed) || parsed < 0) {
      next.unitPrice = t("authorization.grantDialog.enterUnitPrice");
    }
    setErrors(next);
    return Object.keys(next).length === 0;
  };

  const handleSubmit = async () => {
    if (!validate() || saving) return;
    setSaving(true);
    try {
      await grantProduct(
        tenantCode.trim(),
        productId.trim(),
        {
          unit_price: unitPrice.trim(),
          cb_pro_fallback: selectedProduct?.cb_pro_enabled
            ? cbProFallback
            : false,
          cbpro: createDefaultCBProPolicyConfig(),
        },
        { showSuccessMessage: false },
      );
      showSuccessToast(t("authorization.grantDialog.success"));
      await onSuccess();
      onClose();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to grant authorization:", error);
      }
    } finally {
      setSaving(false);
    }
  };

  return (
    <FormModal
      open={open}
      onClose={onClose}
      onSubmit={handleSubmit}
      title={t("authorization.grantDialog.title")}
      size="md"
      submitLabel={t("authorization.grantDialog.confirm")}
      cancelLabel={t("common.cancel")}
      submitting={saving}
      testId="create-authorization-modal"
    >
      <FormField
        label={t("authorization.grantDialog.tenant")}
        required
        error={errors.tenantCode}
      >
        <Select
          value={tenantCode}
          options={tenantSelectOptions}
          searchable
          placeholder={t("authorization.grantDialog.searchTenantPlaceholder")}
          onChange={(e) => {
            setTenantCode(e.target.value);
            if (errors.tenantCode) {
              setErrors((prev) => ({ ...prev, tenantCode: undefined }));
            }
          }}
          className="w-full"
          data-testid="grant-tenant-select"
        />
      </FormField>

      <FormField
        label={t("authorization.grantDialog.product")}
        required
        error={errors.productId}
      >
        {lockedProduct ? (
          <Input
            value={`${lockedProduct.name} (${lockedProduct.product_id})`}
            disabled
          />
        ) : (
          <Select
            value={productId}
            options={productSelectOptions}
            searchable
            placeholder={t("authorization.grantDialog.searchProductPlaceholder")}
            onChange={(e) => {
              setProductId(e.target.value);
              if (errors.productId) {
                setErrors((prev) => ({ ...prev, productId: undefined }));
              }
            }}
            className="w-full"
            data-testid="grant-product-select"
          />
        )}
      </FormField>

      <FormField
        label={t("authorization.grantDialog.unitPrice")}
        htmlFor="grant-unit-price"
        required
        error={errors.unitPrice}
      >
        <Input
          id="grant-unit-price"
          inputMode="decimal"
          value={unitPrice}
          onChange={(e) => {
            setUnitPrice(e.target.value);
            if (errors.unitPrice) {
              setErrors((prev) => ({ ...prev, unitPrice: undefined }));
            }
          }}
          placeholder={t("authorization.grantDialog.unitPricePlaceholder")}
          className="font-mono-tech"
          data-testid="grant-unit-price-input"
        />
      </FormField>

      <FormField
        label={t("authorization.grantDialog.cbProFallback")}
        hint={
          !selectedProduct?.cb_pro_enabled
            ? t("authorization.config.cbProDisabledHint")
            : undefined
        }
      >
        <div className="flex items-center h-8">
          <Switch
            checked={cbProFallback}
            onCheckedChange={setCbProFallback}
            disabled={!selectedProduct?.cb_pro_enabled}
            aria-label={t("authorization.grantDialog.cbProFallback")}
          />
        </div>
      </FormField>
    </FormModal>
  );
}
