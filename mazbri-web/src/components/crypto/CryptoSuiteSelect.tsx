import React, { useEffect, useMemo, useState } from "react";
import { AlertTriangle } from "lucide-react";
import {
  Badge,
  FormField,
  SegmentedControl,
  Select,
  Switch,
  type SelectOptionItem,
} from "@/components/ui";
import { useT } from "@/locales";
import {
  CRYPTO_SUITES,
  dataAlgorithmOptions,
  findAlgorithm,
  findSuite,
  macAlgorithmOptions,
  securityClassLabelKey,
  type CryptoAlgorithmMeta,
  type CryptoSuiteMeta,
  type SecurityClass,
} from "./cryptoCatalog";
import {
  applySelectionModeChange,
  detectLegacyAlgorithms,
  resolveInitialSelectionMode,
  type CryptoSelectionMode,
  type CryptoSuiteSelectValue,
} from "./cryptoSuiteState";

export type { CryptoSuiteSelectValue };

export interface CryptoSuiteSelectProps {
  value: CryptoSuiteSelectValue;
  onChange: (next: CryptoSuiteSelectValue) => void;
  disabled?: boolean;
  /** 仅超级管理员可打开 allow_insecure */
  canToggleInsecure?: boolean;
}

function securityBadgeVariant(
  security: SecurityClass,
): "success" | "xblue" | "danger" | "neutral" {
  if (security === "recommended") return "success";
  if (security === "gm") return "xblue";
  if (security === "legacy") return "danger";
  return "neutral";
}

