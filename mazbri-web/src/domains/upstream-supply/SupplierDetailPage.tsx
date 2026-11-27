import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router";
import {
  Info,
  Loader2,
  Pencil,
  Plug,
  Plus,
  Trash2,
} from "lucide-react";
import {
  deleteSupplier,
  fetchSupplierById,
  updateSupplier,
  type Supplier,
} from "@/api/supplier";
import {
  Badge,
  Button,
  ConfirmDialog,
  CopyableText,
  EntityDetailCard,
  EntityDetailLayout,
  IconButton,
  Input,
  PasswordInput,
  Select,
  Switch,
  Textarea,
} from "@/components/ui";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import { EMPTY_PLACEHOLDER, formatDateTime } from "@/utils/format";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { isBizRole } from "@/utils/ops/roles";
import {
  canonicalOrigin,
  clearMaskedKvEntries,
  hasOriginChanged,
  isMaskedSecretValue,
} from "@/utils/originPinning";
import { formatUrlDisplay, stripUrlScheme } from "@/utils/sensitive";
import { logger } from "@/utils/sys/logger";
import { showErrorToast, showSuccessToast } from "@/utils/ui/message-guard";
import { InjectPreviewCard } from "./components/InjectPreviewCard";
import { JsonPathHelper } from "./components/JsonPathHelper";
import {
  buildSupplierUpdatePayload,
  createEmptySupplierForm,
  resolveUpstreamType,
  supplierToForm,
  type SupplierFormModel,
} from "./supplier-form";

