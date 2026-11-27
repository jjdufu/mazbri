import { useCallback, useEffect, useMemo, useState } from "react";
import { AlertTriangle, Check, CheckCircle2 } from "lucide-react";
import { fetchProductList } from "@/api/product";
import {
  createOnboarding,
  dryRunOnboarding,
  type OnboardingRequest,
  type OnboardingResult,
} from "@/api/upstream-onboarding";
import {
  Badge,
  Button,
  CopyableText,
  Drawer,
  EmptyState,
  FormField,
  Input,
  NumberInput,
  Select,
  type SelectOptionItem,
} from "@/components/ui";
import { useT } from "@/locales";
import type { Product } from "@/types/api/product";
import { EMPTY_PLACEHOLDER } from "@/utils/format";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { stripUrlScheme } from "@/utils/sensitive";
import { showErrorToast, showSuccessToast } from "@/utils/ui/message-guard";

export interface UpstreamAccessWizardDrawerProps {
  open: boolean;
  onClose: () => void;
  onSuccess: () => void;
}

const CHANNEL_CODE_RE = /^[A-Z][A-Z0-9-]*$/;

interface WizardForm {
  supplier_code: string;
  name: string;
  upstream_url: string;
  contract_id: string;
  timeout_ms: number;
  channel_code: string;
  channel_name: string;
  path_prefix: string;
  product_id: string;
  weight: number;
  priority: number;
}

const INITIAL_FORM: WizardForm = {
  supplier_code: "",
  name: "",
  upstream_url: "",
  contract_id: "",
  timeout_ms: 8000,
  channel_code: "",
  channel_name: "",
  path_prefix: "/",
  product_id: "",
  weight: 100,
  priority: 100,
};

