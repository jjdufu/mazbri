import React, { useMemo } from "react";
import { AlertTriangle } from "lucide-react";
import type { ConnectorEndpoint } from "@/api/upstream-connectors";
import {
  Badge,
  Input,
  Switch,
  Textarea,
  Tooltip,
  cn,
} from "@/components/ui";
import { useT } from "@/locales";
import {
  validateTlsFormClient,
  type TlsFormState,
} from "../engine/workbenchHelpers";

export interface WorkbenchTlsTimeoutPaneProps {
  endpoint: ConnectorEndpoint;
  supplierTimeoutMs: number;
  supplierTls: TlsFormState;
  endpointTls: TlsFormState;
  canWrite: boolean;
  isSuperAdmin: boolean;
  onUpdateEndpoint: (
    updater: (ep: ConnectorEndpoint) => ConnectorEndpoint,
  ) => void;
  onChangeSupplierTimeoutMs: (ms: number) => void;
  onChangeSupplierTls: (form: TlsFormState) => void;
  onChangeEndpointTls: (form: TlsFormState) => void;
}

function SheetRow({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="grid grid-cols-1 sm:grid-cols-[168px_minmax(0,1fr)] items-center gap-2 border-b border-border-subtle px-3 py-2 last:border-b-0">
      <span className="text-xs font-medium text-muted-foreground">{label}</span>
      <div className="min-w-0">{children}</div>
    </div>
  );
}