export function SupplierDetailPage() {
  const { t } = useT();
  const navigate = useNavigate();
  const { id } = useParams<{ id: string }>();
  const [searchParams, setSearchParams] = useSearchParams();
  const role = useAuthStore((s) => s.info.role);
  const canWrite = isBizRole(role);

  const supplierId = (id ?? "").trim();

  const [loading, setLoading] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [editing, setEditing] = useState(false);
  const [supplier, setSupplier] = useState<Supplier | null>(null);
  const [form, setForm] = useState<SupplierFormModel>(() =>
    createEmptySupplierForm(),
  );
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [deleting, setDeleting] = useState(false);

  const initialUpstreamUrlRef = useRef("");
  const initialTokenUrlRef = useRef("");
  const [originClearedKeys, setOriginClearedKeys] = useState<Set<string>>(
    () => new Set(),
  );

  const captureOriginBaseline = useCallback((model: SupplierFormModel) => {
    initialUpstreamUrlRef.current = model.upstream_url;
    initialTokenUrlRef.current = model.config.auth_flow.token_url;
    setOriginClearedKeys(new Set());
  }, []);

  const loadData = useCallback(async () => {
    if (!supplierId) return;
    setLoading(true);
    try {
      const data = await fetchSupplierById(supplierId);
      setSupplier(data);
      const nextForm = supplierToForm(data);
      setForm(nextForm);
      captureOriginBaseline(nextForm);
    } catch (error) {
      logger.error("Failed to load supplier:", error);
      showErrorToast(t("supplier.detail.loadFailed"));
    } finally {
      setLoading(false);
    }
  }, [supplierId, captureOriginBaseline, t]);

  useEffect(() => {
    void loadData();
  }, [loadData]);

  useEffect(() => {
    if (searchParams.has("edit") && supplier && !editing && canWrite) {
      const nextForm = supplierToForm(supplier);
      setForm(nextForm);
      captureOriginBaseline(nextForm);
      setEditing(true);
    }
  }, [searchParams, supplier, editing, canWrite, captureOriginBaseline]);

  // Origin pinning: clear masked credentials if upstream_url or token_url origin changed
  useEffect(() => {
    if (!editing) return;
    const scheme = form.upstream_type === "transparent" ? "https" : "http";
    const baseOrigin = canonicalOrigin(initialUpstreamUrlRef.current, {
      defaultScheme: scheme,
    });
    const upstreamChanged = hasOriginChanged(
      initialUpstreamUrlRef.current,
      form.upstream_url,
      { defaultScheme: scheme },
    );
    const tokenChanged = hasOriginChanged(
      initialTokenUrlRef.current,
      form.config.auth_flow.token_url,
      {
        defaultScheme: "https",
        fallbackInitialOrigin: baseOrigin || undefined,
      },
    );
    if (!upstreamChanged && !tokenChanged) return;

    const credsCopy = form.config.auth_flow.credentials.map((c) => ({ ...c }));
    const headersCopy = form.config.auth_flow.headers.map((h) => ({ ...h }));
    const cleared = clearMaskedKvEntries(credsCopy);
    const clearedHeaders = clearMaskedKvEntries(headersCopy);
    if (cleared.length === 0 && clearedHeaders.length === 0) return;

    setForm((prev) => ({
      ...prev,
      config: {
        ...prev.config,
        auth_flow: {
          ...prev.config.auth_flow,
          credentials: credsCopy,
          headers: headersCopy,
        },
      },
    }));
    if (cleared.length > 0) {
      setOriginClearedKeys((prev) => {
        const next = new Set(prev);
        for (const k of cleared) next.add(k);
        return next;
      });
    }
  }, [
    editing,
    form.upstream_url,
    form.config.auth_flow.token_url,
    form.upstream_type,
  ]);

  const upstreamType = editing
    ? form.upstream_type
    : supplier
      ? resolveUpstreamType(supplier)
      : "microservice";

  const upstreamTypeLabel =
    upstreamType === "transparent"
      ? t("supplier.upstreamOptions.transparent")
      : t("supplier.upstreamOptions.microservice");

  const statusValue = editing ? form.status : supplier?.status || "";
  const statusLabel = useMemo(() => {
    if (statusValue === "active") return t("common.status.active");
    if (statusValue === "inactive") return t("common.status.disabled");
    if (statusValue === "suspended") return t("common.status.suspended");
    return statusValue || EMPTY_PLACEHOLDER;
  }, [statusValue, t]);

  const statusBadgeKey =
    statusValue === "active"
      ? "ACTIVE"
      : statusValue === "suspended"
        ? "SUSPENDED"
        : "DISABLED";

  const contractId =
    (supplier?.config as { contract_id?: string } | undefined)?.contract_id ||
    "";
  const authFlow = supplier?.config?.auth_flow;
  const credentialEntries = useMemo<[string, string][]>(() => {
    const creds = authFlow?.credentials;
    if (!creds || typeof creds !== "object") return [];
    return Object.entries(creds).map(([k, v]) => [k, String(v)]);
  }, [authFlow]);

  const hasActiveHttpAuth = useMemo(() => {
    if (!authFlow?.enabled) return false;
    const p = String(authFlow.profile || "").trim();
    if (p && p !== "none" && p !== "skip") return true;
    return Boolean(authFlow.token_url?.trim());
  }, [authFlow]);

  const isVaultOnly = !hasActiveHttpAuth && credentialEntries.length > 0;

  const enterEdit = () => {
    if (!supplier) return;
    const nextForm = supplierToForm(supplier);
    setForm(nextForm);
    captureOriginBaseline(nextForm);
    setErrors({});
    setEditing(true);
  };

  const cancelEdit = () => {
    if (supplier) {
      const nextForm = supplierToForm(supplier);
      setForm(nextForm);
      captureOriginBaseline(nextForm);
    }
    setErrors({});
    setEditing(false);
    if (searchParams.has("edit")) {
      const next = new URLSearchParams(searchParams);
      next.delete("edit");
      setSearchParams(next, { replace: true });
    }
  };

  const handleUrlBlur = () => {
    setForm((prev) => {
      if (prev.upstream_type === "transparent") {
        const trimmed = prev.upstream_url.trim();
        if (
          trimmed &&
          !trimmed.startsWith("http://") &&
          !trimmed.startsWith("https://")
        ) {
          return { ...prev, upstream_url: `https://${trimmed}` };
        }
        return prev;
      }
      return { ...prev, upstream_url: stripUrlScheme(prev.upstream_url) };
    });
  };

  const validate = (): boolean => {
    const next: Record<string, string> = {};
    if (!form.supplier_code.trim()) {
      next.supplier_code = t("supplier.validation.codeRequired");
    }
    if (!form.name.trim()) {
      next.name = t("supplier.validation.nameRequired");
    }
    if (!form.upstream_url.trim()) {
      next.upstream_url = t("supplier.validation.accessAddressRequired");
    }
    if (
      form.upstream_type === "microservice" &&
      !(form.config.contract_id || "").trim()
    ) {
      next.contract_id = t("supplier.validation.contractIdRequired");
      showErrorToast(t("supplier.validation.contractIdRequired"));
    }
    for (const [idx, cred] of form.config.auth_flow.credentials.entries()) {
      const k = cred.key.trim();
      if (k && originClearedKeys.has(k) && !cred.value.trim()) {
        next[`cred_${idx}`] = t(
          "supplier.validation.credentialRequiredOnOriginChange",
        );
      }
    }
    setErrors(next);
    if (Object.keys(next).length > 0) return false;

    if (form.config.auth_flow.enabled) {
      if (!form.config.auth_flow.token_url.trim()) {
        showErrorToast(t("upstreamOnboarding.validation.authTokenUrl"));
        return false;
      }
      if (!form.config.auth_flow.token_jsonpath.trim()) {
        showErrorToast(t("upstreamOnboarding.validation.authTokenPath"));
        return false;
      }
      if (!form.config.auth_flow.inject_key.trim()) {
        showErrorToast(t("upstreamOnboarding.validation.authInjectKey"));
        return false;
      }
    }
    return true;
  };

  const handleSave = async () => {
    if (!validate() || submitting) return;
    setSubmitting(true);
    try {
      const updated = await updateSupplier(
        supplierId,
        buildSupplierUpdatePayload(form),
      );
      setSupplier(updated);
      const nextForm = supplierToForm(updated);
      setForm(nextForm);
      captureOriginBaseline(nextForm);
      setEditing(false);
      showSuccessToast(t("common.feedback.saveSuccess"));
      if (searchParams.has("edit")) {
        const next = new URLSearchParams(searchParams);
        next.delete("edit");
        setSearchParams(next, { replace: true });
      }
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to update supplier:", error);
      }
    } finally {
      setSubmitting(false);
    }
  };

  const handleConfirmDelete = async () => {
    if (!supplier || deleting) return;
    setDeleting(true);
    try {
      await deleteSupplier(supplier.id);
      showSuccessToast(t("common.feedback.deleteSuccess"));
      navigate("/operations/upstream-supply?tab=suppliers");
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to delete supplier:", error);
      }
    } finally {
      setDeleting(false);
    }
  };

  const updateAuthFlow = (
    patch: Partial<SupplierFormModel["config"]["auth_flow"]>,
  ) => {
    setForm((prev) => ({
      ...prev,
      config: {
        ...prev.config,
        auth_flow: { ...prev.config.auth_flow, ...patch },
      },
    }));
  };

  return (
    <>
      <EntityDetailLayout
        title={supplier?.name || supplier?.supplier_code || EMPTY_PLACEHOLDER}
        code={supplier?.supplier_code}
        status={statusBadgeKey}
        statusLabel={statusLabel}
        badges={
          editing ? (
            <Badge variant="warning">{t("supplier.detail.editingBadge")}</Badge>
          ) : undefined
        }
        onBack={() => navigate("/operations/upstream-supply?tab=suppliers")}
        actions={
          canWrite ? (
            editing ? (
              <>
                <Button
                  type="button"
                  variant="outline"
                  size="md"
                  disabled={submitting}
                  onClick={cancelEdit}
                >
                  {t("common.cancel")}
                </Button>
                <Button
                  type="button"
                  variant="primary"
                  size="md"
                  loading={submitting}
                  onClick={() => void handleSave()}
                  data-testid="supplier-detail-save-btn"
                >
                  {t("common.actions.save")}
                </Button>
              </>
            ) : (
              <>
                {upstreamType === "transparent" && (
                  <Button
                    type="button"
                    variant="outline"
                    size="md"
                    onClick={() =>
                      navigate(
                        `/operations/upstream-supply/workbench?supplier_id=${encodeURIComponent(supplierId)}`,
                      )
                    }
                    data-testid="supplier-detail-workbench-btn"
                  >
                    <Plug className="h-3.5 w-3.5" />
                    {t("upstreamConnector.workbench")}
                  </Button>
                )}
                <Button
                  type="button"
                  variant="primary"
                  size="md"
                  disabled={!supplier}
                  onClick={enterEdit}
                  data-testid="supplier-detail-edit-btn"
                >
                  <Pencil className="h-3.5 w-3.5" />
                  {t("common.actions.edit")}
                </Button>
                <Button
                  type="button"
                  variant="danger"
                  size="md"
                  disabled={!supplier}
                  onClick={() => setDeleteOpen(true)}
                  data-testid="supplier-detail-delete-btn"
                >
                  <Trash2 className="h-3.5 w-3.5" />
                  {t("common.actions.delete")}
                </Button>
              </>
            )
          ) : undefined
        }
        testId="supplier-detail-page"
      >
        {/* Basic Info Card */}
        <EntityDetailCard
          title={t("supplier.detail.basicInfoTitle")}
          testId="supplier-basic-info-card"
        >
          {loading && !supplier ? (
            <div className="flex items-center justify-center py-10">
              <Loader2 className="h-5 w-5 animate-spin text-primary" />
            </div>
          ) : (
            <div className="space-y-4">
              {editing && (
                <div className="flex items-start gap-2 rounded-md border border-primary/30 bg-primary-soft/45 px-3 py-2 text-xs text-primary">
                  <Info className="h-4 w-4 shrink-0 mt-0.5" />
                  <span>{t("supplier.form.typeLockedHint")}</span>
                </div>
              )}

              <dl className="grid grid-cols-1 md:grid-cols-2 border border-border rounded-lg overflow-hidden">
                <div className="grid grid-cols-[152px_1fr] border-b border-border">
                  <dt className="bg-card px-3 py-2.5 text-xs font-medium text-muted-foreground border-r border-border">
                    {t("supplier.fields.id")}
                  </dt>
                  <dd className="px-3 py-2.5 text-xs text-foreground min-w-0">
                    {supplier?.id ? (
                      <CopyableText value={supplier.id} truncate />
                    ) : (
                      EMPTY_PLACEHOLDER
                    )}
                  </dd>
                </div>

                <div className="grid grid-cols-[152px_1fr] border-b border-border md:border-l">
                  <dt className="bg-card px-3 py-2.5 text-xs font-medium text-muted-foreground border-r border-border">
                    {t("supplier.fields.upstreamType")}
                  </dt>
                  <dd className="px-3 py-2.5 text-xs">
                    <Badge
                      variant={
                        upstreamType === "transparent" ? "xblue" : "outline"
                      }
                    >
                      {upstreamTypeLabel}
                    </Badge>
                  </dd>
                </div>

                {editing && (
                  <>
                    <div className="grid grid-cols-[152px_1fr] border-b border-border">
                      <dt className="bg-card px-3 py-2.5 text-xs font-medium text-muted-foreground border-r border-border">
                        {t("supplier.fields.code")}
                      </dt>
                      <dd className="px-3 py-2 text-xs text-foreground min-w-0">
                        <div className="space-y-1">
                          <Input
                            value={form.supplier_code}
                            onChange={(e) =>
                              setForm((p) => ({
                                ...p,
                                supplier_code: e.target.value,
                              }))
                            }
                            placeholder={t("supplier.form.codePlaceholder")}
                            className="font-mono-tech"
                          />
                          {errors.supplier_code && (
                            <p className="text-[11px] text-danger-fg">
                              {errors.supplier_code}
                            </p>
                          )}
                        </div>
                      </dd>
                    </div>

                    <div className="grid grid-cols-[152px_1fr] border-b border-border md:border-l">
                      <dt className="bg-card px-3 py-2.5 text-xs font-medium text-muted-foreground border-r border-border">
                        {t("supplier.fields.name")}
                      </dt>
                      <dd className="px-3 py-2 text-xs text-foreground min-w-0">
                        <div className="space-y-1">
                          <Input
                            value={form.name}
                            onChange={(e) =>
                              setForm((p) => ({ ...p, name: e.target.value }))
                            }
                            placeholder={t("supplier.form.namePlaceholder")}
                          />
                          {errors.name && (
                            <p className="text-[11px] text-danger-fg">
                              {errors.name}
                            </p>
                          )}
                        </div>
                      </dd>
                    </div>
                  </>
                )}

                <div className="grid grid-cols-[152px_1fr] border-b border-border md:col-span-2">
                  <dt className="bg-card px-3 py-2.5 text-xs font-medium text-muted-foreground border-r border-border">
                    {upstreamType === "transparent"
                      ? t("supplier.fields.targetAddress")
                      : t("supplier.fields.accessAddress")}
                  </dt>
                  <dd className="px-3 py-2 text-xs text-foreground min-w-0">
                    {editing ? (
                      <div className="space-y-1">
                        <Input
                          value={form.upstream_url}
                          onChange={(e) =>
                            setForm((p) => ({
                              ...p,
                              upstream_url: e.target.value,
                            }))
                          }
                          onBlur={handleUrlBlur}
                          placeholder={
                            upstreamType === "transparent"
                              ? t("supplier.form.targetAddressPlaceholder")
                              : t("supplier.form.accessAddressPlaceholder")
                          }
                          className="font-mono-tech"
                        />
                        {errors.upstream_url && (
                          <p className="text-[11px] text-danger-fg">
                            {errors.upstream_url}
                          </p>
                        )}
                      </div>
                    ) : supplier?.upstream_url ? (
                      <CopyableText
                        value={supplier.upstream_url}
                        displayText={
                          upstreamType === "transparent"
                            ? supplier.upstream_url
                            : formatUrlDisplay(supplier.upstream_url)
                        }
                      />
                    ) : (
                      EMPTY_PLACEHOLDER
                    )}
                  </dd>
                </div>

                {upstreamType !== "transparent" && (
                  <div className="grid grid-cols-[152px_1fr] border-b border-border">
                    <dt className="bg-card px-3 py-2.5 text-xs font-medium text-muted-foreground border-r border-border">
                      {t("supplier.fields.contractId")}
                    </dt>
                    <dd className="px-3 py-2 text-xs text-foreground min-w-0">
                      {editing ? (
                        <div className="space-y-1">
                          <Input
                            value={form.config.contract_id ?? ""}
                            onChange={(e) =>
                              setForm((p) => ({
                                ...p,
                                config: {
                                  ...p.config,
                                  contract_id: e.target.value,
                                },
                              }))
                            }
                            placeholder={t("supplier.placeholders.contractId")}
                            className="font-mono-tech"
                          />
                          {errors.contract_id && (
                            <p className="text-[11px] text-danger-fg">
                              {errors.contract_id}
                            </p>
                          )}
                        </div>
                      ) : (
                        <span className="font-mono-tech">
                          {contractId || EMPTY_PLACEHOLDER}
                        </span>
                      )}
                    </dd>
                  </div>
                )}

                {editing && (
                  <div
                    className={`grid grid-cols-[152px_1fr] border-b border-border ${
                      upstreamType !== "transparent" ? "md:border-l" : "md:col-span-2"
                    }`}
                  >
                    <dt className="bg-card px-3 py-2.5 text-xs font-medium text-muted-foreground border-r border-border">
                      {t("supplier.fields.status")}
                    </dt>
                    <dd className="px-3 py-2 text-xs">
                      <Select
                        value={form.status}
                        onChange={(e) =>
                          setForm((p) => ({ ...p, status: e.target.value }))
                        }
                        className="w-40"
                        options={[
                          {
                            value: "active",
                            label: t("common.status.active"),
                            text: t("common.status.active"),
                          },
                          {
                            value: "inactive",
                            label: t("common.status.disabled"),
                            text: t("common.status.disabled"),
                          },
                          {
                            value: "suspended",
                            label: t("common.status.suspended"),
                            text: t("common.status.suspended"),
                          },
                        ]}
                      />
                    </dd>
                  </div>
                )}

                {!editing && upstreamType !== "transparent" && (
                  <div className="hidden md:block border-b border-border md:border-l bg-card" />
                )}

                <div className="grid grid-cols-[152px_1fr]">
                  <dt className="bg-card px-3 py-2.5 text-xs font-medium text-muted-foreground border-r border-border">
                    {t("common.fields.createdAt")}
                  </dt>
                  <dd className="px-3 py-2.5 text-xs font-mono-tech">
                    {formatDateTime(supplier?.created_at)}
                  </dd>
                </div>

                <div className="grid grid-cols-[152px_1fr] border-t md:border-t-0 border-border md:border-l">
                  <dt className="bg-card px-3 py-2.5 text-xs font-medium text-muted-foreground border-r border-border">
                    {t("common.fields.updatedAt")}
                  </dt>
                  <dd className="px-3 py-2.5 text-xs font-mono-tech">
                    {formatDateTime(supplier?.updated_at)}
                  </dd>
                </div>
              </dl>
            </div>
          )}
        </EntityDetailCard>

        {/* Integration / Auth Config Card (Transparent Upstream) */}
        {upstreamType === "transparent" && (
          <EntityDetailCard
            title={t("supplier.detail.advancedInfoTitle")}
            testId="supplier-integration-card"
          >
            <div className="space-y-4">
              <div className="flex items-center justify-between gap-3 rounded-md border border-border border-l-3 border-l-primary bg-card px-3 py-2">
                <span className="text-xs font-semibold text-foreground">
                  {t("supplier.detail.authSectionTitle")}
                </span>
                <div>
                  {editing ? (
                    <Switch
                      checked={form.config.auth_flow.enabled}
                      onCheckedChange={(checked) =>
                        updateAuthFlow({ enabled: checked })
                      }
                      aria-label={t("supplier.detail.authSectionTitle")}
                    />
                  ) : hasActiveHttpAuth ? (
                    <Badge variant="success">
                      {t("supplier.detail.enabled")}
                    </Badge>
                  ) : isVaultOnly ? (
                    <Badge variant="outline">
                      {t("upstreamConnector.authVaultOnlyTag", {
                        n: credentialEntries.length,
                      })}
                    </Badge>
                  ) : (
                    <Badge variant="neutral">
                      {t("supplier.detail.disabled")}
                    </Badge>
                  )}
                </div>
              </div>

              {(editing || hasActiveHttpAuth || isVaultOnly) && (
                <div className="space-y-4">
                  <dl className="grid grid-cols-1 md:grid-cols-2 border border-border rounded-lg overflow-hidden">
                    {(editing || hasActiveHttpAuth) && (
                      <>
                        <div className="grid grid-cols-[152px_1fr] border-b border-border md:col-span-2">
                          <dt className="bg-card px-3 py-2.5 text-xs font-medium text-muted-foreground border-r border-border">
                            {t("supplier.config.tokenUrl")}
                          </dt>
                          <dd className="px-3 py-2 text-xs min-w-0">
                            {editing ? (
                              <Input
                                value={form.config.auth_flow.token_url}
                                disabled={!form.config.auth_flow.enabled}
                                placeholder="https://api.upstream.com/get/token"
                                onChange={(e) =>
                                  updateAuthFlow({ token_url: e.target.value })
                                }
                                className="font-mono-tech"
                              />
                            ) : authFlow?.token_url ? (
                              <CopyableText value={authFlow.token_url} />
                            ) : (
                              EMPTY_PLACEHOLDER
                            )}
                          </dd>
                        </div>

                        <div className="grid grid-cols-[152px_1fr] border-b border-border md:col-span-2">
                          <dt className="bg-card px-3 py-2.5 text-xs font-medium text-muted-foreground border-r border-border">
                            {t("supplier.config.method")}
                          </dt>
                          <dd className="px-3 py-2 text-xs">
                            {editing ? (
                              <Select
                                value={form.config.auth_flow.method}
                                disabled={!form.config.auth_flow.enabled}
                                onChange={(e) =>
                                  updateAuthFlow({ method: e.target.value })
                                }
                                className="w-32"
                                options={[
                                  { value: "POST", label: "POST", text: "POST" },
                                  { value: "GET", label: "GET", text: "GET" },
                                ]}
                              />
                            ) : (
                              <span className="font-mono-tech">
                                {authFlow?.method || EMPTY_PLACEHOLDER}
                              </span>
                            )}
                          </dd>
                        </div>
                      </>
                    )}

                    {/* Credentials KV */}
                    <div className="grid grid-cols-[152px_1fr] border-b border-border md:col-span-2">
                      <dt className="bg-card px-3 py-2.5 text-xs font-medium text-muted-foreground border-r border-border">
                        {isVaultOnly && !editing
                          ? t("upstreamConnector.authVaultSection")
                          : t("supplier.config.credentials")}
                      </dt>
                      <dd className="px-3 py-2 text-xs min-w-0">
                        {editing ? (
                          <div className="space-y-2">
                            <div className="overflow-x-auto rounded-md border border-border">
                              <table className="w-full border-collapse text-xs">
                                <thead>
                                  <tr className="border-b border-border bg-card text-muted-foreground">
                                    <th className="h-8 px-2.5 text-left font-medium w-2/5">
                                      {t("supplier.config.credKey")}
                                    </th>
                                    <th className="h-8 px-2.5 text-left font-medium">
                                      {t("supplier.config.credValue")}
                                    </th>
                                    <th className="h-8 px-2 text-center font-medium w-14">
                                      {t("common.fields.operations")}
                                    </th>
                                  </tr>
                                </thead>
                                <tbody className="divide-y divide-border-subtle">
                                  {form.config.auth_flow.credentials.length ===
                                  0 ? (
                                    <tr>
                                      <td
                                        colSpan={3}
                                        className="py-3 text-center text-muted-foreground"
                                      >
                                        {EMPTY_PLACEHOLDER}
                                      </td>
                                    </tr>
                                  ) : (
                                    form.config.auth_flow.credentials.map(
                                      (cred, idx) => (
                                        <tr key={idx}>
                                          <td className="p-1.5">
                                            <Input
                                              value={cred.key}
                                              placeholder="key"
                                              onChange={(e) => {
                                                const nextCreds =
                                                  form.config.auth_flow.credentials.map(
                                                    (item, i) =>
                                                      i === idx
                                                        ? {
                                                            ...item,
                                                            key: e.target.value,
                                                          }
                                                        : item,
                                                  );
                                                updateAuthFlow({
                                                  credentials: nextCreds,
                                                });
                                              }}
                                            />
                                          </td>
                                          <td className="p-1.5">
                                            <PasswordInput
                                              value={cred.value}
                                              placeholder="value"
                                              onChange={(e) => {
                                                const nextVal = e.target.value;
                                                const nextCreds =
                                                  form.config.auth_flow.credentials.map(
                                                    (item, i) =>
                                                      i === idx
                                                        ? {
                                                            ...item,
                                                            value: nextVal,
                                                          }
                                                        : item,
                                                  );
                                                updateAuthFlow({
                                                  credentials: nextCreds,
                                                });
                                                const k = cred.key.trim();
                                                if (
                                                  k &&
                                                  nextVal.trim() &&
                                                  !isMaskedSecretValue(
                                                    nextVal,
                                                  ) &&
                                                  originClearedKeys.has(k)
                                                ) {
                                                  setOriginClearedKeys(
                                                    (prev) => {
                                                      const nextSet = new Set(
                                                        prev,
                                                      );
                                                      nextSet.delete(k);
                                                      return nextSet;
                                                    },
                                                  );
                                                }
                                              }}
                                            />
                                            {errors[`cred_${idx}`] && (
                                              <p className="mt-1 text-[11px] text-danger-fg">
                                                {errors[`cred_${idx}`]}
                                              </p>
                                            )}
                                          </td>
                                          <td className="p-1.5 text-center">
                                            <IconButton
                                              size="sm"
                                              aria-label={t(
                                                "common.actions.delete",
                                              )}
                                              onClick={() => {
                                                const removed =
                                                  form.config.auth_flow
                                                    .credentials[idx];
                                                if (
                                                  removed?.key.trim() &&
                                                  originClearedKeys.has(
                                                    removed.key.trim(),
                                                  )
                                                ) {
                                                  setOriginClearedKeys(
                                                    (prev) => {
                                                      const nextSet = new Set(
                                                        prev,
                                                      );
                                                      nextSet.delete(
                                                        removed.key.trim(),
                                                      );
                                                      return nextSet;
                                                    },
                                                  );
                                                }
                                                updateAuthFlow({
                                                  credentials:
                                                    form.config.auth_flow.credentials.filter(
                                                      (_, i) => i !== idx,
                                                    ),
                                                });
                                              }}
                                              className="text-danger-fg hover:text-danger-fg hover:bg-danger-fg/10"
                                            >
                                              <Trash2 className="h-3.5 w-3.5" />
                                            </IconButton>
                                          </td>
                                        </tr>
                                      ),
                                    )
                                  )}
                                </tbody>
                              </table>
                            </div>
                            <Button
                              type="button"
                              variant="link"
                              size="xs"
                              onClick={() =>
                                updateAuthFlow({
                                  credentials: [
                                    ...form.config.auth_flow.credentials,
                                    { key: "", value: "" },
                                  ],
                                })
                              }
                            >
                              <Plus className="h-3.5 w-3.5" />
                              {t("supplier.config.addCredential")}
                            </Button>
                          </div>
                        ) : credentialEntries.length > 0 ? (
                          <div className="flex flex-wrap gap-1.5">
                            {credentialEntries.map(([k, v]) => (
                              <Badge
                                key={k}
                                variant="outline"
                                className="font-mono-tech"
                              >
                                {k}={v}
                              </Badge>
                            ))}
                          </div>
                        ) : (
                          <span className="text-muted-foreground">
                            {t("supplier.detail.noCredentials")}
                          </span>
                        )}
                      </dd>
                    </div>

                    {(editing || hasActiveHttpAuth) && (
                      <>
                        <div className="grid grid-cols-[152px_1fr] border-b border-border md:col-span-2">
                          <dt className="bg-card px-3 py-2.5 text-xs font-medium text-muted-foreground border-r border-border">
                            {t("supplier.config.requestBodyTpl")}
                          </dt>
                          <dd className="px-3 py-2 text-xs min-w-0">
                            {editing ? (
                              <Textarea
                                rows={3}
                                value={form.config.auth_flow.request_body_tpl}
                                disabled={!form.config.auth_flow.enabled}
                                placeholder={t(
                                  "supplier.config.requestBodyTplHint",
                                )}
                                onChange={(e) =>
                                  updateAuthFlow({
                                    request_body_tpl: e.target.value,
                                  })
                                }
                                className="font-mono-tech"
                              />
                            ) : (
                              <pre className="whitespace-pre-wrap break-all font-mono-tech text-xs leading-relaxed">
                                {authFlow?.request_body_tpl || EMPTY_PLACEHOLDER}
                              </pre>
                            )}
                          </dd>
                        </div>

                        <div className="grid grid-cols-[152px_1fr] border-b border-border">
                          <dt className="bg-card px-3 py-2.5 text-xs font-medium text-muted-foreground border-r border-border">
                            {t("supplier.config.tokenJsonpath")}
                          </dt>
                          <dd className="px-3 py-2 text-xs min-w-0">
                            {editing ? (
                              <div className="flex items-center gap-1.5">
                                <Input
                                  value={form.config.auth_flow.token_jsonpath}
                                  disabled={!form.config.auth_flow.enabled}
                                  placeholder="$.token"
                                  onChange={(e) =>
                                    updateAuthFlow({
                                      token_jsonpath: e.target.value,
                                    })
                                  }
                                  className="font-mono-tech"
                                />
                                <JsonPathHelper
                                  disabled={!form.config.auth_flow.enabled}
                                  onSelect={(path) =>
                                    updateAuthFlow({ token_jsonpath: path })
                                  }
                                />
                              </div>
                            ) : (
                              <span className="font-mono-tech">
                                {authFlow?.token_jsonpath || EMPTY_PLACEHOLDER}
                              </span>
                            )}
                          </dd>
                        </div>

                        <div className="grid grid-cols-[152px_1fr] border-b border-border md:border-l">
                          <dt className="bg-card px-3 py-2.5 text-xs font-medium text-muted-foreground border-r border-border">
                            {t("supplier.config.expiryJsonpath")}
                          </dt>
                          <dd className="px-3 py-2 text-xs min-w-0">
                            {editing ? (
                              <div className="flex items-center gap-1.5">
                                <Input
                                  value={form.config.auth_flow.expiry_jsonpath}
                                  disabled={!form.config.auth_flow.enabled}
                                  placeholder="$.expires_in"
                                  onChange={(e) =>
                                    updateAuthFlow({
                                      expiry_jsonpath: e.target.value,
                                    })
                                  }
                                  className="font-mono-tech"
                                />
                                <JsonPathHelper
                                  disabled={!form.config.auth_flow.enabled}
                                  onSelect={(path) =>
                                    updateAuthFlow({ expiry_jsonpath: path })
                                  }
                                />
                              </div>
                            ) : (
                              <span className="font-mono-tech">
                                {authFlow?.expiry_jsonpath || EMPTY_PLACEHOLDER}
                              </span>
                            )}
                          </dd>
                        </div>

                        <div className="grid grid-cols-[152px_1fr]">
                          <dt className="bg-card px-3 py-2.5 text-xs font-medium text-muted-foreground border-r border-border">
                            {t("supplier.config.injectKey")}
                          </dt>
                          <dd className="px-3 py-2 text-xs min-w-0">
                            {editing ? (
                              <div className="flex items-center gap-1.5">
                                <Select
                                  value={form.config.auth_flow.inject_to}
                                  disabled={!form.config.auth_flow.enabled}
                                  onChange={(e) =>
                                    updateAuthFlow({ inject_to: e.target.value })
                                  }
                                  className="w-28 shrink-0"
                                  options={[
                                    {
                                      value: "header",
                                      label: "Header",
                                      text: "Header",
                                    },
                                    {
                                      value: "query",
                                      label: "Query",
                                      text: "Query",
                                    },
                                    {
                                      value: "body",
                                      label: "Body",
                                      text: "Body",
                                    },
                                  ]}
                                />
                                <Input
                                  value={form.config.auth_flow.inject_key}
                                  disabled={!form.config.auth_flow.enabled}
                                  placeholder="Authorization"
                                  onChange={(e) =>
                                    updateAuthFlow({
                                      inject_key: e.target.value,
                                    })
                                  }
                                  className="font-mono-tech"
                                />
                              </div>
                            ) : (
                              <span className="font-mono-tech">
                                {authFlow?.inject_to || EMPTY_PLACEHOLDER} /{" "}
                                {authFlow?.inject_key || EMPTY_PLACEHOLDER}
                              </span>
                            )}
                          </dd>
                        </div>

                        <div className="grid grid-cols-[152px_1fr] border-t md:border-t-0 border-border md:border-l">
                          <dt className="bg-card px-3 py-2.5 text-xs font-medium text-muted-foreground border-r border-border">
                            {t("supplier.config.injectFormat")}
                          </dt>
                          <dd className="px-3 py-2 text-xs min-w-0">
                            {editing ? (
                              <Input
                                value={form.config.auth_flow.inject_value_format}
                                disabled={!form.config.auth_flow.enabled}
                                placeholder="Bearer {}"
                                onChange={(e) =>
                                  updateAuthFlow({
                                    inject_value_format: e.target.value,
                                  })
                                }
                                className="font-mono-tech"
                              />
                            ) : (
                              <span className="font-mono-tech">
                                {authFlow?.inject_value_format ||
                                  EMPTY_PLACEHOLDER}
                              </span>
                            )}
                          </dd>
                        </div>
                      </>
                    )}
                  </dl>

                  {editing && form.config.auth_flow.enabled && (
                    <div className="max-w-2xl space-y-1.5">
                      <div className="text-xs font-medium text-muted-foreground">
                        {t("supplier.config.injectPreview")}
                      </div>
                      <InjectPreviewCard
                        compact
                        injectTo={form.config.auth_flow.inject_to}
                        injectKey={form.config.auth_flow.inject_key}
                        injectValueFormat={
                          form.config.auth_flow.inject_value_format
                        }
                      />
                    </div>
                  )}
                </div>
              )}
            </div>
          </EntityDetailCard>
        )}
      </EntityDetailLayout>

      <ConfirmDialog
        open={deleteOpen}
        onClose={() => setDeleteOpen(false)}
        onConfirm={() => void handleConfirmDelete()}
        title={t("common.tips")}
        description={t("supplier.messages.deleteConfirm", {
          name: supplier?.name ?? "",
        })}
        confirmLabel={t("common.confirm")}
        cancelLabel={t("common.cancel")}
        loading={deleting}
      />
    </>
  );
}