export function UpstreamAccessWizardDrawer({
  open,
  onClose,
  onSuccess,
}: UpstreamAccessWizardDrawerProps) {
  const { t } = useT();
  const [step, setStep] = useState<0 | 1 | 2 | 3>(0);
  const [submitting, setSubmitting] = useState(false);
  const [productOptions, setProductOptions] = useState<Product[]>([]);
  const [dryRunResult, setDryRunResult] = useState<OnboardingResult | null>(
    null,
  );
  const [result, setResult] = useState<OnboardingResult | null>(null);
  const [form, setForm] = useState<WizardForm>(INITIAL_FORM);
  const [errors, setErrors] = useState<Record<string, string>>({});

  const loadProducts = useCallback(async () => {
    try {
      const res = await fetchProductList({ page: 1, page_size: 100 });
      setProductOptions((res.products ?? []).filter((item) => item.enabled));
    } catch {
      setProductOptions([]);
    }
  }, []);

  useEffect(() => {
    if (open) {
      setStep(0);
      setDryRunResult(null);
      setResult(null);
      setForm(INITIAL_FORM);
      setErrors({});
      setProductOptions([]);
    }
  }, [open]);

  const productSelectOptions = useMemo<SelectOptionItem[]>(
    () =>
      productOptions.map((item) => ({
        value: item.product_id,
        label: `${item.name} (${item.product_id})`,
        text: `${item.name} ${item.product_id}`,
      })),
    [productOptions],
  );

  const selectedProductSummary = useMemo(() => {
    const externalId =
      dryRunResult?.plan?.product_external_id || form.product_id.trim();
    const matched = productOptions.find((p) => p.product_id === externalId);
    if (matched) return `${matched.name} (${matched.product_id})`;
    return externalId || EMPTY_PLACEHOLDER;
  }, [dryRunResult, form.product_id, productOptions]);

  const formatPlanAction = (action: string): string => {
    switch (action) {
      case "insert suppliers":
        return t("upstreamOnboarding.confirm.actionInsertSuppliers");
      case "insert channels":
        return t("upstreamOnboarding.confirm.actionInsertChannels");
      case "insert route_rules":
        return t("upstreamOnboarding.confirm.actionInsertRouteRules");
      case "insert route_channel_bindings":
        return t("upstreamOnboarding.confirm.actionInsertRouteBindings");
      case "grant tenant_products":
        return t("upstreamOnboarding.confirm.actionGrantTenantProducts");
      case "skip authorization":
        return t("upstreamOnboarding.confirm.actionSkipAuthorization");
      default:
        return action;
    }
  };

  const buildPayload = (dryRun: boolean): OnboardingRequest => {
    const contractId = form.contract_id.trim();
    return {
      supplier: {
        supplier_code: form.supplier_code.trim(),
        name: form.name.trim(),
        upstream_type: "microservice",
        upstream_url: form.upstream_url.trim(),
        status: "active",
        config: {
          upstream_type: "microservice",
          timeouts: { total_ms: form.timeout_ms },
          ...(contractId ? { contract_id: contractId } : {}),
        },
      },
      channel: {
        channel_code: form.channel_code.trim().toUpperCase(),
        name: form.channel_name.trim() || form.name.trim(),
        path_prefix: form.path_prefix.trim() || "/",
        cost_price: "0",
        qps_limit: 0,
      },
      route: {
        product_ref: { product_id: form.product_id.trim() },
        tenant_id: null,
        priority: form.priority,
        weight: form.weight,
      },
      options: {
        dry_run: dryRun,
        fail_if_route_exists: true,
      },
    };
  };

  const validateStep0 = (): boolean => {
    const next: Record<string, string> = {};
    if (!form.supplier_code.trim()) {
      next.supplier_code = t("upstreamOnboarding.validation.basicRequired");
    }
    if (!form.name.trim()) {
      next.name = t("upstreamOnboarding.validation.basicRequired");
    }
    if (!form.upstream_url.trim()) {
      next.upstream_url = t("upstreamOnboarding.validation.basicRequired");
    }
    if (!form.contract_id.trim()) {
      next.contract_id = t("upstreamOnboarding.validation.contractIdRequired");
    }
    const chCode = form.channel_code.trim().toUpperCase();
    if (!chCode) {
      next.channel_code = t("upstreamOnboarding.validation.channelCode");
    } else if (!CHANNEL_CODE_RE.test(chCode)) {
      next.channel_code = t("upstreamOnboarding.validation.channelCodePattern");
    }
    if (!form.channel_name.trim()) {
      next.channel_name = t("upstreamOnboarding.validation.basicRequired");
    }
    setErrors(next);
    return Object.keys(next).length === 0;
  };

  const handleNextStep = () => {
    if (step === 0) {
      if (!validateStep0()) return;
      if (productOptions.length === 0) {
        void loadProducts();
      }
      setStep(1);
      return;
    }
    if (step === 1) {
      if (!form.product_id.trim()) {
        showErrorToast(t("upstreamOnboarding.validation.productRequired"));
        return;
      }
      setDryRunResult(null);
      setStep(2);
    }
  };

  const handleDryRun = async () => {
    if (submitting) return;
    setSubmitting(true);
    try {
      const res = await dryRunOnboarding(buildPayload(true));
      setDryRunResult(res);
      showSuccessToast(t("upstreamOnboarding.confirm.dryRunOk"));
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        // HTTP layer reports backend error
      }
    } finally {
      setSubmitting(false);
    }
  };

  const handleSubmit = async () => {
    if (submitting || !dryRunResult) return;
    setSubmitting(true);
    try {
      const res = await createOnboarding(buildPayload(false));
      setResult(res);
      setStep(3);
      showSuccessToast(t("upstreamOnboarding.done.title"));
      onSuccess();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        // HTTP layer reports backend error
      }
    } finally {
      setSubmitting(false);
    }
  };

  const stepLabels = [
    t("upstreamOnboarding.steps.basic"),
    t("upstreamOnboarding.steps.route"),
    t("upstreamOnboarding.steps.confirm"),
    t("upstreamOnboarding.steps.done"),
  ];

  return (
    <Drawer
      open={open}
      onClose={submitting ? () => {} : onClose}
      title={t("upstreamOnboarding.mode.microservice")}
      size="lg"
      closeOnBackdrop={false}
      testId="upstream-access-wizard-drawer"
      footer={
        step < 3 ? (
          <>
            <Button
              type="button"
              variant="outline"
              disabled={step === 0 || submitting}
              onClick={() => setStep((s) => (s > 0 ? ((s - 1) as 0 | 1 | 2) : 0))}
            >
              {t("common.actions.prev")}
            </Button>
            {step < 2 ? (
              <Button
                type="button"
                variant="primary"
                onClick={handleNextStep}
                data-testid="wizard-next-btn"
              >
                {t("common.actions.next")}
              </Button>
            ) : (
              <>
                <Button
                  type="button"
                  variant="outline"
                  loading={submitting}
                  onClick={() => void handleDryRun()}
                  data-testid="wizard-dryrun-btn"
                >
                  {t("upstreamOnboarding.actions.dryRun")}
                </Button>
                <Button
                  type="button"
                  variant="primary"
                  loading={submitting}
                  disabled={!dryRunResult}
                  onClick={() => void handleSubmit()}
                  data-testid="wizard-submit-btn"
                >
                  {t("upstreamOnboarding.actions.submit")}
                </Button>
              </>
            )}
          </>
        ) : (
          <Button type="button" variant="primary" onClick={onClose}>
            {t("common.actions.confirm")}
          </Button>
        )
      }
    >
      {/* Top 4-Step Indicator */}
      <ol className="grid grid-cols-4 gap-2 border-b border-border pb-4 mb-2">
        {stepLabels.map((label, idx) => {
          const isCompleted = idx < step;
          const isCurrent = idx === step;
          return (
            <li key={idx} className="flex items-center gap-2 min-w-0">
              <span
                className={`inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-full text-[11px] font-semibold border ${
                  isCompleted
                    ? "border-success-fg bg-success-fg/15 text-success-fg"
                    : isCurrent
                      ? "border-primary bg-primary text-primary-foreground"
                      : "border-border bg-muted text-muted-foreground"
                }`}
              >
                {isCompleted ? <Check className="h-3.5 w-3.5" /> : idx + 1}
              </span>
              <span
                className={`text-xs truncate ${
                  isCurrent
                    ? "font-semibold text-foreground"
                    : isCompleted
                      ? "font-medium text-foreground"
                      : "text-muted-foreground"
                }`}
              >
                {label}
              </span>
            </li>
          );
        })}
      </ol>

      {/* Step 0: Basic Info */}
      {step === 0 && (
        <div className="space-y-4">
          <p className="text-xs text-muted-foreground leading-relaxed">
            {t("upstreamOnboarding.mode.microserviceHint")}
          </p>

          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
            <FormField
              label={t("upstreamOnboarding.fields.supplierCode")}
              htmlFor="wiz-supplier-code"
              required
              error={errors.supplier_code}
            >
              <Input
                id="wiz-supplier-code"
                value={form.supplier_code}
                onChange={(e) =>
                  setForm((p) => ({ ...p, supplier_code: e.target.value }))
                }
                placeholder="PARTNER-ACME"
                className="font-mono-tech"
              />
            </FormField>

            <FormField
              label={t("upstreamOnboarding.fields.name")}
              htmlFor="wiz-name"
              required
              error={errors.name}
            >
              <Input
                id="wiz-name"
                value={form.name}
                onChange={(e) =>
                  setForm((p) => ({ ...p, name: e.target.value }))
                }
                placeholder={t("upstreamOnboarding.placeholders.name")}
              />
            </FormField>

            <FormField
              label={t("upstreamOnboarding.fields.upstreamUrl")}
              htmlFor="wiz-upstream-url"
              required
              error={errors.upstream_url}
              className="sm:col-span-2"
            >
              <Input
                id="wiz-upstream-url"
                value={form.upstream_url}
                onChange={(e) =>
                  setForm((p) => ({ ...p, upstream_url: e.target.value }))
                }
                onBlur={() =>
                  setForm((p) => ({
                    ...p,
                    upstream_url: stripUrlScheme(p.upstream_url),
                  }))
                }
                placeholder={t("upstreamOnboarding.placeholders.upstreamUrl")}
                className="font-mono-tech"
              />
            </FormField>

            <FormField
              label={t("upstreamOnboarding.fields.contractId")}
              htmlFor="wiz-contract-id"
              required
              error={errors.contract_id}
            >
              <Input
                id="wiz-contract-id"
                value={form.contract_id}
                onChange={(e) =>
                  setForm((p) => ({ ...p, contract_id: e.target.value }))
                }
                placeholder={t("upstreamOnboarding.placeholders.contractId")}
                className="font-mono-tech"
              />
            </FormField>

            <FormField label={t("upstreamOnboarding.fields.timeoutMs")}>
              <NumberInput
                value={form.timeout_ms}
                min={1000}
                max={300000}
                step={1000}
                onChange={(val) => setForm((p) => ({ ...p, timeout_ms: val }))}
              />
            </FormField>

            <FormField
              label={t("upstreamOnboarding.fields.channelCode")}
              htmlFor="wiz-channel-code"
              required
              error={errors.channel_code}
            >
              <Input
                id="wiz-channel-code"
                value={form.channel_code}
                onChange={(e) =>
                  setForm((p) => ({ ...p, channel_code: e.target.value }))
                }
                placeholder="ACME-DEFAULT"
                className="font-mono-tech"
              />
            </FormField>

            <FormField
              label={t("upstreamOnboarding.fields.channelName")}
              htmlFor="wiz-channel-name"
              required
              error={errors.channel_name}
            >
              <Input
                id="wiz-channel-name"
                value={form.channel_name}
                onChange={(e) =>
                  setForm((p) => ({ ...p, channel_name: e.target.value }))
                }
                placeholder={t("upstreamOnboarding.placeholders.channelName")}
              />
            </FormField>

            <FormField
              label={t("upstreamOnboarding.fields.pathPrefix")}
              htmlFor="wiz-path-prefix"
              className="sm:col-span-2"
            >
              <Input
                id="wiz-path-prefix"
                value={form.path_prefix}
                onChange={(e) =>
                  setForm((p) => ({ ...p, path_prefix: e.target.value }))
                }
                placeholder="/"
                className="font-mono-tech"
              />
            </FormField>
          </div>
        </div>
      )}

      {/* Step 1: Route Binding */}
      {step === 1 && (
        <div className="space-y-4">
          <FormField
            label={t("upstreamOnboarding.fields.productId")}
            required
          >
            <Select
              value={form.product_id}
              options={productSelectOptions}
              searchable
              placeholder={t("upstreamOnboarding.placeholders.productSelect")}
              onChange={(e) =>
                setForm((p) => ({ ...p, product_id: e.target.value }))
              }
              className="w-full"
            />
          </FormField>

          <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
            <FormField label={t("upstreamOnboarding.fields.weight")}>
              <NumberInput
                value={form.weight}
                min={0}
                max={100}
                step={1}
                onChange={(val) => setForm((p) => ({ ...p, weight: val }))}
              />
            </FormField>

            <FormField label={t("upstreamOnboarding.fields.priority")}>
              <NumberInput
                value={form.priority}
                min={1}
                max={1000}
                step={1}
                onChange={(val) => setForm((p) => ({ ...p, priority: val }))}
              />
            </FormField>
          </div>
        </div>
      )}

      {/* Step 2: Confirm & Dry-Run */}
      {step === 2 && (
        <div className="space-y-4">
          {dryRunResult?.plan ? (
            <>
              <div
                className={`flex items-start gap-2 rounded-md border px-3 py-2 text-xs ${
                  dryRunResult.warnings?.length
                    ? "border-warning-border bg-warning-bg text-warning-fg"
                    : "border-success-fg/35 bg-success-fg/10 text-success-fg"
                }`}
              >
                {dryRunResult.warnings?.length ? (
                  <AlertTriangle className="h-4 w-4 shrink-0 mt-0.5" />
                ) : (
                  <CheckCircle2 className="h-4 w-4 shrink-0 mt-0.5" />
                )}
                <span className="font-medium">
                  {t("upstreamOnboarding.confirm.dryRunOk")}
                </span>
              </div>

              {dryRunResult.warnings && dryRunResult.warnings.length > 0 && (
                <div className="rounded-md border border-warning-border bg-warning-bg px-3 py-2 text-xs text-warning-fg space-y-1">
                  <div className="font-semibold">
                    {t("upstreamOnboarding.confirm.warningsTitle")}
                  </div>
                  <ul className="list-disc pl-4 space-y-0.5">
                    {dryRunResult.warnings.map((w, idx) => (
                      <li key={idx}>{w}</li>
                    ))}
                  </ul>
                </div>
              )}

              <div className="space-y-2">
                <div className="text-xs font-semibold text-foreground">
                  {t("upstreamOnboarding.confirm.summaryTitle")}
                </div>
                <dl className="border border-border rounded-lg overflow-hidden divide-y divide-border">
                  <div className="grid grid-cols-[140px_1fr]">
                    <dt className="bg-card px-3 py-2 text-muted-foreground border-r border-border">
                      {t("upstreamOnboarding.fields.supplierCode")}
                    </dt>
                    <dd className="px-3 py-2">
                      {form.name.trim()} ({dryRunResult.plan.supplier_code})
                    </dd>
                  </div>
                  <div className="grid grid-cols-[140px_1fr]">
                    <dt className="bg-card px-3 py-2 text-muted-foreground border-r border-border">
                      {t("upstreamOnboarding.confirm.upstreamType")}
                    </dt>
                    <dd className="px-3 py-2 font-mono-tech">
                      {dryRunResult.plan.upstream_type === "microservice"
                        ? t("supplier.upstreamOptions.microservice")
                        : t("supplier.upstreamOptions.transparent")}{" "}
                      · {dryRunResult.plan.upstream_url}
                    </dd>
                  </div>
                  <div className="grid grid-cols-[140px_1fr]">
                    <dt className="bg-card px-3 py-2 text-muted-foreground border-r border-border">
                      {t("upstreamOnboarding.fields.channelCode")}
                    </dt>
                    <dd className="px-3 py-2">
                      {(form.channel_name || form.name).trim()} (
                      {dryRunResult.plan.channel_code}) ·{" "}
                      {form.path_prefix.trim() || "/"}
                    </dd>
                  </div>
                  <div className="grid grid-cols-[140px_1fr]">
                    <dt className="bg-card px-3 py-2 text-muted-foreground border-r border-border">
                      {t("upstreamOnboarding.confirm.targetProduct")}
                    </dt>
                    <dd className="px-3 py-2">{selectedProductSummary}</dd>
                  </div>
                  <div className="grid grid-cols-[140px_1fr]">
                    <dt className="bg-card px-3 py-2 text-muted-foreground border-r border-border">
                      {t("upstreamOnboarding.fields.priority")} /{" "}
                      {t("upstreamOnboarding.fields.weight")}
                    </dt>
                    <dd className="px-3 py-2 font-mono-tech tabular-nums">
                      {form.priority} / {form.weight}
                    </dd>
                  </div>
                  <div className="grid grid-cols-[140px_1fr]">
                    <dt className="bg-card px-3 py-2 text-muted-foreground border-r border-border">
                      {t("upstreamOnboarding.confirm.plannedActions")}
                    </dt>
                    <dd className="px-3 py-2 flex flex-wrap gap-1.5">
                      {dryRunResult.plan.actions.map((action) => (
                        <Badge
                          key={action}
                          variant={
                            action.startsWith("skip") ? "neutral" : "xblue"
                          }
                        >
                          {formatPlanAction(action)}
                        </Badge>
                      ))}
                    </dd>
                  </div>
                </dl>
              </div>
            </>
          ) : (
            <EmptyState
              title={t("upstreamOnboarding.confirm.needDryRun")}
              action={
                <Button
                  type="button"
                  variant="primary"
                  size="sm"
                  loading={submitting}
                  onClick={() => void handleDryRun()}
                >
                  {t("upstreamOnboarding.actions.dryRun")}
                </Button>
              }
            />
          )}
        </div>
      )}

      {/* Step 3: Done */}
      {step === 3 && (
        <div className="space-y-4">
          <div className="flex flex-col items-center justify-center gap-2 rounded-lg border border-success-fg/30 bg-success-fg/10 px-4 py-6 text-center">
            <CheckCircle2 className="h-9 w-9 text-success-fg" />
            <div className="text-sm font-semibold text-foreground">
              {t("upstreamOnboarding.done.title")}
            </div>
            <div className="text-xs text-muted-foreground">
              {t("upstreamOnboarding.done.subtitle", {
                version: result?.route_version ?? EMPTY_PLACEHOLDER,
              })}
            </div>
          </div>

          {result && (
            <dl className="border border-border rounded-lg overflow-hidden divide-y divide-border">
              <div className="grid grid-cols-[140px_1fr]">
                <dt className="bg-card px-3 py-2 text-muted-foreground border-r border-border">
                  {t("upstreamOnboarding.done.supplierId")}
                </dt>
                <dd className="px-3 py-2">
                  {result.supplier_id ? (
                    <CopyableText value={result.supplier_id} />
                  ) : (
                    EMPTY_PLACEHOLDER
                  )}
                </dd>
              </div>
              <div className="grid grid-cols-[140px_1fr]">
                <dt className="bg-card px-3 py-2 text-muted-foreground border-r border-border">
                  {t("upstreamOnboarding.done.channelId")}
                </dt>
                <dd className="px-3 py-2">
                  {result.channel_id ? (
                    <CopyableText value={result.channel_id} />
                  ) : (
                    EMPTY_PLACEHOLDER
                  )}
                </dd>
              </div>
              <div className="grid grid-cols-[140px_1fr]">
                <dt className="bg-card px-3 py-2 text-muted-foreground border-r border-border">
                  {t("upstreamOnboarding.done.routeRuleId")}
                </dt>
                <dd className="px-3 py-2">
                  {result.route_rule_id ? (
                    <CopyableText value={result.route_rule_id} />
                  ) : (
                    EMPTY_PLACEHOLDER
                  )}
                </dd>
              </div>
              <div className="grid grid-cols-[140px_1fr]">
                <dt className="bg-card px-3 py-2 text-muted-foreground border-r border-border">
                  {t("upstreamOnboarding.done.bindingId")}
                </dt>
                <dd className="px-3 py-2">
                  {result.binding_id ? (
                    <CopyableText value={result.binding_id} />
                  ) : (
                    EMPTY_PLACEHOLDER
                  )}
                </dd>
              </div>
              <div className="grid grid-cols-[140px_1fr]">
                <dt className="bg-card px-3 py-2 text-muted-foreground border-r border-border">
                  {t("upstreamOnboarding.done.routeVersion")}
                </dt>
                <dd className="px-3 py-2 font-mono-tech tabular-nums">
                  {result.route_version ?? EMPTY_PLACEHOLDER}
                </dd>
              </div>
            </dl>
          )}

          <p className="text-xs text-muted-foreground">
            {t("upstreamOnboarding.done.reloadHint")}
          </p>
        </div>
      )}
    </Drawer>
  );
}