function TlsCard({
  title,
  form,
  canWrite,
  isSuperAdmin,
  onChange,
}: {
  title: string;
  form: TlsFormState;
  canWrite: boolean;
  isSuperAdmin: boolean;
  onChange: (next: TlsFormState) => void;
}) {
  const { t } = useT();
  const issues = useMemo(() => validateTlsFormClient(form), [form]);

  return (
    <section className="flex flex-col overflow-hidden rounded-lg border border-border bg-card">
      <header className="flex min-h-9 items-center justify-between gap-2 border-b border-border bg-muted/40 px-3 py-1.5">
        <span className="text-xs font-semibold text-foreground">{title}</span>
        <label className="inline-flex items-center gap-2 text-xs text-muted-foreground cursor-pointer select-none">
          <Switch
            checked={form.enabled}
            disabled={!canWrite}
            aria-label={`${title} ${t("upstreamConnector.tlsEnable")}`}
            onCheckedChange={(enabled) => onChange({ ...form, enabled })}
          />
          <span>{t("upstreamConnector.tlsEnable")}</span>
        </label>
      </header>

      {form.enabled && (
        <div className="flex flex-col gap-3 p-3">
          <Tooltip
            content={
              isSuperAdmin ? "" : t("upstreamConnector.tlsInsecureAdminOnly")
            }
            disabled={isSuperAdmin}
            side="top"
          >
            <label className="inline-flex items-center gap-2 text-xs text-foreground cursor-pointer select-none">
              <input
                type="checkbox"
                checked={form.insecure_skip_verify}
                disabled={!canWrite || !isSuperAdmin}
                onChange={(e) =>
                  onChange({
                    ...form,
                    insecure_skip_verify: e.target.checked,
                  })
                }
              />
              <span>{t("upstreamConnector.tlsInsecureSkip")}</span>
            </label>
          </Tooltip>

          <label className="flex flex-col gap-1">
            <span className="text-[11.5px] font-medium text-muted-foreground">
              {t("upstreamConnector.tlsCaCert")}
            </span>
            <Textarea
              rows={3}
              value={form.ca_cert}
              disabled={!canWrite}
              placeholder="-----BEGIN CERTIFICATE-----"
              spellCheck={false}
              className="font-mono-tech text-[11.5px] leading-snug"
              onChange={(e) => onChange({ ...form, ca_cert: e.target.value })}
            />
          </label>

          <label className="flex flex-col gap-1">
            <span className="text-[11.5px] font-medium text-muted-foreground">
              {t("upstreamConnector.tlsClientCert")}
            </span>
            <Textarea
              rows={3}
              value={form.client_cert}
              disabled={!canWrite}
              placeholder="-----BEGIN CERTIFICATE-----"
              spellCheck={false}
              className="font-mono-tech text-[11.5px] leading-snug"
              onChange={(e) =>
                onChange({ ...form, client_cert: e.target.value })
              }
            />
          </label>

          <label className="flex flex-col gap-1">
            <span className="inline-flex items-center gap-2 text-[11.5px] font-medium text-muted-foreground">
              <span>{t("upstreamConnector.tlsClientKey")}</span>
              {form.has_saved_key && !form.client_key.trim() && (
                <Badge variant="success">
                  {t("upstreamConnector.tlsKeySavedTag")}
                </Badge>
              )}
            </span>
            <Textarea
              rows={3}
              value={form.client_key}
              disabled={!canWrite}
              placeholder={t("upstreamConnector.tlsKeyReplacePh")}
              spellCheck={false}
              className="font-mono-tech text-[11.5px] leading-snug"
              onChange={(e) =>
                onChange({ ...form, client_key: e.target.value })
              }
            />
          </label>

          {issues.length > 0 && (
            <div className="rounded-md border border-warning-border bg-warning-bg px-2.5 py-2 space-y-1">
              {issues.map((code) => (
                <div
                  key={code}
                  className="flex items-center gap-1.5 text-xs text-warning-fg"
                >
                  <AlertTriangle className="h-3.5 w-3.5 shrink-0" />
                  <span>{t(`upstreamConnector.tlsErr.${code}`)}</span>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </section>
  );
}

export function WorkbenchTlsTimeoutPane({
  endpoint,
  supplierTimeoutMs,
  supplierTls,
  endpointTls,
  canWrite,
  isSuperAdmin,
  onUpdateEndpoint,
  onChangeSupplierTimeoutMs,
  onChangeSupplierTls,
  onChangeEndpointTls,
}: WorkbenchTlsTimeoutPaneProps) {
  const { t } = useT();

  const handleEndpointTimeout = (raw: string) => {
    const trimmed = raw.trim();
    if (!trimmed) {
      onUpdateEndpoint((ep) => ({ ...ep, timeout_ms: undefined }));
      return;
    }
    const n = Number(trimmed);
    if (Number.isFinite(n) && n > 0) {
      onUpdateEndpoint((ep) => ({ ...ep, timeout_ms: Math.round(n) }));
    }
  };

  const handleSupplierTimeout = (raw: string) => {
    const n = Number(raw);
    if (Number.isFinite(n) && n > 0) {
      onChangeSupplierTimeoutMs(Math.round(n));
    }
  };

  return (
    <div
      className={cn(
        "flex flex-col gap-3.5",
        !canWrite && "pointer-events-none opacity-75",
      )}
    >
      <div className="rounded-lg border border-border bg-card overflow-hidden">
        <SheetRow label={t("upstreamConnector.timeoutEndpoint")}>
          <Input
            type="number"
            min={100}
            max={600000}
            disabled={!canWrite}
            value={endpoint.timeout_ms ?? ""}
            placeholder="15000"
            aria-label={t("upstreamConnector.timeoutEndpoint")}
            className="font-mono-tech tabular-nums"
            onChange={(e) => handleEndpointTimeout(e.target.value)}
          />
        </SheetRow>

        <SheetRow label={t("upstreamConnector.timeoutSupplier")}>
          <Input
            type="number"
            min={100}
            max={600000}
            disabled={!canWrite}
            value={supplierTimeoutMs}
            placeholder="15000"
            aria-label={t("upstreamConnector.timeoutSupplier")}
            className="font-mono-tech tabular-nums"
            onChange={(e) => handleSupplierTimeout(e.target.value)}
          />
        </SheetRow>

        <SheetRow label={t("upstreamConnector.endpointBaseUrl")}>
          <Input
            type="text"
            disabled={!canWrite}
            value={endpoint.base_url || ""}
            placeholder="https://api.example.com"
            aria-label={t("upstreamConnector.endpointBaseUrl")}
            spellCheck={false}
            className="font-mono-tech"
            onChange={(e) => {
              const val = e.target.value;
              onUpdateEndpoint((ep) => ({ ...ep, base_url: val }));
            }}
          />
        </SheetRow>

        <SheetRow label={t("upstreamConnector.requestIdHeader")}>
          <Input
            type="text"
            disabled={!canWrite}
            value={endpoint.request_id_header || ""}
            placeholder="X-Upstream-Req-Id"
            aria-label={t("upstreamConnector.requestIdHeader")}
            spellCheck={false}
            className="font-mono-tech"
            onChange={(e) => {
              const val = e.target.value;
              onUpdateEndpoint((ep) => ({ ...ep, request_id_header: val }));
            }}
          />
        </SheetRow>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-3">
        <TlsCard
          title={t("upstreamConnector.tlsSupplierSection")}
          form={supplierTls}
          canWrite={canWrite}
          isSuperAdmin={isSuperAdmin}
          onChange={onChangeSupplierTls}
        />
        <TlsCard
          title={t("upstreamConnector.tlsEndpointSection")}
          form={endpointTls}
          canWrite={canWrite}
          isSuperAdmin={isSuperAdmin}
          onChange={onChangeEndpointTls}
        />
      </div>
    </div>
  );
}
