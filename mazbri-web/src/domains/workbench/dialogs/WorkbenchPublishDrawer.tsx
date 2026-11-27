import React, { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router";
import {
  AlertCircle,
  AlertTriangle,
  Check,
  CheckCircle2,
  Copy,
  X,
} from "lucide-react";
import { fetchProductList } from "@/api/product";
import type { Product } from "@/types/api/product";
import { useT } from "@/locales";
import { copyWithFeedback } from "@/utils/ui/copy-feedback";
import {
  Badge,
  Button,
  Drawer,
  FormDrawer,
  FormField,
  Input,
  Select,
  Switch,
  Textarea,
  type SelectOptionItem,
} from "@/components/ui";
import {
  listMissingAdapterFields,
  type ChannelDraft,
  type MissingAdapterField,
} from "../engine/workbenchHelpers";
import {
  selectCanWriteWorkbench,
  selectGateView,
  selectLeftoverTokens,
  selectPublishResultDesc,
  selectUnresolvedAll,
  useWorkbenchStore,
  type PublishMode,
} from "../store";

type MissingRow = {
  id: string;
  name: string;
  fields: MissingAdapterField[];
};

function WorkbenchPublishDrawerBody() {
  const { t } = useT();
  const navigate = useNavigate();
  const state = useWorkbenchStore();

  const publishing = state.publishing;
  const form = state.publishForm;
  const channels = state.publishChannels;
  const result = state.publishResult;
  const endpoints = state.endpoints;
  const gate = useMemo(() => selectGateView(state), [state]);
  const canForcePublish = useMemo(() => selectCanWriteWorkbench(state), [state]);
  const leftoverTokens = useMemo(() => selectLeftoverTokens(state), [state]);
  const unresolvedVars = useMemo(() => selectUnresolvedAll(state), [state]);
  const resultDesc = useMemo(() => selectPublishResultDesc(state), [state]);

  const [products, setProducts] = useState<Product[]>([]);
  const [productLoading, setProductLoading] = useState(false);

  const gateAllMet = gate.total === 0 || gate.ok === gate.total;
  const showForce = canForcePublish && form.require_gate && !gateAllMet;

  useEffect(() => {
    if (!form.require_gate && (form.force_publish || form.force_reason)) {
      state.updatePublishForm({ force_publish: false, force_reason: "" });
    }
  }, [form.require_gate, form.force_publish, form.force_reason, state]);

  useEffect(() => {
    if (!showForce && (form.force_publish || form.force_reason)) {
      state.updatePublishForm({ force_publish: false, force_reason: "" });
    }
  }, [showForce, form.force_publish, form.force_reason, state]);

  useEffect(() => {
    if (form.mode !== "full_onboarding" || products.length > 0) return;
    let cancelled = false;
    setProductLoading(true);
    void fetchProductList({ page_size: 50 })
      .then((res) => {
        if (!cancelled) {
          setProducts(res.products || []);
        }
      })
      .finally(() => {
        if (!cancelled) setProductLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [form.mode, products.length]);

  const hasJobPollEndpoint = useMemo(
    () => endpoints.some((ep) => (ep.adapter || "http_json") === "job_poll"),
    [endpoints],
  );

  const missingAdapterSummary = useMemo<MissingRow[]>(() => {
    const out: MissingRow[] = [];
    endpoints.forEach((ep, i) => {
      if (ep.gate === false) return;
      const fields = listMissingAdapterFields(ep);
      if (!fields.length) return;
      const id = ep.id || `ep-${i + 1}`;
      out.push({
        id,
        name: ep.name || ep.path || id,
        fields,
      });
    });
    return out;
  }, [endpoints]);

  const modeOptions = useMemo<SelectOptionItem[]>(
    () => [
      {
        value: "supplier_only",
        label: t("upstreamConnector.modeSupplierOnly"),
        text: t("upstreamConnector.modeSupplierOnly"),
      },
      {
        value: "supplier_channels",
        label: t("upstreamConnector.modeSupplierChannels"),
        text: t("upstreamConnector.modeSupplierChannels"),
      },
      {
        value: "full_onboarding",
        label: t("upstreamConnector.modeFull"),
        text: t("upstreamConnector.modeFull"),
      },
    ],
    [t],
  );

  const productOptions = useMemo<SelectOptionItem[]>(
    () =>
      products.map((p) => {
        const text = `${p.name} (${p.product_id})`;
        return {
          value: p.product_id,
          label: text,
          text,
        };
      }),
    [products],
  );

  const gateReasonText = (reason?: string): string => {
    if (reason === "spec_changed")
      return t("upstreamConnector.gateReasonSpecChanged");
    if (reason === "unresolved_vars")
      return t("upstreamConnector.gateReasonUnresolved");
    if (reason === "not_run") return t("upstreamConnector.gateReasonNotRun");
    if (reason === "spec_invalid")
      return t("upstreamConnector.gateReasonSpecInvalid");
    return "";
  };

  const updateChannel = (index: number, patch: Partial<ChannelDraft>) => {
    const next = channels.map((row, idx) =>
      idx === index ? { ...row, ...patch } : row,
    );
    state.setPublishChannels(next);
  };

  const closeDrawer = () => {
    state.setShowPublish(false);
  };

  if (result) {
    return (
      <Drawer
        open
        onClose={closeDrawer}
        title={t("upstreamConnector.publish")}
        size="lg"
        footer={
          <Button type="button" variant="primary" onClick={closeDrawer}>
            {t("common.actions.close")}
          </Button>
        }
      >
        <div className="space-y-4">
          <div className="flex items-start gap-2.5 rounded-lg border border-success-fg/35 bg-success-fg/10 p-3">
            <CheckCircle2 className="h-4 w-4 text-success-fg shrink-0 mt-0.5" />
            <div className="min-w-0 space-y-0.5">
              <div className="text-xs font-semibold text-success-fg">
                {t("upstreamConnector.publishOk")}
              </div>
              {resultDesc && (
                <div className="font-mono-tech text-[11px] text-foreground break-all">
                  {resultDesc}
                </div>
              )}
            </div>
          </div>

          <div className="grid grid-cols-2 gap-3">
            <div className="rounded-lg border border-border bg-muted/25 p-2.5 space-y-0.5">
              <div className="text-[11px] text-muted-foreground">
                {t("upstreamConnector.fields.supplierCode")}
              </div>
              <div className="font-mono-tech text-xs font-medium text-foreground">
                {form.supplier_code || "—"}
              </div>
            </div>
            <div className="rounded-lg border border-border bg-muted/25 p-2.5 space-y-0.5">
              <div className="text-[11px] text-muted-foreground">
                {t("upstreamOnboarding.done.supplierId")}
              </div>
              <div className="font-mono-tech text-xs font-medium text-foreground truncate">
                {result.supplier_id}
              </div>
            </div>
            <div className="rounded-lg border border-border bg-muted/25 p-2.5 space-y-0.5">
              <div className="text-[11px] text-muted-foreground">
                {t("upstreamConnector.fields.versionNo")}
              </div>
              <div className="font-mono-tech text-xs font-medium text-foreground">
                v{result.version_no}
              </div>
            </div>
            <div className="rounded-lg border border-border bg-muted/25 p-2.5 space-y-0.5">
              <div className="text-[11px] text-muted-foreground">
                {t("upstreamOnboarding.done.routeVersion")}
              </div>
              <div className="font-mono-tech text-xs font-medium text-foreground">
                {result.route_version ?? "—"}
              </div>
            </div>
          </div>

          {Array.isArray(result.warnings) && result.warnings.length > 0 && (
            <div className="rounded-lg border border-warning-border bg-warning-bg p-3 space-y-1.5">
              {result.warnings.map((w, idx) => (
                <div
                  key={idx}
                  className="flex items-start gap-1.5 text-xs text-warning-fg"
                >
                  <AlertTriangle className="h-3.5 w-3.5 shrink-0 mt-0.5" />
                  <span>{w}</span>
                </div>
              ))}
            </div>
          )}

          {result.sample_curl && (
            <div className="space-y-1.5">
              <div className="flex items-center justify-between gap-2">
                <span className="text-xs font-semibold text-foreground">
                  {t("upstreamConnector.sampleCurl")}
                </span>
                <Button
                  type="button"
                  variant="outline"
                  size="xs"
                  onClick={() => void copyWithFeedback(result.sample_curl || "")}
                >
                  <Copy className="h-3 w-3" />
                  {t("common.actions.copy")}
                </Button>
              </div>
              <pre className="m-0 rounded-lg border border-border bg-muted/45 p-3 font-mono-tech text-[11px] leading-relaxed text-foreground whitespace-pre-wrap break-all">
                {result.sample_curl}
              </pre>
            </div>
          )}
        </div>
      </Drawer>
    );
  }

  const submitBlocked =
    unresolvedVars.length > 0 || missingAdapterSummary.length > 0;
  const gatedItems = gate.items.filter((g) => g.gated);

  return (
    <FormDrawer
      open
      onClose={closeDrawer}
      onSubmit={() => state.doPublish(navigate)}
      title={t("upstreamConnector.publish")}
      size="lg"
      submitLabel={t("upstreamConnector.publish")}
      cancelLabel={t("common.cancel")}
      submitting={publishing}
      submitDisabled={submitBlocked}
    >
      {/* Pre-flight alerts */}
      {unresolvedVars.length > 0 && (
        <div className="flex items-start gap-2 rounded-lg border border-danger-fg/35 bg-danger-fg/10 p-3 text-xs text-danger-fg">
          <AlertCircle className="h-4 w-4 shrink-0 mt-0.5" />
          <span>
            {t("upstreamConnector.unresolvedVarsBlock", {
              vars: unresolvedVars.join(", "),
            })}
          </span>
        </div>
      )}

      {missingAdapterSummary.length > 0 && (
        <div className="rounded-lg border border-danger-fg/35 bg-danger-fg/10 p-3 space-y-1.5 text-xs text-danger-fg">
          <div className="flex items-center gap-2 font-medium">
            <AlertCircle className="h-4 w-4 shrink-0" />
            <span>{t("upstreamConnector.adapterFieldsMissingPublish")}</span>
          </div>
          <ul className="list-disc pl-6 space-y-1">
            {missingAdapterSummary.map((row) => (
              <li key={row.id}>
                <strong className="font-semibold">{row.name}</strong>:
                {row.fields.map((f) => (
                  <code
                    key={f.path}
                    className="ml-1.5 rounded-md bg-card/80 px-1.5 py-0.5 font-mono-tech text-[11px] text-foreground"
                  >
                    {f.path}
                  </code>
                ))}
              </li>
            ))}
          </ul>
        </div>
      )}

      {leftoverTokens.length > 0 && (
        <div className="flex items-start gap-2 rounded-lg border border-warning-border bg-warning-bg p-3 text-xs text-warning-fg">
          <AlertTriangle className="h-4 w-4 shrink-0 mt-0.5" />
          <span>
            {t("upstreamConnector.staticTokenHint", {
              headers: leftoverTokens.join(", "),
            })}
          </span>
        </div>
      )}

      {hasJobPollEndpoint && (
        <div className="flex items-start gap-2 rounded-lg border border-warning-border bg-warning-bg p-3 text-xs text-warning-fg">
          <AlertTriangle className="h-4 w-4 shrink-0 mt-0.5" />
          <span>{t("upstreamConnector.jobPollRiskPublish")}</span>
        </div>
      )}

      {/* Core publish form fields */}
      <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
        <FormField
          label={t("upstreamConnector.publishMode")}
          className="sm:col-span-2"
        >
          <Select
            value={form.mode}
            options={modeOptions}
            aria-label={t("upstreamConnector.publishMode")}
            className="w-full"
            onChange={(e) =>
              state.updatePublishForm({ mode: e.target.value as PublishMode })
            }
          />
        </FormField>

        <FormField
          label={t("upstreamConnector.fields.supplierCode")}
          required
        >
          <Input
            value={form.supplier_code}
            placeholder="SUP_ACME"
            aria-label={t("upstreamConnector.fields.supplierCode")}
            onChange={(e) =>
              state.updatePublishForm({ supplier_code: e.target.value })
            }
          />
        </FormField>

        <FormField label={t("upstreamConnector.fields.name")} required>
          <Input
            value={form.supplier_name}
            placeholder="Acme API"
            aria-label={t("upstreamConnector.fields.name")}
            onChange={(e) =>
              state.updatePublishForm({ supplier_name: e.target.value })
            }
          />
        </FormField>

        {form.mode === "full_onboarding" && (
          <FormField
            label={t("upstreamConnector.fields.product")}
            required
            className="sm:col-span-2"
          >
            <Select
              value={form.product_id}
              options={productOptions}
              searchable
              disabled={productLoading}
              placeholder={t("upstreamConnector.productPlaceholder")}
              aria-label={t("upstreamConnector.fields.product")}
              className="w-full"
              onChange={(e) =>
                state.updatePublishForm({ product_id: e.target.value })
              }
            />
          </FormField>
        )}
      </div>

      {/* Editable Channels Table */}
      {form.mode !== "supplier_only" && (
        <section className="space-y-2">
          <div className="text-xs font-semibold text-foreground">
            {t("upstreamConnector.channelMapping")}
          </div>
          <div className="rounded-lg border border-border overflow-x-auto">
            <table className="w-full text-xs border-collapse">
              <thead>
                <tr className="border-b border-border bg-muted/45 text-left text-[11px] text-muted-foreground">
                  <th className="px-2.5 py-2 font-medium">
                    {t("upstreamConnector.fields.epName")}
                  </th>
                  <th className="px-2 py-2 font-medium">
                    {t("upstreamConnector.fields.channelCode")}
                  </th>
                  <th className="px-2 py-2 font-medium">
                    {t("upstreamConnector.fields.pathPrefix")}
                  </th>
                  <th className="px-2 py-2 font-medium w-24">
                    {t("channels.fields.costPrice")}
                  </th>
                  <th className="px-2 py-2 font-medium w-20">
                    {t("listFilters.fields.qpsLimit")}
                  </th>
                </tr>
              </thead>
              <tbody className="divide-y divide-border-subtle">
                {channels.map((row, idx) => (
                  <tr key={row.endpoint_id || idx}>
                    <td className="px-2.5 py-1.5 font-medium text-foreground max-w-[140px] truncate">
                      {row.name}
                    </td>
                    <td className="px-2 py-1.5">
                      <Input
                        value={row.channel_code}
                        aria-label={t("upstreamConnector.fields.channelCode")}
                        className="h-7 font-mono-tech"
                        onChange={(e) =>
                          updateChannel(idx, { channel_code: e.target.value })
                        }
                      />
                    </td>
                    <td className="px-2 py-1.5">
                      <Input
                        value={row.path_prefix}
                        aria-label={t("upstreamConnector.fields.pathPrefix")}
                        className="h-7 font-mono-tech"
                        onChange={(e) =>
                          updateChannel(idx, { path_prefix: e.target.value })
                        }
                      />
                    </td>
                    <td className="px-2 py-1.5">
                      <Input
                        value={row.cost_price ?? ""}
                        placeholder={t("channels.placeholders.costPrice")}
                        aria-label={t("channels.fields.costPrice")}
                        className="h-7 font-mono-tech"
                        onChange={(e) =>
                          updateChannel(idx, {
                            cost_price: e.target.value || undefined,
                          })
                        }
                      />
                    </td>
                    <td className="px-2 py-1.5">
                      <Input
                        type="number"
                        min={0}
                        value={row.qps_limit}
                        aria-label={t("listFilters.fields.qpsLimit")}
                        className="h-7 font-mono-tech tabular-nums"
                        onChange={(e) => {
                          const n = Number(e.target.value);
                          updateChannel(idx, {
                            qps_limit: Number.isFinite(n) && n >= 0 ? Math.floor(n) : 0,
                          });
                        }}
                      />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </section>
      )}

      {/* Gate Checklist */}
      <section className="rounded-lg border border-border bg-muted/20 p-3 space-y-2">
        <div className="flex items-center justify-between gap-2">
          <span className="text-xs font-semibold text-foreground">
            {t("upstreamConnector.gateChecklist")}
          </span>
          <Badge
            variant={gateAllMet ? "success" : "warning"}
            className="font-mono-tech tabular-nums"
          >
            {gate.ok}/{gate.total}
          </Badge>
        </div>

        {gatedItems.length > 0 && (
          <ul className="space-y-1.5">
            {gatedItems.map((g) => (
              <li
                key={g.id}
                className="flex items-center gap-2 text-xs text-foreground"
              >
                {g.ok ? (
                  <Check className="h-3.5 w-3.5 text-success-fg shrink-0" />
                ) : (
                  <X className="h-3.5 w-3.5 text-danger-fg shrink-0" />
                )}
                <span className="truncate font-medium">{g.name}</span>
                {!g.ok && g.reason && (
                  <span className="text-[11px] text-muted-foreground ml-auto shrink-0">
                    {gateReasonText(g.reason)}
                  </span>
                )}
              </li>
            ))}
          </ul>
        )}
      </section>

      {/* Release comment & gate options */}
      <FormField label={t("upstreamConnector.comment")}>
        <Input
          value={form.comment}
          placeholder="v1.0.0"
          aria-label={t("upstreamConnector.comment")}
          onChange={(e) => state.updatePublishForm({ comment: e.target.value })}
        />
      </FormField>

      <div className="space-y-3 rounded-lg border border-border bg-card p-3">
        <div className="flex items-center justify-between gap-3">
          <span className="text-xs font-medium text-foreground">
            {t("upstreamConnector.requireGate")}
          </span>
          <Switch
            checked={form.require_gate}
            aria-label={t("upstreamConnector.requireGate")}
            onCheckedChange={(checked) =>
              state.updatePublishForm({ require_gate: checked })
            }
          />
        </div>

        {showForce && (
          <>
            <div className="flex items-center justify-between gap-3 border-t border-border-subtle pt-3">
              <span className="text-xs font-medium text-warning-fg">
                {t("upstreamConnector.forcePublish")}
              </span>
              <Switch
                checked={form.force_publish}
                aria-label={t("upstreamConnector.forcePublish")}
                onCheckedChange={(checked) =>
                  state.updatePublishForm({ force_publish: checked })
                }
              />
            </div>

            {form.force_publish && (
              <FormField label={t("upstreamConnector.forceReason")}>
                <Textarea
                  value={form.force_reason}
                  rows={3}
                  aria-label={t("upstreamConnector.forceReason")}
                  onChange={(e) =>
                    state.updatePublishForm({ force_reason: e.target.value })
                  }
                />
              </FormField>
            )}
          </>
        )}
      </div>
    </FormDrawer>
  );
}

export function WorkbenchPublishDrawer() {
  const open = useWorkbenchStore((s) => s.showPublish);
  if (!open) return null;
  return <WorkbenchPublishDrawerBody />;
}