export function CryptoSuiteSelect({
  value,
  onChange,
  disabled = false,
  canToggleInsecure = false,
}: CryptoSuiteSelectProps) {
  const { t } = useT();
  const [selectionMode, setSelectionMode] = useState<CryptoSelectionMode>(() =>
    resolveInitialSelectionMode(value),
  );

  useEffect(() => {
    if (value.suite) {
      setSelectionMode("suite");
    } else if (value.dataAlg || value.macAlg) {
      setSelectionMode("custom");
    }
  }, [value.suite, value.dataAlg, value.macAlg]);

  const legacyAlgorithms = useMemo(
    () => detectLegacyAlgorithms(value),
    [value],
  );
  const dataOptions = useMemo(
    () => dataAlgorithmOptions(value.allowInsecure),
    [value.allowInsecure],
  );
  const macOptions = useMemo(
    () => macAlgorithmOptions(value.allowInsecure),
    [value.allowInsecure],
  );
  const selectedSuite = useMemo(() => findSuite(value.suite), [value.suite]);

  const selectedSuiteHint = useMemo(() => {
    if (!selectedSuite) return "";
    const mac =
      selectedSuite.macAlg ?? t("tenant.transportEncryption.hints.noMac");
    return t("tenant.transportEncryption.hints.suiteFollows", {
      data: selectedSuite.dataAlg,
      mac,
    });
  }, [selectedSuite, t]);

  const macRequired = useMemo(() => {
    const alg =
      findAlgorithm(value.dataAlg) ??
      (selectedSuite ? findAlgorithm(selectedSuite.dataAlg) : undefined);
    return Boolean(alg?.requireMac);
  }, [value.dataAlg, selectedSuite]);

  const patch = (partial: Partial<CryptoSuiteSelectValue>) => {
    onChange({ ...value, ...partial });
  };

  const handleModeChange = (nextMode: CryptoSelectionMode) => {
    setSelectionMode(nextMode);
    onChange(applySelectionModeChange(value, nextMode));
  };

  const handleSuiteChange = (nextSuiteId: string) => {
    const suiteId = nextSuiteId || null;
    if (!suiteId) {
      patch({ suite: null });
      return;
    }
    patch({ suite: suiteId, dataAlg: null, macAlg: null });
  };

  const handleDataAlgChange = (nextDataAlg: string) => {
    const dataAlg = nextDataAlg || null;
    const meta = findAlgorithm(dataAlg);
    patch({
      suite: null,
      dataAlg,
      macAlg: meta?.requireMac ? value.macAlg : null,
    });
  };

  const handleMacAlgChange = (nextMacAlg: string) => {
    patch({
      suite: null,
      macAlg: nextMacAlg || null,
    });
  };

  const handleAllowInsecureChange = (allow: boolean) => {
    if (!allow) {
      const data = findAlgorithm(value.dataAlg);
      const mac = findAlgorithm(value.macAlg);
      patch({
        allowInsecure: false,
        dataAlg: data?.legacy ? null : value.dataAlg,
        macAlg: mac?.legacy ? null : value.macAlg,
      });
      return;
    }
    patch({ allowInsecure: true });
  };

  const suiteSelectOptions = useMemo<SelectOptionItem[]>(
    () =>
      CRYPTO_SUITES.map((suite: CryptoSuiteMeta) => ({
        value: suite.id,
        text: `${t(suite.labelKey)} (${suite.dataAlg}${suite.macAlg ? ` + ${suite.macAlg}` : ""})`,
        label: (
          <span className="inline-flex w-full items-center justify-between gap-2">
            <span className="truncate">{t(suite.labelKey)}</span>
            <Badge variant={securityBadgeVariant(suite.security)}>
              {t(securityClassLabelKey(suite.security))}
            </Badge>
          </span>
        ),
      })),
    [t],
  );

  const dataSelectOptions = useMemo<SelectOptionItem[]>(
    () =>
      dataOptions.map((alg: CryptoAlgorithmMeta) => ({
        value: alg.id,
        text: `${alg.label} · ${t(securityClassLabelKey(alg.security))}`,
        label: (
          <span className="inline-flex w-full items-center justify-between gap-2">
            <span className="font-mono-tech">{alg.label}</span>
            <Badge variant={securityBadgeVariant(alg.security)}>
              {t(securityClassLabelKey(alg.security))}
            </Badge>
          </span>
        ),
      })),
    [dataOptions, t],
  );

  const macSelectOptions = useMemo<SelectOptionItem[]>(
    () => [
      {
        value: "",
        text: t("tenant.transportEncryption.hints.noMac"),
        label: (
          <span className="text-muted-foreground">
            {t("tenant.transportEncryption.hints.noMac")}
          </span>
        ),
      },
      ...macOptions.map((alg: CryptoAlgorithmMeta) => ({
        value: alg.id,
        text: `${alg.label} · ${t(securityClassLabelKey(alg.security))}`,
        label: (
          <span className="inline-flex w-full items-center justify-between gap-2">
            <span className="font-mono-tech">{alg.label}</span>
            <Badge variant={securityBadgeVariant(alg.security)}>
              {t(securityClassLabelKey(alg.security))}
            </Badge>
          </span>
        ),
      })),
    ],
    [macOptions, t],
  );

  return (
    <div className={`space-y-3 ${disabled ? "opacity-75" : ""}`}>
      <SegmentedControl<CryptoSelectionMode>
        value={selectionMode}
        onChange={handleModeChange}
        items={[
          {
            key: "suite",
            label: t("tenant.transportEncryption.modes.suite"),
            disabled,
          },
          {
            key: "custom",
            label: t("tenant.transportEncryption.modes.custom"),
            disabled,
          },
        ]}
      />

      {selectionMode === "suite" ? (
        <FormField
          label={t("tenant.transportEncryption.modes.suite")}
          hint={selectedSuiteHint || undefined}
        >
          <Select
            value={value.suite ?? ""}
            onChange={(e) => handleSuiteChange(e.target.value)}
            disabled={disabled}
            placeholder={t("tenant.transportEncryption.placeholders.selectSuite")}
            options={suiteSelectOptions}
            className="w-full"
          />
        </FormField>
      ) : (
        <div className="space-y-3">
          <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
            <FormField label={t("tenant.transportEncryption.fields.dataAlg")}>
              <Select
                value={value.dataAlg ?? ""}
                onChange={(e) => handleDataAlgChange(e.target.value)}
                disabled={disabled}
                placeholder={t(
                  "tenant.transportEncryption.placeholders.selectDataAlg",
                )}
                options={dataSelectOptions}
                className="w-full"
              />
            </FormField>
            <FormField
              label={t("tenant.transportEncryption.fields.macAlg")}
              required={macRequired}
              hint={
                macRequired
                  ? t("tenant.transportEncryption.hints.macRequired")
                  : undefined
              }
            >
              <Select
                value={value.macAlg ?? ""}
                onChange={(e) => handleMacAlgChange(e.target.value)}
                disabled={disabled}
                placeholder={t(
                  "tenant.transportEncryption.placeholders.selectMacAlg",
                )}
                options={macSelectOptions}
                className="w-full"
              />
            </FormField>
          </div>

          {legacyAlgorithms.length > 0 && (
            <div className="flex items-start gap-2 rounded-md border border-warning-border bg-warning-bg px-3 py-2 text-xs text-warning-fg">
              <AlertTriangle className="h-4 w-4 shrink-0 mt-0.5" />
              <span>
                {t("tenant.transportEncryption.hints.legacyRisk", {
                  algorithms: legacyAlgorithms.map((item) => item.label).join(", "),
                })}
              </span>
            </div>
          )}
        </div>
      )}

      <div className="space-y-1">
        <div className="flex items-center gap-2.5">
          <Switch
            checked={value.allowInsecure}
            disabled={disabled || !canToggleInsecure}
            onCheckedChange={handleAllowInsecureChange}
            aria-label={t("tenant.transportEncryption.fields.allowInsecure")}
          />
          <span className="text-xs font-medium text-foreground">
            {t("tenant.transportEncryption.fields.allowInsecure")}
          </span>
        </div>
        <p className="text-[11px] text-warning-fg leading-snug">
          {canToggleInsecure
            ? t("tenant.transportEncryption.hints.allowInsecure")
            : t("tenant.transportEncryption.hints.allowInsecureAdminOnly")}
        </p>
      </div>
    </div>
  );
}
