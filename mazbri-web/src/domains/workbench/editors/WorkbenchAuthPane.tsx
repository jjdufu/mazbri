import React, { useEffect, useMemo, useState } from "react";
import {
  ChevronDown,
  ChevronRight,
  Play,
  Plus,
  Trash2,
  X,
} from "lucide-react";
import {
  Badge,
  Button,
  IconButton,
  Input,
  NumberInput,
  PasswordInput,
  Select,
  Switch,
  cn,
  type SelectOptionItem,
} from "@/components/ui";
import { useT } from "@/locales";
import type { AuthFlowForm } from "@/domains/upstream-supply/supplier-form";
import { InjectPreviewCard } from "@/domains/upstream-supply/components/InjectPreviewCard";
import { JsonPathHelper } from "@/domains/upstream-supply/components/JsonPathHelper";
import { JsonTemplateEditor } from "@/domains/upstream-supply/components/JsonTemplateEditor";
import {
  applyPickerDefaults,
  authFieldGroup,
  authPickerFromState,
  authProfileFromPicker,
  credValue,
  isSessionHttpAuthActive,
  sessionProfileLabel,
  setCredValue,
  visibleAuthPickers,
  type AuthPickerId,
} from "../engine/authMatrix";
import {
  createEmptyInvalidation,
  invalidationFields,
  validateInvalidation,
  type InvalidationForm,
  type InvalidationSource,
} from "../engine/authInvalidation";
import { normalizeKvRow, type KvRow } from "../engine/workbenchHelpers";
import { WorkbenchKvRows } from "./WorkbenchKvRows";

export interface WorkbenchAuthPaneProps {
  profile: string;
  flow: AuthFlowForm;
  adapter?: string;
  invalidationSource?: InvalidationSource | "";
  env?: Record<string, string>;
  seedJson?: string;
  running?: boolean;
  disabled?: boolean;
  requiredCredKeys?: string[];
  onChangeProfile: (profile: string) => void;
  onUpdateFlow: (updater: (flow: AuthFlowForm) => AuthFlowForm) => void;
  onRunAuth: () => void;
}

const HTTP_STATUS_PRESETS = [401, 403, 419, 440, 498] as const;

function cloneFlow(flow: AuthFlowForm): AuthFlowForm {
  return {
    ...flow,
    headers: (flow.headers || []).map((h) => ({ ...h })),
    credentials: (flow.credentials || []).map((c) => ({ ...c })),
    jwt: flow.jwt
      ? { ...flow.jwt }
      : {
          algorithm: "HS256",
          ttl_secs: 300,
          claims_tpl: '{"sub":"mazbri"}',
          kid: "",
        },
    digest: flow.digest
      ? { ...flow.digest }
      : { algorithm: "MD5", realm: "" },
    aws: flow.aws ? { ...flow.aws } : { region: "", service: "" },
    hawk: flow.hawk ? { ...flow.hawk } : { algorithm: "sha256", ext: "" },
    invalidation: flow.invalidation
      ? {
          ...flow.invalidation,
          http_status: [...(flow.invalidation.http_status || [])],
          jsonrpc_codes: [...(flow.invalidation.jsonrpc_codes || [])],
          body_match: (flow.invalidation.body_match || []).map((r) => ({
            ...r,
          })),
        }
      : createEmptyInvalidation(),
  };
}

function FieldRow({
  label,
  required,
  children,
}: {
  label: string;
  required?: boolean;
  children: React.ReactNode;
}) {
  return (
    <div className="grid grid-cols-1 sm:grid-cols-[168px_minmax(0,1fr)] items-center gap-2 border-b border-border-subtle px-3 py-2 last:border-b-0">
      <span className="text-xs font-medium text-muted-foreground">
        {label}
        {required && <span className="ml-0.5 text-danger-fg">*</span>}
      </span>
      <div className="min-w-0">{children}</div>
    </div>
  );
}

function AuthInjectSection({
  flow,
  disabled,
  onUpdateFlow,
}: {
  flow: AuthFlowForm;
  disabled: boolean;
  onUpdateFlow: (updater: (flow: AuthFlowForm) => AuthFlowForm) => void;
}) {
  const { t } = useT();
  const [previewOpen, setPreviewOpen] = useState(false);

  const injectToOptions: SelectOptionItem[] = useMemo(
    () => [
      {
        value: "header",
        label: t("upstreamConnector.injectToHeader"),
        text: t("upstreamConnector.injectToHeader"),
      },
      {
        value: "query",
        label: t("upstreamConnector.injectToQuery"),
        text: t("upstreamConnector.injectToQuery"),
      },
      {
        value: "metadata",
        label: t("upstreamConnector.injectToMetadata"),
        text: t("upstreamConnector.injectToMetadata"),
      },
      {
        value: "first_frame",
        label: t("upstreamConnector.injectToFirstFrame"),
        text: t("upstreamConnector.injectToFirstFrame"),
      },
    ],
    [t],
  );

  return (
    <>
      <FieldRow label={t("supplier.config.injectLocation")}>
        <Select
          value={flow.inject_to || "header"}
          options={injectToOptions}
          disabled={disabled}
          aria-label={t("supplier.config.injectLocation")}
          className="w-full"
          onChange={(e) => {
            const val = e.target.value;
            onUpdateFlow((prev) => ({ ...prev, inject_to: val }));
          }}
        />
      </FieldRow>
      <FieldRow label={t("supplier.config.injectKey")}>
        <Input
          value={flow.inject_key}
          disabled={disabled}
          placeholder="Authorization"
          aria-label={t("supplier.config.injectKey")}
          className="font-mono-tech"
          onChange={(e) => {
            const val = e.target.value;
            onUpdateFlow((prev) => ({ ...prev, inject_key: val }));
          }}
        />
      </FieldRow>
      <FieldRow label={t("supplier.config.injectFormat")}>
        <Input
          value={flow.inject_value_format}
          disabled={disabled}
          placeholder="Bearer {}"
          aria-label={t("supplier.config.injectFormat")}
          className="font-mono-tech"
          onChange={(e) => {
            const val = e.target.value;
            onUpdateFlow((prev) => ({ ...prev, inject_value_format: val }));
          }}
        />
      </FieldRow>
      <div className="border-b border-border-subtle last:border-b-0">
        <button
          type="button"
          onClick={() => setPreviewOpen((v) => !v)}
          className="flex w-full items-center gap-1.5 px-3 py-2 text-left text-xs font-medium text-muted-foreground hover:bg-hover-bg hover:text-foreground cursor-pointer"
        >
          {previewOpen ? (
            <ChevronDown className="h-3.5 w-3.5 shrink-0" />
          ) : (
            <ChevronRight className="h-3.5 w-3.5 shrink-0" />
          )}
          <span>{t("supplier.config.injectPreview")}</span>
        </button>
        {previewOpen && (
          <div className="px-3 pb-3">
            <InjectPreviewCard
              injectTo={flow.inject_to}
              injectKey={flow.inject_key}
              injectValueFormat={flow.inject_value_format}
              compact
            />
          </div>
        )}
      </div>
    </>
  );
}

