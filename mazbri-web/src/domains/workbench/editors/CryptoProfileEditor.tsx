import { useMemo, useState } from "react";
import {
  AlertTriangle,
  ChevronDown,
  ChevronRight,
  Info,
  Plus,
  Trash2,
} from "lucide-react";
import {
  Button,
  IconButton,
  Input,
  Popover,
  Select,
  Switch,
  cn,
  type SelectOptionItem,
} from "@/components/ui";
import {
  securityClassLabelKey,
  type CryptoAlgorithmMeta,
} from "@/components/crypto/cryptoCatalog";
import { useT } from "@/locales";
import {
  CANON_STYLES,
  CIPHER_SCOPES,
  CRYPTO_ORDERS,
  CRYPTO_PROFILE_TEMPLATES,
  IV_SOURCES,
  SLOT_LOCATIONS,
  WIRE_ENCODING_KINDS,
  applyCryptoTemplate,
  applyOrderDefaults,
  dataOptionsForProfile,
  defaultCryptoProfile,
  kindToWireEncoding,
  macOptionsForProfile,
  parseCryptoProfile,
  serializeCryptoProfile,
  validateCryptoProfileClient,
  wireEncodingToKind,
  type CanonStyleId,
  type CipherScopeId,
  type CryptoOrderId,
  type CryptoProfileTemplateId,
  type IvSourceId,
  type SlotLocationId,
  type SlotSpecDraft,
  type UpstreamCryptoProfileDraft,
  type WireEncodingKind,
} from "../engine/cryptoProfileHelpers";

export interface CryptoProfileEditorProps {
  value: Record<string, unknown> | null | undefined;
  onChange: (next: Record<string, unknown> | null) => void;
  disabled?: boolean;
  canToggleInsecure?: boolean;
  credentialKeys?: string[];
}

function parseFieldsCsv(raw: string): string[] {
  return String(raw || "")
    .split(/[,;\s]+/)
    .map((s) => s.trim())
    .filter(Boolean);
}

