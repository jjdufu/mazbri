import { useEffect, useMemo, useState } from "react";
import { AlertCircle, Plus, X } from "lucide-react";
import {
  createConfig,
  updateConfig,
  validateToml,
} from "@/api/config-center";
import { fetchCanonicalServiceNames } from "@/api/service-names";
import {
  Badge,
  Button,
  CodeEditor,
  FormDrawer,
  FormField,
  Input,
  Select,
  Switch,
  Tabs,
} from "@/components/ui";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import type { ConfigEntry } from "@/types/api/config-center";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { logger } from "@/utils/sys/logger";
import {
  showSuccessToast,
  showWarningToast,
} from "@/utils/ui/message-guard";
import { parseGatewayCors, serializeGatewayCors } from "./helpers";

type GatewayTab = "toml" | "visual";

export interface ConfigEditDrawerProps {
  open: boolean;
  /** Non-null means edit mode with pre-fetched ConfigEntry (U1 MFA-safe). */
  config: ConfigEntry | null;
  existingNames: readonly string[];
  authorizationDisabled: boolean;
  authorizationDisabledReason: string;
  onClose: () => void;
  onSuccess: () => void;
}

export function ConfigEditDrawer({
  open,
  config,
  existingNames,
  authorizationDisabled,
  authorizationDisabledReason,
  onClose,
  onSuccess,
}: ConfigEditDrawerProps) {
  const { t } = useT();
  const currentUsername = useAuthStore(
    (s) => s.info.username || s.info.userName || "admin",
  );
  const isEdit = config !== null;

  const [canonicalServices, setCanonicalServices] = useState<string[]>([]);
  const [globalName, setGlobalName] = useState("global");

  const [serviceName, setServiceName] = useState("");
  const [customServiceMode, setCustomServiceMode] = useState(false);
  const [content, setContent] = useState("");
  const [activeTab, setActiveTab] = useState<GatewayTab>("toml");

  const [allowedOrigins, setAllowedOrigins] = useState<string[]>([]);
  const [allowCredentials, setAllowCredentials] = useState(true);
  const [originDraft, setOriginDraft] = useState("");

  const [serviceError, setServiceError] = useState<string | undefined>(
    undefined,
  );
  const [contentError, setContentError] = useState<string | undefined>(
    undefined,
  );
  const [tomlError, setTomlError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!open || isEdit) return;
    let active = true;
    fetchCanonicalServiceNames()
      .then((data) => {
        if (!active) return;
        setCanonicalServices(data.services ?? []);
        setGlobalName(data.global || "global");
      })
      .catch((error) => {
        logger.error("Failed to load canonical service names:", error);
      });
    return () => {
      active = false;
    };
  }, [open, isEdit]);

  useEffect(() => {
    if (!open) {
      setServiceError(undefined);
      setContentError(undefined);
      setTomlError(null);
      setOriginDraft("");
      setCustomServiceMode(false);
      return;
    }
    setActiveTab("toml");
    setServiceError(undefined);
    setContentError(undefined);
    setTomlError(null);
    setOriginDraft("");
    setCustomServiceMode(false);

    if (config) {
      setServiceName(config.service_name);
      setContent(config.content);
      if (config.service_name === "mazbri-gateway") {
        const parsed = parseGatewayCors(config.content);
        setAllowedOrigins(parsed.allowed_origins);
        setAllowCredentials(parsed.allow_credentials);
      } else {
        setAllowedOrigins([]);
        setAllowCredentials(true);
      }
    } else {
      setServiceName("");
      setContent("");
      setAllowedOrigins([]);
      setAllowCredentials(true);
    }
  }, [open, config]);

  const existingSet = useMemo(() => new Set(existingNames), [existingNames]);

  const serviceOptions = useMemo(() => {
    const names = [globalName, ...canonicalServices];
    return names.map((name) => {
      const label =
        name === globalName
          ? t("configCenter.serviceNameSelector.global")
          : name;
      return {
        value: name,
        label,
        text: `${label} ${name}`,
        disabled: existingSet.has(name),
      };
    });
  }, [globalName, canonicalServices, existingSet, t]);

  const isGateway = serviceName.trim() === "mazbri-gateway";

  const handleTabChange = (nextTab: GatewayTab) => {
    if (!isGateway) return;
    if (nextTab === "visual") {
      const parsed = parseGatewayCors(content);
      setAllowedOrigins(parsed.allowed_origins);
      setAllowCredentials(parsed.allow_credentials);
    } else {
      const nextContent = serializeGatewayCors(
        content,
        allowedOrigins,
        allowCredentials,
      );
      setContent(nextContent);
    }
    setActiveTab(nextTab);
  };

  const syncCorsToContent = (nextOrigins: string[], nextCreds: boolean) => {
    setAllowedOrigins(nextOrigins);
    setAllowCredentials(nextCreds);
    if (isGateway) {
      setContent((prev) => serializeGatewayCors(prev, nextOrigins, nextCreds));
    }
  };

  const handleAddOrigin = () => {
    const trimmed = originDraft.trim();
    if (!trimmed) return;
    if (!allowedOrigins.includes(trimmed)) {
      syncCorsToContent([...allowedOrigins, trimmed], allowCredentials);
    }
    setOriginDraft("");
  };

  const handleRemoveOrigin = (origin: string) => {
    syncCorsToContent(
      allowedOrigins.filter((item) => item !== origin),
      allowCredentials,
    );
  };

  const handleContentChange = (next: string) => {
    setContent(next);
    setContentError(undefined);
    setTomlError(null);
    if (isGateway) {
      const parsed = parseGatewayCors(next);
      setAllowedOrigins(parsed.allowed_origins);
      setAllowCredentials(parsed.allow_credentials);
    }
  };

  const handleSubmit = async () => {
    if (authorizationDisabled) {
      showWarningToast(authorizationDisabledReason);
      return;
    }
    if (saving) return;

    const finalService = serviceName.trim();
    const finalContent =
      isGateway && activeTab === "visual"
        ? serializeGatewayCors(content, allowedOrigins, allowCredentials)
        : content;

    let valid = true;
    if (!finalService) {
      setServiceError(
        t("configCenter.editDialog.validation.serviceNameRequired"),
      );
      valid = false;
    }
    if (!finalContent.trim()) {
      setContentError(t("configCenter.editDialog.validation.contentRequired"));
      valid = false;
    }
    if (!valid) return;

    setSaving(true);
    setTomlError(null);
    try {
      const validation = await validateToml(finalContent);
      if (!validation.valid && validation.error) {
        const { line, column, message } = validation.error;
        setTomlError(
          t("configCenter.editDialog.validation.tomlError", {
            line,
            column,
            message,
          }),
        );
        if (isGateway) setActiveTab("toml");
        return;
      }

      if (isEdit && config) {
        await updateConfig(config.service_name, {
          content: finalContent,
          updated_by: currentUsername,
        });
      } else {
        await createConfig({
          service_name: finalService,
          content: finalContent,
          updated_by: currentUsername,
        });
      }
      showSuccessToast(t("configCenter.editDialog.messages.saveSuccess"));
      onClose();
      onSuccess();
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        logger.error("Failed to save config:", error);
      }
    } finally {
      setSaving(false);
    }
  };

  return (
    <FormDrawer
      open={open}
      onClose={onClose}
      onSubmit={handleSubmit}
      size="lg"
      title={
        isEdit
          ? t("configCenter.editDialog.actions.edit")
          : t("configCenter.editDialog.actions.create")
      }
      subtitle={isEdit ? config?.service_name : undefined}
      submitLabel={t("common.actions.save")}
      cancelLabel={t("common.cancel")}
      submitting={saving}
      submitDisabled={authorizationDisabled}
      testId="config-edit-drawer"
    >
      {isEdit ? (
        <FormField label={t("configCenter.editDialog.fields.serviceName")}>
          <Input value={config?.service_name ?? ""} disabled />
        </FormField>
      ) : (
        <FormField
          label={t("configCenter.serviceNameSelector.label")}
          required
          error={serviceError}
        >
          <div className="space-y-2">
            {customServiceMode ? (
              <Input
                value={serviceName}
                onChange={(e) => {
                  setServiceName(e.target.value);
                  setServiceError(undefined);
                }}
                placeholder="custom-service"
              />
            ) : (
              <Select
                value={serviceName}
                onChange={(e) => {
                  const val = e.target.value;
                  setServiceName(val);
                  setServiceError(undefined);
                  setActiveTab("toml");
                  if (val === "mazbri-gateway") {
                    const parsed = parseGatewayCors(content);
                    setAllowedOrigins(parsed.allowed_origins);
                    setAllowCredentials(parsed.allow_credentials);
                  }
                }}
                searchable
                placeholder={t("configCenter.serviceNameSelector.placeholder")}
                options={serviceOptions}
                className="w-full"
              />
            )}
            <div className="flex items-center justify-end">
              <Button
                type="button"
                variant="link"
                size="xs"
                onClick={() => {
                  setCustomServiceMode((v) => !v);
                  setServiceName("");
                  setServiceError(undefined);
                }}
              >
                {customServiceMode
                  ? t("configCenter.serviceNameSelector.placeholder")
                  : t("common.actions.create")}
              </Button>
            </div>
          </div>
        </FormField>
      )}

      {isGateway && (
        <Tabs<GatewayTab>
          value={activeTab}
          onChange={handleTabChange}
          items={[
            { key: "toml", label: t("configCenter.editDialog.tabs.toml") },
            { key: "visual", label: t("configCenter.editDialog.tabs.cors") },
          ]}
        />
      )}

      {(!isGateway || activeTab === "toml") && (
        <FormField
          label={t("configCenter.editDialog.fields.content")}
          required
          error={contentError}
        >
          {tomlError && (
            <div className="mb-2 flex items-start gap-2 rounded-md border border-danger-fg/40 bg-danger-fg/10 px-3 py-2 text-xs text-danger-fg">
              <AlertCircle className="h-4 w-4 shrink-0 mt-0.5" />
              <span className="font-mono-tech break-all">{tomlError}</span>
            </div>
          )}
          <CodeEditor
            value={content}
            onChange={handleContentChange}
            language="toml"
            minHeight={420}
            maxHeight="calc(100vh - 300px)"
            testId="config-toml-editor"
          />
        </FormField>
      )}

      {isGateway && activeTab === "visual" && (
        <div className="space-y-4 pt-1">
          <FormField
            label={t("configCenter.editDialog.cors.allowedOrigins")}
            hint={t("configCenter.editDialog.cors.allowedOriginsTip")}
          >
            <div className="space-y-2.5">
              {allowedOrigins.length > 0 && (
                <div className="flex flex-wrap gap-1.5">
                  {allowedOrigins.map((origin) => (
                    <Badge key={origin} variant="xblue" className="gap-1.5 pr-1">
                      <span className="font-mono-tech">{origin}</span>
                      <button
                        type="button"
                        onClick={() => handleRemoveOrigin(origin)}
                        aria-label={t("common.actions.delete")}
                        className="inline-flex h-4 w-4 items-center justify-center rounded-full hover:bg-primary/20 cursor-pointer"
                      >
                        <X className="h-2.5 w-2.5" />
                      </button>
                    </Badge>
                  ))}
                </div>
              )}
              <div className="flex items-center gap-2">
                <Input
                  value={originDraft}
                  onChange={(e) => setOriginDraft(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.preventDefault();
                      handleAddOrigin();
                    }
                  }}
                  placeholder="https://console.example.com"
                  className="max-w-sm font-mono-tech"
                />
                <Button
                  type="button"
                  variant="outline"
                  size="md"
                  onClick={handleAddOrigin}
                >
                  <Plus className="h-3.5 w-3.5" />
                  {t("common.actions.add")}
                </Button>
              </div>
            </div>
          </FormField>

          <div className="flex items-center justify-between gap-4 rounded-lg border border-border bg-card px-3.5 py-2.5">
            <span className="text-xs font-medium text-foreground">
              {t("configCenter.editDialog.cors.allowCredentials")}
            </span>
            <Switch
              checked={allowCredentials}
              onCheckedChange={(checked) =>
                syncCorsToContent(allowedOrigins, checked)
              }
              aria-label={t("configCenter.editDialog.cors.allowCredentials")}
            />
          </div>
        </div>
      )}
    </FormDrawer>
  );
}