function NumberTagInput({
  values,
  presets = [],
  placeholder,
  ariaLabel,
  disabled,
  onChange,
}: {
  values: number[];
  presets?: readonly number[];
  placeholder: string;
  ariaLabel: string;
  disabled: boolean;
  onChange: (next: number[]) => void;
}) {
  const [draft, setDraft] = useState("");

  const addCode = (raw: string) => {
    const parts = raw
      .split(/[,\s]+/)
      .map((s) => s.trim())
      .filter(Boolean);
    if (!parts.length) return;
    const seen = new Set(values);
    const next = [...values];
    for (const p of parts) {
      const n = Number(p);
      if (Number.isInteger(n) && !seen.has(n)) {
        seen.add(n);
        next.push(n);
      }
    }
    onChange(next);
    setDraft("");
  };

  const togglePreset = (code: number) => {
    if (disabled) return;
    if (values.includes(code)) {
      onChange(values.filter((v) => v !== code));
    } else {
      onChange([...values, code]);
    }
  };

  return (
    <div className="space-y-1.5">
      <div className="flex flex-wrap items-center gap-1.5 rounded-md border border-input bg-card px-2 py-1 min-h-8 focus-within:border-primary focus-within:ring-2 focus-within:ring-primary/25">
        {values.map((code) => (
          <Badge key={code} variant="xblue" className="font-mono-tech gap-1">
            <span>{code}</span>
            {!disabled && (
              <button
                type="button"
                aria-label={`Remove ${code}`}
                onClick={() => onChange(values.filter((v) => v !== code))}
                className="inline-flex items-center hover:text-danger-fg cursor-pointer"
              >
                <X className="h-3 w-3" />
              </button>
            )}
          </Badge>
        ))}
        <input
          type="text"
          value={draft}
          disabled={disabled}
          placeholder={placeholder}
          aria-label={ariaLabel}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={() => addCode(draft)}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === ",") {
              e.preventDefault();
              addCode(draft);
            }
          }}
          className="min-w-[80px] flex-1 bg-transparent text-xs font-mono-tech text-foreground placeholder:text-muted-foreground outline-none disabled:opacity-50"
        />
      </div>
      {presets.length > 0 && (
        <div className="flex flex-wrap items-center gap-1">
          {presets.map((code) => {
            const active = values.includes(code);
            return (
              <button
                key={code}
                type="button"
                disabled={disabled}
                onClick={() => togglePreset(code)}
                className={cn(
                  "h-6 rounded-md border px-2 font-mono-tech text-[11px] transition-colors disabled:opacity-45 cursor-pointer",
                  active
                    ? "border-primary/50 bg-primary-soft text-primary font-semibold"
                    : "border-border bg-muted/40 text-muted-foreground hover:bg-hover-bg hover:text-foreground",
                )}
              >
                {code}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}

function AuthInvalidationSection({
  form,
  adapter,
  source,
  disabled,
  onChange,
}: {
  form: InvalidationForm;
  adapter: string;
  source: InvalidationSource | "";
  disabled: boolean;
  onChange: (next: InvalidationForm) => void;
}) {
  const { t } = useT();
  const fields = useMemo(() => invalidationFields(adapter), [adapter]);

  const sourceLabel = useMemo(() => {
    switch (source) {
      case "channel":
        return t("upstreamConnector.invalidationSourceChannel");
      case "supplier":
        return t("upstreamConnector.invalidationSourceSupplier");
      default:
        return t("upstreamConnector.invalidationSourceDefault");
    }
  }, [source, t]);

  const sourceVariant =
    source === "channel"
      ? "warning"
      : source === "supplier"
        ? "xblue"
        : "neutral";

  const issueText = useMemo(() => {
    const issue = validateInvalidation(form);
    if (!issue) return "";
    switch (issue.kind) {
      case "status_range":
        return t("upstreamConnector.invalidationErrStatusRange", {
          code: issue.value,
        });
      case "status_duplicate":
      case "jsonrpc_duplicate":
        return t("upstreamConnector.invalidationErrDuplicate", {
          code: issue.value,
        });
      case "body_path":
        return t("upstreamConnector.invalidationErrBodyPath", {
          index: issue.index + 1,
        });
      case "body_equals":
        return t("upstreamConnector.invalidationErrBodyEquals", {
          index: issue.index + 1,
        });
      default:
        return "";
    }
  }, [form, t]);

  return (
    <div className="border-t border-dashed border-border p-3 space-y-2.5">
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-2 min-w-0">
          <span className="text-xs font-semibold text-foreground">
            {t("upstreamConnector.invalidation")}
          </span>
          {source && (
            <Badge variant={sourceVariant}>
              {t("upstreamConnector.invalidationSource")}: {sourceLabel}
            </Badge>
          )}
        </div>
        <Switch
          checked={form.enabled}
          disabled={disabled}
          aria-label={t("upstreamConnector.invalidation")}
          onCheckedChange={(enabled) => onChange({ ...form, enabled })}
        />
      </div>

      <p className="text-xs text-muted-foreground">
        {t("upstreamConnector.invalidationDesc")}
      </p>

      {source === "channel" && (
        <p className="rounded-md border border-warning-border bg-warning-bg px-2.5 py-1.5 text-xs text-warning-fg">
          {t("upstreamConnector.invalidationChannelOverride")}
        </p>
      )}

      {form.enabled && (
        <div className="space-y-3 pt-1">
          <div className="space-y-1">
            <label className="block text-xs font-medium text-foreground">
              {t("upstreamConnector.invalidationHttpStatus")}
            </label>
            <NumberTagInput
              values={form.http_status}
              presets={HTTP_STATUS_PRESETS}
              placeholder="401, 403"
              ariaLabel={t("upstreamConnector.invalidationHttpStatus")}
              disabled={disabled}
              onChange={(http_status) => onChange({ ...form, http_status })}
            />
          </div>

          {fields.jsonrpc && (
            <div className="space-y-1">
              <label className="block text-xs font-medium text-foreground">
                {t("upstreamConnector.invalidationJsonrpcCodes")}
              </label>
              <NumberTagInput
                values={form.jsonrpc_codes}
                placeholder="-32001, -32002"
                ariaLabel={t("upstreamConnector.invalidationJsonrpcCodes")}
                disabled={disabled}
                onChange={(jsonrpc_codes) =>
                  onChange({ ...form, jsonrpc_codes })
                }
              />
            </div>
          )}

          <div className="space-y-1.5">
            <div className="flex items-center justify-between gap-2">
              <span className="text-xs font-medium text-foreground">
                {t("upstreamConnector.invalidationBodyMatch")}
              </span>
              <Button
                type="button"
                variant="link"
                size="xs"
                disabled={disabled}
                onClick={() =>
                  onChange({
                    ...form,
                    body_match: [...form.body_match, { path: "", equals: "" }],
                  })
                }
              >
                <Plus className="h-3.5 w-3.5" />
                {t("upstreamConnector.invalidationAddBodyMatch")}
              </Button>
            </div>
            <p className="text-[11px] text-muted-foreground">
              {t("upstreamConnector.invalidationBodyMatchDesc")}
            </p>
            {form.body_match.map((row, idx) => (
              <div key={idx} className="flex items-center gap-2">
                <Input
                  value={row.path}
                  disabled={disabled}
                  placeholder="$.code"
                  aria-label={t("upstreamConnector.invalidationBodyMatch")}
                  className="font-mono-tech"
                  onChange={(e) => {
                    const body_match = form.body_match.map((item, i) =>
                      i === idx ? { ...item, path: e.target.value } : item,
                    );
                    onChange({ ...form, body_match });
                  }}
                />
                <Input
                  value={row.equals}
                  disabled={disabled}
                  placeholder='"TOKEN_EXPIRED", 40101'
                  aria-label={t("upstreamConnector.invalidationBodyMatch")}
                  className="font-mono-tech"
                  onChange={(e) => {
                    const body_match = form.body_match.map((item, i) =>
                      i === idx ? { ...item, equals: e.target.value } : item,
                    );
                    onChange({ ...form, body_match });
                  }}
                />
                <IconButton
                  size="sm"
                  disabled={disabled}
                  aria-label={t("common.actions.delete")}
                  title={t("common.actions.delete")}
                  onClick={() =>
                    onChange({
                      ...form,
                      body_match: form.body_match.filter((_, i) => i !== idx),
                    })
                  }
                  className="text-danger-fg hover:text-danger-fg hover:bg-danger-fg/10"
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </IconButton>
              </div>
            ))}
          </div>

          {fields.wsClose && (
            <div className="space-y-1">
              <div className="flex items-center gap-2">
                <Switch
                  checked={form.ws_close_before_reply}
                  disabled={disabled}
                  aria-label={t("upstreamConnector.invalidationWsClose")}
                  onCheckedChange={(ws_close_before_reply) =>
                    onChange({ ...form, ws_close_before_reply })
                  }
                />
                <span className="text-xs font-medium text-foreground">
                  {t("upstreamConnector.invalidationWsClose")}
                </span>
              </div>
              <p className="text-[11px] text-muted-foreground">
                {t("upstreamConnector.invalidationWsCloseDesc")}
              </p>
            </div>
          )}

          {issueText && (
            <div className="rounded-md border border-warning-border bg-warning-bg px-2.5 py-1.5 text-xs text-warning-fg">
              {issueText}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

export function WorkbenchAuthPane({
  profile,
  flow,
  adapter = "",
  invalidationSource = "",
  env = {},
  seedJson = "",
  running = false,
  disabled = false,
  requiredCredKeys = [],
  onChangeProfile,
  onUpdateFlow,
  onRunAuth,
}: WorkbenchAuthPaneProps) {
  const { t } = useT();
  const [bodyOpen, setBodyOpen] = useState(false);

  const sessionHttpActive = isSessionHttpAuthActive(flow.enabled, flow.profile);
  const picker = authPickerFromState(profile, flow.enabled, flow.profile);
  const group = authFieldGroup(picker);

  const nonEmptyCredCount = useMemo(
    () => (flow.credentials || []).filter((r) => r.key?.trim()).length,
    [flow.credentials],
  );

  const [rawOpen, setRawOpen] = useState(
    () => group === "empty" && nonEmptyCredCount > 0,
  );

  useEffect(() => {
    if (group === "empty" && nonEmptyCredCount > 0) {
      setRawOpen(true);
    }
  }, [group, nonEmptyCredCount]);

  const pickerOptions: SelectOptionItem[] = useMemo(() => {
    const labelOf = (id: AuthPickerId): string => {
      switch (id) {
        case "inherit":
          return t("upstreamConnector.authInherit");
        case "skip":
          return t("upstreamConnector.authSkip");
        case "none":
          return t("upstreamConnector.authNone");
        case "api_key":
          return t("upstreamConnector.authApiKey");
        case "oauth_cc_form":
          return t("upstreamConnector.authOauthCc");
        case "gettoken":
          return t("upstreamConnector.authGettoken");
        case "basic":
          return t("upstreamConnector.authBasic");
        case "sign_form":
          return t("upstreamConnector.authSignForm");
        case "grpc_metadata":
          return t("upstreamConnector.authGrpcMeta");
        case "bearer":
          return t("upstreamConnector.authBearer");
        case "jwt":
          return t("upstreamConnector.authJwt");
        case "digest":
          return t("upstreamConnector.authDigest");
        case "aws_sigv4":
          return t("upstreamConnector.authAws");
        case "hawk":
          return t("upstreamConnector.authHawk");
        default:
          return id;
      }
    };
    return visibleAuthPickers(flow.enabled, flow.profile).map((id) => {
      const label = labelOf(id);
      return { value: id, label, text: label };
    });
  }, [flow.enabled, flow.profile, t]);

  const sessionLabel = useMemo(() => {
    if (!sessionHttpActive) return t("upstreamConnector.authSessionUnset");
    const p = sessionProfileLabel(flow.profile);
    if (!p) return t("upstreamConnector.authSessionUnset");
    const hit = pickerOptions.find((o) => o.value === p);
    return hit ? hit.text : p;
  }, [sessionHttpActive, flow.profile, pickerOptions, t]);

  const canSetSession =
    picker !== "inherit" && picker !== "none" && picker !== "skip";

  const handlePick = (nextValue: string) => {
    const next = nextValue as AuthPickerId;
    onChangeProfile(authProfileFromPicker(next));
    onUpdateFlow((prev) => {
      const cloned = cloneFlow(prev);
      applyPickerDefaults(next, cloned);
      return cloned;
    });
  };

  const handleEnabledChange = (checked: boolean) => {
    onUpdateFlow((prev) => {
      const cloned = cloneFlow(prev);
      cloned.enabled = checked;
      if (checked && !sessionProfileLabel(cloned.profile)) {
        const fallback: AuthPickerId = canSetSession ? picker : "bearer";
        cloned.profile = fallback;
        applyPickerDefaults(fallback, cloned);
      }
      return cloned;
    });
  };

  const handleSetSessionDefault = () => {
    if (!canSetSession) return;
    onUpdateFlow((prev) => {
      const cloned = cloneFlow(prev);
      cloned.enabled = true;
      cloned.profile = picker;
      applyPickerDefaults(picker, cloned);
      return cloned;
    });
  };

  const handleClearProfile = () => {
    onChangeProfile(sessionHttpActive ? "skip" : "none");
  };

  const cred = (key: string): string => credValue(flow, key);
  const setCred = (key: string, value: string) => {
    onUpdateFlow((prev) => {
      const cloned = cloneFlow(prev);
      setCredValue(cloned, key, value);
      return cloned;
    });
  };
  const isCredRequired = (key: string): boolean =>
    requiredCredKeys.includes(key) && !cred(key).trim();

  const hintText = useMemo(() => {
    switch (picker) {
      case "basic":
        return t("upstreamConnector.authHintBasic");
      case "api_key":
        return t("upstreamConnector.authHintApiKey");
      case "oauth_cc_form":
        return t("upstreamConnector.authHintOauth");
      case "gettoken":
        return t("upstreamConnector.authHintGettoken");
      case "sign_form":
        return t("upstreamConnector.authHintSign");
      case "grpc_metadata":
        return t("upstreamConnector.authHintGrpc");
      case "bearer":
        return t("upstreamConnector.authHintBearer");
      case "jwt":
        return t("upstreamConnector.authHintJwt");
      case "digest":
        return t("upstreamConnector.authHintDigest");
      case "aws_sigv4":
        return t("upstreamConnector.authHintAws");
      case "hawk":
        return t("upstreamConnector.authHintHawk");
      default:
        return "";
    }
  }, [picker, t]);

  const methodOptions: SelectOptionItem[] = [
    { value: "GET", label: "GET", text: "GET" },
    { value: "POST", label: "POST", text: "POST" },
  ];
  const digestAlgoOptions: SelectOptionItem[] = [
    { value: "MD5", label: "MD5", text: "MD5" },
    { value: "SHA-256", label: "SHA-256", text: "SHA-256" },
  ];
  const jwtAlgoOptions: SelectOptionItem[] = [
    "HS256",
    "HS384",
    "HS512",
    "RS256",
    "RS384",
    "RS512",
  ].map((v) => ({ value: v, label: v, text: v }));
  const hawkAlgoOptions: SelectOptionItem[] = [
    { value: "sha256", label: "sha256", text: "sha256" },
  ];

  const isHsJwt = String(flow.jwt?.algorithm || "HS256").startsWith("HS");

  const authVariables = useMemo(() => {
    const keys = (flow.credentials || [])
      .map((c) => c.key.trim())
      .filter(Boolean);
    const extra = keys.map((k) => `credential.${k}`);
    return [...keys, ...extra, "username", "password"];
  }, [flow.credentials]);

  const headerRows = useMemo<KvRow[]>(
    () => (flow.headers || []).map((row) => normalizeKvRow(row)),
    [flow.headers],
  );

  const credRows = useMemo<KvRow[]>(
    () =>
      (flow.credentials || []).map((row) =>
        normalizeKvRow({ key: row.key, value: row.value }),
      ),
    [flow.credentials],
  );

  const invalidRing = "border-danger-fg ring-2 ring-danger-fg/20";

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center justify-between gap-2 border-b border-border-subtle pb-2">
        <div className="flex flex-wrap items-center gap-2 min-w-0">
          <span className="text-xs font-semibold text-muted-foreground">
            {t("upstreamConnector.authType")}
          </span>
          <Select
            value={picker}
            options={pickerOptions}
            disabled={disabled}
            aria-label={t("upstreamConnector.authType")}
            className="w-44"
            onChange={(e) => handlePick(e.target.value)}
          />
          <span className="text-xs text-muted-foreground truncate">
            {t("upstreamConnector.authSessionDefault")}:{" "}
            <span className="font-medium text-foreground">{sessionLabel}</span>
          </span>
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={disabled || !canSetSession}
            onClick={handleSetSessionDefault}
          >
            {t("upstreamConnector.authSetSessionDefault")}
          </Button>
        </div>

        <div className="flex items-center gap-2 shrink-0">
          <label className="inline-flex items-center gap-1.5 text-xs text-muted-foreground cursor-pointer select-none">
            <Switch
              checked={sessionHttpActive}
              disabled={disabled}
              aria-label={t("upstreamConnector.authEnabled")}
              onCheckedChange={handleEnabledChange}
            />
            <span>{t("upstreamConnector.authEnabled")}</span>
          </label>
          <IconButton
            size="sm"
            disabled={disabled}
            title={t("upstreamConnector.authClear")}
            aria-label={t("upstreamConnector.authClear")}
            onClick={handleClearProfile}
          >
            <Trash2 className="h-3.5 w-3.5" />
          </IconButton>
          <Button
            type="button"
            variant="outline"
            size="sm"
            loading={running}
            disabled={disabled || running}
            onClick={onRunAuth}
          >
            {!running && <Play className="h-3.5 w-3.5" />}
            {t("upstreamConnector.runAuth")}
          </Button>
        </div>
      </div>

      {group === "empty" ? (
        <div className="rounded-lg border border-dashed border-border px-4 py-6 text-center text-xs text-muted-foreground">
          {picker === "inherit"
            ? t("upstreamConnector.authInheritDesc", { profile: sessionLabel })
            : picker === "skip"
              ? t("upstreamConnector.authSkipEmpty")
              : t("upstreamConnector.authNoneEmpty")}
        </div>
      ) : (
        <div className="rounded-lg border border-border bg-card overflow-hidden">
          {hintText && (
            <p className="border-b border-border-subtle bg-muted/35 px-3 py-2 text-xs text-muted-foreground">
              {hintText}
            </p>
          )}

          {(group === "basic" || group === "digest") && (
            <>
              <FieldRow label={t("upstreamConnector.authUsername")}>
                <Input
                  value={cred("username")}
                  disabled={disabled}
                  aria-label={t("upstreamConnector.authUsername")}
                  onChange={(e) => setCred("username", e.target.value)}
                />
              </FieldRow>
              <FieldRow
                label={t("upstreamConnector.authPassword")}
                required={isCredRequired("password")}
              >
                <PasswordInput
                  value={cred("password")}
                  disabled={disabled}
                  aria-label={t("upstreamConnector.authPassword")}
                  className={cn(isCredRequired("password") && invalidRing)}
                  onChange={(e) => setCred("password", e.target.value)}
                />
              </FieldRow>
              {group === "digest" && (
                <>
                  <FieldRow label={t("upstreamConnector.authDigestAlgo")}>
                    <Select
                      value={flow.digest?.algorithm || "MD5"}
                      options={digestAlgoOptions}
                      disabled={disabled}
                      aria-label={t("upstreamConnector.authDigestAlgo")}
                      className="w-full"
                      onChange={(e) => {
                        const val = e.target.value;
                        onUpdateFlow((prev) => ({
                          ...prev,
                          digest: {
                            ...(prev.digest || { algorithm: "MD5", realm: "" }),
                            algorithm: val,
                          },
                        }));
                      }}
                    />
                  </FieldRow>
                  <FieldRow label={t("upstreamConnector.authDigestRealm")}>
                    <Input
                      value={flow.digest?.realm || ""}
                      disabled={disabled}
                      aria-label={t("upstreamConnector.authDigestRealm")}
                      onChange={(e) => {
                        const val = e.target.value;
                        onUpdateFlow((prev) => ({
                          ...prev,
                          digest: {
                            ...(prev.digest || { algorithm: "MD5", realm: "" }),
                            realm: val,
                          },
                        }));
                      }}
                    />
                  </FieldRow>
                </>
              )}
            </>
          )}

          {(group === "api_key" || group === "bearer") && (
            <>
              {group === "api_key" ? (
                <FieldRow
                  label={t("upstreamConnector.authApiKeyLabel")}
                  required={isCredRequired("api_key")}
                >
                  <PasswordInput
                    value={cred("api_key")}
                    disabled={disabled}
                    aria-label={t("upstreamConnector.authApiKeyLabel")}
                    className={cn(isCredRequired("api_key") && invalidRing)}
                    onChange={(e) => setCred("api_key", e.target.value)}
                  />
                </FieldRow>
              ) : (
                <FieldRow
                  label={t("upstreamConnector.authTokenLabel")}
                  required={isCredRequired("token")}
                >
                  <PasswordInput
                    value={cred("token")}
                    disabled={disabled}
                    aria-label={t("upstreamConnector.authTokenLabel")}
                    className={cn(isCredRequired("token") && invalidRing)}
                    onChange={(e) => setCred("token", e.target.value)}
                  />
                </FieldRow>
              )}
              <AuthInjectSection
                flow={flow}
                disabled={disabled}
                onUpdateFlow={onUpdateFlow}
              />
            </>
          )}

          {group === "ticket" && (
            <>
              <FieldRow label={t("supplier.config.tokenUrl")}>
                <Input
                  value={flow.token_url}
                  disabled={disabled}
                  placeholder="https://api.upstream.com/get/token"
                  aria-label={t("supplier.config.tokenUrl")}
                  className="font-mono-tech"
                  onChange={(e) => {
                    const val = e.target.value;
                    onUpdateFlow((prev) => ({ ...prev, token_url: val }));
                  }}
                />
              </FieldRow>
              <FieldRow label={t("upstreamConnector.authTokenPath")}>
                <Input
                  value={flow.token_path}
                  disabled={disabled}
                  placeholder="/oauth/token"
                  aria-label={t("upstreamConnector.authTokenPath")}
                  className="font-mono-tech"
                  onChange={(e) => {
                    const val = e.target.value;
                    onUpdateFlow((prev) => ({ ...prev, token_path: val }));
                  }}
                />
              </FieldRow>
              <FieldRow label={t("supplier.config.method")}>
                <Select
                  value={flow.method || (picker === "gettoken" ? "GET" : "POST")}
                  options={methodOptions}
                  disabled={disabled}
                  aria-label={t("supplier.config.method")}
                  className="w-full"
                  onChange={(e) => {
                    const val = e.target.value;
                    onUpdateFlow((prev) => ({ ...prev, method: val }));
                  }}
                />
              </FieldRow>
              <div className="border-b border-border-subtle px-3 py-2">
                <WorkbenchKvRows
                  rows={headerRows}
                  onChange={(rows) =>
                    onUpdateFlow((prev) => ({
                      ...prev,
                      headers: rows.map((r) => ({
                        key: r.key,
                        value: r.value,
                      })),
                    }))
                  }
                  variant="headers"
                  emptyCompact
                  foldable
                  title={t("supplier.config.authHeaders")}
                  addTitle={t("upstreamConnector.addHeader")}
                  clearTitle={t("upstreamConnector.clearKv")}
                  env={env}
                  disabled={disabled}
                />
              </div>

              {picker === "oauth_cc_form" && (
                <>
                  <FieldRow label={t("upstreamConnector.authClientId")}>
                    <Input
                      value={cred("client_id")}
                      disabled={disabled}
                      aria-label={t("upstreamConnector.authClientId")}
                      onChange={(e) => setCred("client_id", e.target.value)}
                    />
                  </FieldRow>
                  <FieldRow
                    label={t("upstreamConnector.authClientSecret")}
                    required={isCredRequired("client_secret")}
                  >
                    <PasswordInput
                      value={cred("client_secret")}
                      disabled={disabled}
                      aria-label={t("upstreamConnector.authClientSecret")}
                      className={cn(
                        isCredRequired("client_secret") && invalidRing,
                      )}
                      onChange={(e) => setCred("client_secret", e.target.value)}
                    />
                  </FieldRow>
                  <div className="border-b border-border-subtle">
                    <button
                      type="button"
                      onClick={() => setBodyOpen((v) => !v)}
                      className="flex w-full items-center gap-1.5 px-3 py-2 text-left text-xs font-medium text-muted-foreground hover:bg-hover-bg hover:text-foreground cursor-pointer"
                    >
                      {bodyOpen ? (
                        <ChevronDown className="h-3.5 w-3.5 shrink-0" />
                      ) : (
                        <ChevronRight className="h-3.5 w-3.5 shrink-0" />
                      )}
                      <span>{t("supplier.config.requestBodyTpl")}</span>
                    </button>
                    {bodyOpen && (
                      <div className="px-3 pb-3">
                        <JsonTemplateEditor
                          value={flow.request_body_tpl}
                          onChange={(val) =>
                            onUpdateFlow((prev) => ({
                              ...prev,
                              request_body_tpl: val,
                            }))
                          }
                          disabled={disabled}
                          emptyPlaceholder
                          variables={authVariables}
                          keyColumnLabel={t("upstreamConnector.kvKey")}
                          valueColumnLabel={t("upstreamConnector.kvValue")}
                          opColumnLabel={t("common.fields.operations")}
                          keyPlaceholder={t("upstreamConnector.kvPhKey")}
                          valuePlaceholder={t("upstreamConnector.kvPhValue")}
                        />
                      </div>
                    )}
                  </div>
                </>
              )}

              {picker === "gettoken" && (
                <>
                  <FieldRow label={t("upstreamConnector.authAppId")}>
                    <Input
                      value={cred("app_id")}
                      disabled={disabled}
                      aria-label={t("upstreamConnector.authAppId")}
                      onChange={(e) => setCred("app_id", e.target.value)}
                    />
                  </FieldRow>
                  <FieldRow
                    label={t("upstreamConnector.authSecret")}
                    required={isCredRequired("secret")}
                  >
                    <PasswordInput
                      value={cred("secret")}
                      disabled={disabled}
                      aria-label={t("upstreamConnector.authSecret")}
                      className={cn(isCredRequired("secret") && invalidRing)}
                      onChange={(e) => setCred("secret", e.target.value)}
                    />
                  </FieldRow>
                </>
              )}

              <FieldRow label={t("supplier.config.tokenJsonpath")}>
                <div className="flex items-center gap-2">
                  <Input
                    value={flow.token_jsonpath}
                    disabled={disabled}
                    placeholder={
                      picker === "gettoken" ? "$.token" : "$.access_token"
                    }
                    aria-label={t("supplier.config.tokenJsonpath")}
                    className="font-mono-tech"
                    onChange={(e) => {
                      const val = e.target.value;
                      onUpdateFlow((prev) => ({
                        ...prev,
                        token_jsonpath: val,
                      }));
                    }}
                  />
                  <JsonPathHelper
                    plain
                    disabled={disabled}
                    seedJson={seedJson}
                    onSelect={(path) =>
                      onUpdateFlow((prev) => ({
                        ...prev,
                        token_jsonpath: path,
                      }))
                    }
                  />
                </div>
              </FieldRow>

              <FieldRow label={t("supplier.config.expiryJsonpath")}>
                <div className="flex items-center gap-2">
                  <Input
                    value={flow.expiry_jsonpath}
                    disabled={disabled}
                    placeholder="$.expires_in"
                    aria-label={t("supplier.config.expiryJsonpath")}
                    className="font-mono-tech"
                    onChange={(e) => {
                      const val = e.target.value;
                      onUpdateFlow((prev) => ({
                        ...prev,
                        expiry_jsonpath: val,
                      }));
                    }}
                  />
                  <JsonPathHelper
                    plain
                    disabled={disabled}
                    seedJson={seedJson}
                    onSelect={(path) =>
                      onUpdateFlow((prev) => ({
                        ...prev,
                        expiry_jsonpath: path,
                      }))
                    }
                  />
                </div>
              </FieldRow>

              <AuthInjectSection
                flow={flow}
                disabled={disabled}
                onUpdateFlow={onUpdateFlow}
              />
            </>
          )}

          {group === "sign_form" && (
            <>
              <FieldRow label={t("upstreamConnector.authAppId")}>
                <Input
                  value={cred("app_id")}
                  disabled={disabled}
                  aria-label={t("upstreamConnector.authAppId")}
                  onChange={(e) => setCred("app_id", e.target.value)}
                />
              </FieldRow>
              <FieldRow
                label={t("upstreamConnector.authClientSecret")}
                required={isCredRequired("client_secret")}
              >
                <PasswordInput
                  value={cred("client_secret")}
                  disabled={disabled}
                  aria-label={t("upstreamConnector.authClientSecret")}
                  className={cn(isCredRequired("client_secret") && invalidRing)}
                  onChange={(e) => setCred("client_secret", e.target.value)}
                />
              </FieldRow>
              {adapter && adapter !== "sign_form" && (
                <p className="px-3 py-2 text-xs text-warning-fg">
                  {t("upstreamConnector.authSignAdapterHint")}
                </p>
              )}
            </>
          )}

          {group === "grpc_metadata" && (
            <>
              <FieldRow label={t("supplier.config.injectKey")}>
                <Input
                  value={flow.inject_key}
                  disabled={disabled}
                  placeholder="Authorization"
                  aria-label={t("supplier.config.injectKey")}
                  className="font-mono-tech"
                  onChange={(e) => {
                    const val = e.target.value;
                    onUpdateFlow((prev) => ({ ...prev, inject_key: val }));
                  }}
                />
              </FieldRow>
              <FieldRow label={t("supplier.config.injectFormat")}>
                <Input
                  value={flow.inject_value_format}
                  disabled={disabled}
                  placeholder="{}"
                  aria-label={t("supplier.config.injectFormat")}
                  className="font-mono-tech"
                  onChange={(e) => {
                    const val = e.target.value;
                    onUpdateFlow((prev) => ({
                      ...prev,
                      inject_value_format: val,
                    }));
                  }}
                />
              </FieldRow>
              <FieldRow
                label={t("upstreamConnector.authSecret")}
                required={isCredRequired("api_key") || isCredRequired("secret")}
              >
                <PasswordInput
                  value={cred("api_key") || cred("secret")}
                  disabled={disabled}
                  aria-label={t("upstreamConnector.authSecret")}
                  className={cn(
                    (isCredRequired("api_key") || isCredRequired("secret")) &&
                      invalidRing,
                  )}
                  onChange={(e) => setCred("api_key", e.target.value)}
                />
              </FieldRow>
            </>
          )}

          {group === "jwt" && (
            <>
              <FieldRow label={t("upstreamConnector.authJwtAlgo")}>
                <Select
                  value={flow.jwt?.algorithm || "HS256"}
                  options={jwtAlgoOptions}
                  disabled={disabled}
                  aria-label={t("upstreamConnector.authJwtAlgo")}
                  className="w-full"
                  onChange={(e) => {
                    const val = e.target.value;
                    onUpdateFlow((prev) => ({
                      ...prev,
                      jwt: {
                        ...(prev.jwt || {
                          algorithm: "HS256",
                          ttl_secs: 300,
                          claims_tpl: '{"sub":"mazbri"}',
                          kid: "",
                        }),
                        algorithm: val,
                      },
                    }));
                  }}
                />
              </FieldRow>
              <FieldRow label={t("upstreamConnector.authJwtTtl")}>
                <NumberInput
                  value={flow.jwt?.ttl_secs || 300}
                  min={1}
                  disabled={disabled}
                  aria-label={t("upstreamConnector.authJwtTtl")}
                  onChange={(next) =>
                    onUpdateFlow((prev) => ({
                      ...prev,
                      jwt: {
                        ...(prev.jwt || {
                          algorithm: "HS256",
                          ttl_secs: 300,
                          claims_tpl: '{"sub":"mazbri"}',
                          kid: "",
                        }),
                        ttl_secs: next || 300,
                      },
                    }))
                  }
                />
              </FieldRow>
              <FieldRow label={t("upstreamConnector.authJwtKid")}>
                <Input
                  value={flow.jwt?.kid || ""}
                  disabled={disabled}
                  aria-label={t("upstreamConnector.authJwtKid")}
                  className="font-mono-tech"
                  onChange={(e) => {
                    const val = e.target.value;
                    onUpdateFlow((prev) => ({
                      ...prev,
                      jwt: {
                        ...(prev.jwt || {
                          algorithm: "HS256",
                          ttl_secs: 300,
                          claims_tpl: '{"sub":"mazbri"}',
                          kid: "",
                        }),
                        kid: val,
                      },
                    }));
                  }}
                />
              </FieldRow>
              <div className="border-b border-border-subtle p-3 space-y-2">
                <div className="text-xs font-medium text-muted-foreground">
                  {t("upstreamConnector.authJwtClaims")}
                </div>
                <JsonTemplateEditor
                  value={flow.jwt?.claims_tpl || ""}
                  onChange={(val) =>
                    onUpdateFlow((prev) => ({
                      ...prev,
                      jwt: {
                        ...(prev.jwt || {
                          algorithm: "HS256",
                          ttl_secs: 300,
                          claims_tpl: '{"sub":"mazbri"}',
                          kid: "",
                        }),
                        claims_tpl: val,
                      },
                    }))
                  }
                  disabled={disabled}
                  emptyPlaceholder
                  variables={authVariables}
                  keyColumnLabel={t("upstreamConnector.kvKey")}
                  valueColumnLabel={t("upstreamConnector.kvValue")}
                  opColumnLabel={t("common.fields.operations")}
                  keyPlaceholder={t("upstreamConnector.kvPhKey")}
                  valuePlaceholder={t("upstreamConnector.kvPhValue")}
                />
              </div>
              {isHsJwt ? (
                <FieldRow
                  label={t("upstreamConnector.authJwtSecret")}
                  required={isCredRequired("jwt_secret")}
                >
                  <PasswordInput
                    value={cred("jwt_secret")}
                    disabled={disabled}
                    aria-label={t("upstreamConnector.authJwtSecret")}
                    className={cn(isCredRequired("jwt_secret") && invalidRing)}
                    onChange={(e) => setCred("jwt_secret", e.target.value)}
                  />
                </FieldRow>
              ) : (
                <FieldRow
                  label={t("upstreamConnector.authJwtPrivateKey")}
                  required={isCredRequired("jwt_private_key")}
                >
                  <PasswordInput
                    value={cred("jwt_private_key")}
                    disabled={disabled}
                    aria-label={t("upstreamConnector.authJwtPrivateKey")}
                    className={cn(
                      isCredRequired("jwt_private_key") && invalidRing,
                    )}
                    onChange={(e) => setCred("jwt_private_key", e.target.value)}
                  />
                </FieldRow>
              )}
              <AuthInjectSection
                flow={flow}
                disabled={disabled}
                onUpdateFlow={onUpdateFlow}
              />
            </>
          )}

          {group === "aws_sigv4" && (
            <>
              <FieldRow label={t("upstreamConnector.authAwsRegion")}>
                <Input
                  value={flow.aws?.region || ""}
                  disabled={disabled}
                  placeholder="us-east-1"
                  aria-label={t("upstreamConnector.authAwsRegion")}
                  className="font-mono-tech"
                  onChange={(e) => {
                    const val = e.target.value;
                    onUpdateFlow((prev) => ({
                      ...prev,
                      aws: {
                        ...(prev.aws || { region: "", service: "" }),
                        region: val,
                      },
                    }));
                  }}
                />
              </FieldRow>
              <FieldRow label={t("upstreamConnector.authAwsService")}>
                <Input
                  value={flow.aws?.service || ""}
                  disabled={disabled}
                  placeholder="execute-api"
                  aria-label={t("upstreamConnector.authAwsService")}
                  className="font-mono-tech"
                  onChange={(e) => {
                    const val = e.target.value;
                    onUpdateFlow((prev) => ({
                      ...prev,
                      aws: {
                        ...(prev.aws || { region: "", service: "" }),
                        service: val,
                      },
                    }));
                  }}
                />
              </FieldRow>
              <FieldRow
                label={t("upstreamConnector.authAwsAccessKey")}
                required={isCredRequired("access_key_id")}
              >
                <PasswordInput
                  value={cred("access_key_id")}
                  disabled={disabled}
                  aria-label={t("upstreamConnector.authAwsAccessKey")}
                  className={cn(
                    isCredRequired("access_key_id") && invalidRing,
                  )}
                  onChange={(e) => setCred("access_key_id", e.target.value)}
                />
              </FieldRow>
              <FieldRow
                label={t("upstreamConnector.authAwsSecretKey")}
                required={isCredRequired("secret_access_key")}
              >
                <PasswordInput
                  value={cred("secret_access_key")}
                  disabled={disabled}
                  aria-label={t("upstreamConnector.authAwsSecretKey")}
                  className={cn(
                    isCredRequired("secret_access_key") && invalidRing,
                  )}
                  onChange={(e) => setCred("secret_access_key", e.target.value)}
                />
              </FieldRow>
              <FieldRow
                label={t("upstreamConnector.authAwsSession")}
                required={isCredRequired("session_token")}
              >
                <PasswordInput
                  value={cred("session_token")}
                  disabled={disabled}
                  aria-label={t("upstreamConnector.authAwsSession")}
                  className={cn(
                    isCredRequired("session_token") && invalidRing,
                  )}
                  onChange={(e) => setCred("session_token", e.target.value)}
                />
              </FieldRow>
            </>
          )}

          {group === "hawk" && (
            <>
              <FieldRow label={t("upstreamConnector.authHawkId")}>
                <Input
                  value={cred("hawk_id")}
                  disabled={disabled}
                  aria-label={t("upstreamConnector.authHawkId")}
                  onChange={(e) => setCred("hawk_id", e.target.value)}
                />
              </FieldRow>
              <FieldRow
                label={t("upstreamConnector.authHawkKey")}
                required={isCredRequired("hawk_key")}
              >
                <PasswordInput
                  value={cred("hawk_key")}
                  disabled={disabled}
                  aria-label={t("upstreamConnector.authHawkKey")}
                  className={cn(isCredRequired("hawk_key") && invalidRing)}
                  onChange={(e) => setCred("hawk_key", e.target.value)}
                />
              </FieldRow>
              <FieldRow label={t("upstreamConnector.authHawkAlgo")}>
                <Select
                  value={flow.hawk?.algorithm || "sha256"}
                  options={hawkAlgoOptions}
                  disabled={disabled}
                  aria-label={t("upstreamConnector.authHawkAlgo")}
                  className="w-full"
                  onChange={(e) => {
                    const val = e.target.value;
                    onUpdateFlow((prev) => ({
                      ...prev,
                      hawk: {
                        ...(prev.hawk || { algorithm: "sha256", ext: "" }),
                        algorithm: val,
                      },
                    }));
                  }}
                />
              </FieldRow>
              <FieldRow label={t("upstreamConnector.authHawkExt")}>
                <Input
                  value={flow.hawk?.ext || ""}
                  disabled={disabled}
                  aria-label={t("upstreamConnector.authHawkExt")}
                  onChange={(e) => {
                    const val = e.target.value;
                    onUpdateFlow((prev) => ({
                      ...prev,
                      hawk: {
                        ...(prev.hawk || { algorithm: "sha256", ext: "" }),
                        ext: val,
                      },
                    }));
                  }}
                />
              </FieldRow>
            </>
          )}

          {(group === "ticket" || group === "jwt") && (
            <AuthInvalidationSection
              form={flow.invalidation || createEmptyInvalidation()}
              adapter={adapter}
              source={invalidationSource}
              disabled={disabled}
              onChange={(invalidation) =>
                onUpdateFlow((prev) => ({ ...prev, invalidation }))
              }
            />
          )}
        </div>
      )}

      <div className="rounded-lg border border-border bg-card overflow-hidden">
        <button
          type="button"
          onClick={() => setRawOpen((v) => !v)}
          className="flex w-full items-center justify-between gap-2 px-3 py-2 text-left text-xs font-medium text-foreground hover:bg-hover-bg cursor-pointer"
        >
          <span className="inline-flex items-center gap-1.5">
            {rawOpen ? (
              <ChevronDown className="h-3.5 w-3.5 text-muted-foreground" />
            ) : (
              <ChevronRight className="h-3.5 w-3.5 text-muted-foreground" />
            )}
            <span>{t("upstreamConnector.authVaultSection")}</span>
            {nonEmptyCredCount > 0 && (
              <Badge variant="xblue" className="font-mono-tech">
                {nonEmptyCredCount}
              </Badge>
            )}
          </span>
        </button>

        {rawOpen && (
          <div className="border-t border-border-subtle p-3">
            <WorkbenchKvRows
              rows={credRows}
              onChange={(rows) =>
                onUpdateFlow((prev) => ({
                  ...prev,
                  credentials: rows.map((r) => ({
                    key: r.key,
                    value: r.value,
                  })),
                }))
              }
              secretValues
              emptyCompact
              requiredKeys={requiredCredKeys}
              disabled={disabled}
              title={t("supplier.config.credentials")}
              addTitle={t("supplier.config.addCredential")}
              clearTitle={t("upstreamConnector.clearKv")}
              env={env}
            />
          </div>
        )}
      </div>
    </div>
  );
}