export function CryptoProfileEditor({
  value,
  onChange,
  disabled = false,
  canToggleInsecure = false,
  credentialKeys = [],
}: CryptoProfileEditorProps) {
  const { t } = useT();
  const [infoOpen, setInfoOpen] = useState(false);
  const [responseOpen, setResponseOpen] = useState(false);

  const local = useMemo(() => parseCryptoProfile(value), [value]);
  const enabled = local !== null;

  const needsSign =
    local?.order === "sign_only" ||
    local?.order === "sign_then_encrypt" ||
    local?.order === "encrypt_then_sign";
  const needsCrypt =
    local?.order === "encrypt_only" ||
    local?.order === "aead_only" ||
    local?.order === "sign_then_encrypt" ||
    local?.order === "encrypt_then_sign";

  const signAlgorithms = useMemo(
    () => macOptionsForProfile(Boolean(local?.allow_insecure)),
    [local?.allow_insecure],
  );

  const cryptAlgorithms = useMemo(
    () =>
      dataOptionsForProfile(
        Boolean(local?.allow_insecure),
        local?.order === "aead_only",
      ),
    [local?.allow_insecure, local?.order],
  );

  const issues = useMemo(() => validateCryptoProfileClient(local), [local]);

  const credKeySet = useMemo(
    () => new Set(credentialKeys.map((k) => k.trim()).filter(Boolean)),
    [credentialKeys],
  );

  const boundSignSlot = useMemo(() => {
    const explicit = local?.sign?.key_slot?.trim();
    if (explicit && credKeySet.has(explicit)) return explicit;
    for (const candidate of [
      "crypto_mac_key",
      "mac_key",
      "client_secret",
      "app_secret",
      "secret",
    ]) {
      if (credKeySet.has(candidate)) return candidate;
    }
    return "";
  }, [local?.sign?.key_slot, credKeySet]);

  const boundCryptSlot = useMemo(() => {
    const explicit = local?.crypt?.key_slot?.trim();
    if (explicit && credKeySet.has(explicit)) return explicit;
    for (const candidate of ["crypto_data_key", "data_key"]) {
      if (credKeySet.has(candidate)) return candidate;
    }
    return "";
  }, [local?.crypt?.key_slot, credKeySet]);

  const emitProfile = (profile: UpstreamCryptoProfileDraft | null) => {
    onChange(serializeCryptoProfile(profile));
  };

  const patch = (partial: Partial<UpstreamCryptoProfileDraft>) => {
    if (!local) return;
    const next: UpstreamCryptoProfileDraft = { ...local, ...partial };
    applyOrderDefaults(next);
    emitProfile(next);
  };

  const orderOptions: SelectOptionItem[] = useMemo(
    () =>
      CRYPTO_ORDERS.map((ord) => {
        const label = t(`upstreamConnector.cryptoOrders.${ord}`);
        return { value: ord, label, text: label };
      }),
    [t],
  );

  const signSelectOptions: SelectOptionItem[] = useMemo(
    () => [
      {
        value: "",
        label: t("upstreamConnector.cryptoSelectAlg"),
        text: t("upstreamConnector.cryptoSelectAlg"),
      },
      ...signAlgorithms.map((alg: CryptoAlgorithmMeta) => {
        const label = `${alg.label} · ${t(securityClassLabelKey(alg.security))}`;
        return { value: alg.id, label, text: label };
      }),
    ],
    [signAlgorithms, t],
  );

  const cryptSelectOptions: SelectOptionItem[] = useMemo(
    () => [
      {
        value: "",
        label: t("upstreamConnector.cryptoSelectAlg"),
        text: t("upstreamConnector.cryptoSelectAlg"),
      },
      ...cryptAlgorithms.map((alg: CryptoAlgorithmMeta) => {
        const label = `${alg.label} · ${t(securityClassLabelKey(alg.security))}`;
        return { value: alg.id, label, text: label };
      }),
    ],
    [cryptAlgorithms, t],
  );

  const responseCryptSelectOptions: SelectOptionItem[] = useMemo(() => {
    const placeholder = local?.decrypt_response
      ? t("upstreamConnector.cryptoReuseRequestCipher")
      : t("upstreamConnector.cryptoSelectAlgOptional");
    return [
      { value: "", label: placeholder, text: placeholder },
      ...cryptAlgorithms.map((alg: CryptoAlgorithmMeta) => {
        const label = `${alg.label} · ${t(securityClassLabelKey(alg.security))}`;
        return { value: alg.id, label, text: label };
      }),
    ];
  }, [cryptAlgorithms, local?.decrypt_response, t]);

  const canonOptions: SelectOptionItem[] = useMemo(
    () =>
      CANON_STYLES.map((c) => {
        const label = t(`upstreamConnector.cryptoCanons.${c}`);
        return { value: c, label, text: label };
      }),
    [t],
  );

  const wireEncodingOptions: SelectOptionItem[] = useMemo(
    () =>
      WIRE_ENCODING_KINDS.map((k) => {
        const label = t(`upstreamConnector.cryptoWireEncodings.${k}`);
        return { value: k, label, text: label };
      }),
    [t],
  );

  const ivOptions: SelectOptionItem[] = useMemo(
    () =>
      IV_SOURCES.map((s) => {
        const label = t(`upstreamConnector.cryptoIvSources.${s}`);
        return { value: s, label, text: label };
      }),
    [t],
  );

  const scopeOptions: SelectOptionItem[] = useMemo(
    () =>
      CIPHER_SCOPES.map((s) => {
        const label = t(`upstreamConnector.cryptoScopes.${s}`);
        return { value: s, label, text: label };
      }),
    [t],
  );

  const slotLocationOptions: SelectOptionItem[] = useMemo(
    () => SLOT_LOCATIONS.map((loc) => ({ value: loc, label: loc, text: loc })),
    [],
  );

  const handleToggleEnabled = (nextEnabled: boolean) => {
    if (nextEnabled) {
      emitProfile(defaultCryptoProfile("aead_only"));
      return;
    }
    onChange(null);
  };

  const handleImportTemplate = (id: CryptoProfileTemplateId) => {
    const next = applyCryptoTemplate(id);
    if (!canToggleInsecure) next.allow_insecure = false;
    emitProfile(next);
  };

  const handleAllowInsecureChange = (allow: boolean) => {
    if (!local) return;
    const next: UpstreamCryptoProfileDraft = { ...local, allow_insecure: allow };
    if (!allow) {
      const signMeta = next.sign?.alg
        ? macOptionsForProfile(true).find((a) => a.id === next.sign?.alg)
        : undefined;
      const cryptMeta = next.crypt?.alg
        ? dataOptionsForProfile(true, false).find(
            (a) => a.id === next.crypt?.alg,
          )
        : undefined;
      if (signMeta?.legacy) next.sign = { alg: "hmac-sha256" };
      if (cryptMeta?.legacy) {
        next.crypt = {
          alg: next.order === "aead_only" ? "aes-256-gcm" : "aes-256-cbc",
          fields: next.crypt?.fields || [],
        };
      }
    }
    emitProfile(next);
  };

  const addSlot = () => {
    if (!local) return;
    const slot: SlotSpecDraft = { role: "sign", location: "form", name: "sign" };
    patch({ slots: [...local.slots, slot] });
  };

  const updateSlot = (idx: number, partial: Partial<SlotSpecDraft>) => {
    if (!local) return;
    const slots = local.slots.map((s, i) =>
      i === idx ? { ...s, ...partial } : s,
    );
    patch({ slots });
  };

  const removeSlot = (idx: number) => {
    if (!local) return;
    patch({ slots: local.slots.filter((_, i) => i !== idx) });
  };

  return (
    <div className={cn("flex w-full flex-col gap-3", disabled && "opacity-75")}>
      <div className="flex items-center gap-2">
        <Switch
          checked={enabled}
          disabled={disabled}
          aria-label={t("upstreamConnector.cryptoEnable")}
          onCheckedChange={handleToggleEnabled}
        />
        <span className="text-xs font-medium text-foreground">
          {t("upstreamConnector.cryptoEnable")}
        </span>
        <Popover
          open={infoOpen}
          onOpenChange={setInfoOpen}
          align="left"
          contentClassName="w-72 p-2.5"
          trigger={
            <button
              type="button"
              aria-label={t("upstreamConnector.cryptoAuthBoundaryHint")}
              onClick={() => setInfoOpen((v) => !v)}
              className="inline-flex h-6 w-6 items-center justify-center rounded-full text-muted-foreground hover:bg-hover-bg hover:text-primary cursor-pointer"
            >
              <Info className="h-3.5 w-3.5" />
            </button>
          }
        >
          <p className="text-xs leading-relaxed text-foreground">
            {t("upstreamConnector.cryptoAuthBoundaryHint")}
          </p>
        </Popover>
      </div>

      {enabled && local && (
        <div className="space-y-3 rounded-lg border border-border bg-card p-3">
          <div className="space-y-1.5">
            <div className="text-xs font-semibold text-foreground">
              {t("upstreamConnector.cryptoImportTemplate")}
            </div>
            <div className="flex flex-wrap gap-1.5">
              {CRYPTO_PROFILE_TEMPLATES.map((tpl) => (
                <Button
                  key={tpl.id}
                  type="button"
                  variant="outline"
                  size="sm"
                  disabled={disabled}
                  onClick={() => handleImportTemplate(tpl.id)}
                >
                  {t(tpl.labelKey)}
                </Button>
              ))}
            </div>
          </div>

          <div className="space-y-1">
            <label className="block text-xs font-semibold text-foreground">
              {t("upstreamConnector.cryptoOrder")}
            </label>
            <Select
              value={local.order}
              options={orderOptions}
              searchable
              disabled={disabled}
              aria-label={t("upstreamConnector.cryptoOrder")}
              className="w-full"
              onChange={(e) =>
                patch({ order: e.target.value as CryptoOrderId })
              }
            />
          </div>

          {needsSign && (
            <div className="space-y-1">
              <label className="block text-xs font-semibold text-foreground">
                {t("upstreamConnector.cryptoSignAlg")}
                <span className="ml-0.5 text-danger-fg">*</span>
              </label>
              <Select
                value={local.sign?.alg || ""}
                options={signSelectOptions}
                searchable
                disabled={disabled}
                aria-label={t("upstreamConnector.cryptoSignAlg")}
                className="w-full"
                onChange={(e) => {
                  const alg = e.target.value;
                  emitProfile({
                    ...local,
                    sign: alg ? { ...(local.sign || { alg: "" }), alg } : null,
                  });
                }}
              />
              <div
                className={cn(
                  "text-[11px]",
                  boundSignSlot ? "text-success-fg" : "text-warning-fg",
                )}
              >
                {boundSignSlot
                  ? t("upstreamConnector.cryptoBoundKeySlot", {
                      slot: boundSignSlot,
                    })
                  : t("upstreamConnector.cryptoUnboundKeySlot", {
                      slot: local.sign?.key_slot || "crypto_mac_key",
                    })}
              </div>
            </div>
          )}

          {needsCrypt && (
            <div className="space-y-1">
              <label className="block text-xs font-semibold text-foreground">
                {t("upstreamConnector.cryptoCryptAlg")}
                <span className="ml-0.5 text-danger-fg">*</span>
              </label>
              <Select
                value={local.crypt?.alg || ""}
                options={cryptSelectOptions}
                searchable
                disabled={disabled}
                aria-label={t("upstreamConnector.cryptoCryptAlg")}
                className="w-full"
                onChange={(e) => {
                  const alg = e.target.value;
                  emitProfile({
                    ...local,
                    crypt: alg
                      ? {
                          ...(local.crypt || { alg: "", fields: [] }),
                          alg,
                          fields: local.crypt?.fields || [],
                        }
                      : null,
                  });
                }}
              />
              <div
                className={cn(
                  "text-[11px]",
                  boundCryptSlot ? "text-success-fg" : "text-warning-fg",
                )}
              >
                {boundCryptSlot
                  ? t("upstreamConnector.cryptoBoundKeySlot", {
                      slot: boundCryptSlot,
                    })
                  : t("upstreamConnector.cryptoUnboundKeySlot", {
                      slot: local.crypt?.key_slot || "crypto_data_key",
                    })}
              </div>
            </div>
          )}

          <div className="space-y-1">
            <label className="block text-xs font-semibold text-foreground">
              {t("upstreamConnector.cryptoCanon")}
            </label>
            <Select
              value={local.canon}
              options={canonOptions}
              disabled={disabled}
              aria-label={t("upstreamConnector.cryptoCanon")}
              className="w-full"
              onChange={(e) =>
                patch({ canon: e.target.value as CanonStyleId })
              }
            />
          </div>

          <div className="grid grid-cols-1 sm:grid-cols-3 gap-2.5">
            <div className="space-y-1">
              <label className="block text-xs font-semibold text-foreground">
                {t("upstreamConnector.cryptoEncPayload")}
              </label>
              <Select
                value={wireEncodingToKind(local.encodings.payload)}
                options={wireEncodingOptions}
                disabled={disabled}
                aria-label={t("upstreamConnector.cryptoEncPayload")}
                className="w-full"
                onChange={(e) =>
                  patch({
                    encodings: {
                      ...local.encodings,
                      payload: kindToWireEncoding(
                        e.target.value as WireEncodingKind,
                      ),
                    },
                  })
                }
              />
            </div>

            <div className="space-y-1">
              <label className="block text-xs font-semibold text-foreground">
                {t("upstreamConnector.cryptoEncSignature")}
              </label>
              <Select
                value={wireEncodingToKind(local.encodings.signature)}
                options={wireEncodingOptions}
                disabled={disabled}
                aria-label={t("upstreamConnector.cryptoEncSignature")}
                className="w-full"
                onChange={(e) =>
                  patch({
                    encodings: {
                      ...local.encodings,
                      signature: kindToWireEncoding(
                        e.target.value as WireEncodingKind,
                      ),
                    },
                  })
                }
              />
            </div>

            <div className="space-y-1">
              <label className="block text-xs font-semibold text-foreground">
                {t("upstreamConnector.cryptoEncIv")}
              </label>
              <Select
                value={wireEncodingToKind(local.encodings.iv)}
                options={wireEncodingOptions}
                disabled={disabled}
                aria-label={t("upstreamConnector.cryptoEncIv")}
                className="w-full"
                onChange={(e) =>
                  patch({
                    encodings: {
                      ...local.encodings,
                      iv: kindToWireEncoding(
                        e.target.value as WireEncodingKind,
                      ),
                    },
                  })
                }
              />
            </div>
          </div>

          <div className="grid grid-cols-1 sm:grid-cols-2 gap-2.5">
            <div className="space-y-1">
              <label className="block text-xs font-semibold text-foreground">
                {t("upstreamConnector.cryptoIvSource")}
              </label>
              <Select
                value={local.iv}
                options={ivOptions}
                disabled={disabled}
                aria-label={t("upstreamConnector.cryptoIvSource")}
                className="w-full"
                onChange={(e) => patch({ iv: e.target.value as IvSourceId })}
              />
            </div>

            <div className="space-y-1">
              <label className="block text-xs font-semibold text-foreground">
                {t("upstreamConnector.cryptoScope")}
              </label>
              <Select
                value={local.scope}
                options={scopeOptions}
                disabled={disabled}
                aria-label={t("upstreamConnector.cryptoScope")}
                className="w-full"
                onChange={(e) =>
                  patch({ scope: e.target.value as CipherScopeId })
                }
              />
            </div>
          </div>

          {local.scope === "named_fields" && needsCrypt && (
            <div className="space-y-1">
              <label className="block text-xs font-semibold text-foreground">
                {t("upstreamConnector.cryptoCryptFields")}
              </label>
              <Input
                value={(local.crypt?.fields || []).join(", ")}
                disabled={disabled}
                placeholder={t("upstreamConnector.cryptoFieldsPlaceholder")}
                aria-label={t("upstreamConnector.cryptoCryptFields")}
                className="font-mono-tech"
                onChange={(e) => {
                  if (!local.crypt) return;
                  emitProfile({
                    ...local,
                    crypt: {
                      ...local.crypt,
                      fields: parseFieldsCsv(e.target.value),
                    },
                  });
                }}
              />
            </div>
          )}

          <div className="space-y-1.5">
            <div className="flex items-center justify-between gap-2">
              <span className="text-xs font-semibold text-foreground">
                {t("upstreamConnector.cryptoSlots")}
              </span>
              <Button
                type="button"
                variant="link"
                size="xs"
                disabled={disabled}
                onClick={addSlot}
              >
                <Plus className="h-3.5 w-3.5" />
                {t("upstreamConnector.cryptoAddSlot")}
              </Button>
            </div>
            {!local.slots.length ? (
              <div className="text-xs text-muted-foreground">
                {t("upstreamConnector.cryptoSlotsEmpty")}
              </div>
            ) : (
              <div className="space-y-1.5">
                {local.slots.map((slot, idx) => (
                  <div
                    key={idx}
                    className="grid grid-cols-1 sm:grid-cols-[minmax(0,1fr)_120px_minmax(0,1fr)_28px] items-center gap-1.5"
                  >
                    <Input
                      value={slot.role}
                      disabled={disabled}
                      placeholder={t("upstreamConnector.cryptoSlotRole")}
                      aria-label={t("upstreamConnector.cryptoSlotRole")}
                      className="font-mono-tech"
                      onChange={(e) =>
                        updateSlot(idx, { role: e.target.value })
                      }
                    />
                    <Select
                      value={slot.location}
                      options={slotLocationOptions}
                      disabled={disabled}
                      aria-label={t("upstreamConnector.cryptoSlots")}
                      className="w-full"
                      onChange={(e) =>
                        updateSlot(idx, {
                          location: e.target.value as SlotLocationId,
                        })
                      }
                    />
                    <Input
                      value={slot.name}
                      disabled={disabled}
                      placeholder={t("upstreamConnector.cryptoSlotName")}
                      aria-label={t("upstreamConnector.cryptoSlotName")}
                      className="font-mono-tech"
                      onChange={(e) =>
                        updateSlot(idx, { name: e.target.value })
                      }
                    />
                    <IconButton
                      size="sm"
                      disabled={disabled}
                      aria-label={t("common.actions.delete")}
                      title={t("common.actions.delete")}
                      onClick={() => removeSlot(idx)}
                      className="text-danger-fg hover:text-danger-fg hover:bg-danger-fg/10"
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </IconButton>
                  </div>
                ))}
              </div>
            )}
          </div>

          <div className="space-y-1.5">
            <div className="text-xs font-semibold text-foreground">
              {t("upstreamConnector.cryptoDirection")}
            </div>
            <div className="flex flex-wrap items-center gap-4">
              {needsSign && (
                <label className="inline-flex items-center gap-2 text-xs text-foreground cursor-pointer select-none">
                  <Switch
                    checked={Boolean(local.verify_response)}
                    disabled={disabled || !local.sign?.alg}
                    aria-label={t("upstreamConnector.cryptoVerifyResponse")}
                    onCheckedChange={(val) => patch({ verify_response: val })}
                  />
                  <span>{t("upstreamConnector.cryptoVerifyResponse")}</span>
                </label>
              )}
              <label className="inline-flex items-center gap-2 text-xs text-foreground cursor-pointer select-none">
                <Switch
                  checked={Boolean(local.decrypt_response)}
                  disabled={
                    disabled || (!needsCrypt && !local.response_crypt?.alg)
                  }
                  aria-label={t("upstreamConnector.cryptoDecryptResponse")}
                  onCheckedChange={(val) =>
                    patch({
                      decrypt_response: val,
                      ...(val ? {} : { response_crypt: null }),
                    })
                  }
                />
                <span>{t("upstreamConnector.cryptoDecryptResponse")}</span>
              </label>
            </div>
          </div>

          <div className="rounded-lg border border-border-subtle overflow-hidden">
            <button
              type="button"
              onClick={() => setResponseOpen((v) => !v)}
              className="flex w-full items-center gap-1.5 bg-muted/35 px-3 py-2 text-left text-xs font-medium text-foreground hover:bg-hover-bg cursor-pointer"
            >
              {responseOpen ? (
                <ChevronDown className="h-3.5 w-3.5 text-muted-foreground" />
              ) : (
                <ChevronRight className="h-3.5 w-3.5 text-muted-foreground" />
              )}
              <span>{t("upstreamConnector.cryptoResponseCrypt")}</span>
            </button>
            {responseOpen && (
              <div className="space-y-2.5 border-t border-border-subtle p-3">
                <div className="space-y-1">
                  <label className="block text-xs font-semibold text-foreground">
                    {t("upstreamConnector.cryptoCryptAlg")}
                  </label>
                  <Select
                    value={local.response_crypt?.alg || ""}
                    options={responseCryptSelectOptions}
                    searchable
                    disabled={disabled}
                    aria-label={t("upstreamConnector.cryptoResponseCrypt")}
                    className="w-full"
                    onChange={(e) => {
                      const alg = e.target.value;
                      emitProfile({
                        ...local,
                        decrypt_response: alg
                          ? true
                          : Boolean(local.decrypt_response),
                        response_crypt: alg
                          ? {
                              ...(local.response_crypt || {
                                alg: "",
                                fields: [],
                              }),
                              alg,
                              fields: local.response_crypt?.fields || [],
                            }
                          : null,
                      });
                    }}
                  />
                </div>
                {local.scope === "named_fields" &&
                  local.response_crypt?.alg && (
                    <div className="space-y-1">
                      <label className="block text-xs font-semibold text-foreground">
                        {t("upstreamConnector.cryptoResponseFields")}
                      </label>
                      <Input
                        value={(local.response_crypt?.fields || []).join(", ")}
                        disabled={disabled}
                        placeholder={t(
                          "upstreamConnector.cryptoFieldsPlaceholder",
                        )}
                        aria-label={t("upstreamConnector.cryptoResponseFields")}
                        className="font-mono-tech"
                        onChange={(e) => {
                          if (!local.response_crypt) return;
                          emitProfile({
                            ...local,
                            response_crypt: {
                              ...local.response_crypt,
                              fields: parseFieldsCsv(e.target.value),
                            },
                          });
                        }}
                      />
                    </div>
                  )}
              </div>
            )}
          </div>

          <div className="space-y-1">
            <label className="inline-flex items-center gap-2 text-xs text-foreground cursor-pointer select-none">
              <Switch
                checked={local.allow_insecure}
                disabled={disabled || !canToggleInsecure}
                aria-label={t("upstreamConnector.cryptoAllowInsecure")}
                onCheckedChange={handleAllowInsecureChange}
              />
              <span>{t("upstreamConnector.cryptoAllowInsecure")}</span>
            </label>
            <p className="text-[11px] text-warning-fg">
              {canToggleInsecure
                ? t("upstreamConnector.cryptoAllowInsecureHint")
                : t("upstreamConnector.cryptoAllowInsecureAdminOnly")}
            </p>
          </div>

          {issues.length > 0 && (
            <div className="rounded-lg border border-warning-border bg-warning-bg p-3 space-y-1">
              <div className="flex items-center gap-1.5 text-xs font-semibold text-warning-fg">
                <AlertTriangle className="h-3.5 w-3.5 shrink-0" />
                <span>{t("upstreamConnector.cryptoValidationTitle")}</span>
              </div>
              <ul className="list-disc pl-5 text-xs text-warning-fg space-y-0.5">
                {issues.map((issue, idx) => (
                  <li key={`${issue.code}-${idx}`}>{issue.message}</li>
                ))}
              </ul>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
