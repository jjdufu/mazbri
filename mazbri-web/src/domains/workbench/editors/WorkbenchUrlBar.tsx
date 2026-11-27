import React, { useEffect, useRef, useState } from "react";
import { ChevronDown, Send, X } from "lucide-react";
import { Button, Popover, Tooltip, cn } from "@/components/ui";
import { useT } from "@/locales";
import {
  HTTP_METHODS,
  HTTP_METHOD_PRESETS,
  displayMethodBadge,
  methodColorVar,
  sanitizeHttpMethod,
  tryParseCurlPaste,
  type ParsedImport,
} from "../engine/workbenchHelpers";
import {
  WorkbenchEnvText,
  type WorkbenchEnvTextHandle,
} from "./WorkbenchEnvText";

export interface WorkbenchUrlBarProps {
  method: string;
  urlDraft: string;
  urlMissing: boolean;
  running: boolean;
  canWrite: boolean;
  hasLinkedSupplier: boolean;
  env: Record<string, string>;
  unresolved: string[];
  staticCaptureWarnings: string[];
  urlInputRef?: React.RefObject<WorkbenchEnvTextHandle | null>;
  onMethodChange: (method: string) => void;
  onUrlFocus: () => void;
  onUrlInput: (raw: string) => void;
  onUrlBlur: () => void;
  onCurlPaste: (parsed: ParsedImport) => void;
  onSend: () => void;
  onCancel: () => void;
  onLiveRun: () => void;
  onRunAuth: () => void;
  onTransformPreview: () => void;
  onCopyCurl: () => void;
}

export function WorkbenchUrlBar({
  method,
  urlDraft,
  urlMissing,
  running,
  canWrite,
  hasLinkedSupplier,
  env,
  unresolved,
  staticCaptureWarnings,
  urlInputRef,
  onMethodChange,
  onUrlFocus,
  onUrlInput,
  onUrlBlur,
  onCurlPaste,
  onSend,
  onCancel,
  onLiveRun,
  onRunAuth,
  onTransformPreview,
  onCopyCurl,
}: WorkbenchUrlBarProps) {
  const { t } = useT();
  const [methodOpen, setMethodOpen] = useState(false);
  const [sendMenuOpen, setSendMenuOpen] = useState(false);
  const customInputRef = useRef<HTMLInputElement | null>(null);

  const upperMethod = (method || "GET").toUpperCase();
  const isCustomMethod =
    upperMethod === "CUSTOM" ||
    !(HTTP_METHOD_PRESETS as readonly string[]).includes(upperMethod);

  const [customDraft, setCustomDraft] = useState(() =>
    isCustomMethod && upperMethod !== "CUSTOM" ? upperMethod.slice(0, 16) : "",
  );

  useEffect(() => {
    if (isCustomMethod && upperMethod !== "CUSTOM") {
      setCustomDraft(upperMethod.slice(0, 16));
    }
  }, [isCustomMethod, upperMethod]);

  const shownMethod = displayMethodBadge(method);

  const handlePickMethod = (next: string) => {
    setMethodOpen(false);
    if (next === "CUSTOM") {
      onMethodChange("CUSTOM");
      setCustomDraft("");
      requestAnimationFrame(() => customInputRef.current?.focus());
      return;
    }
    onMethodChange(next);
  };

  const handleCustomTyped = (e: React.ChangeEvent<HTMLInputElement>) => {
    const raw = e.target.value.toUpperCase().slice(0, 16);
    setCustomDraft(raw);
    if (!raw) return;
    const valid = sanitizeHttpMethod(raw);
    if (valid) onMethodChange(valid);
  };

  const handlePaste = (ev: React.ClipboardEvent<HTMLInputElement>) => {
    const text = ev.clipboardData?.getData("text") ?? "";
    const parsed = tryParseCurlPaste(text);
    if (!parsed) return;
    ev.preventDefault();
    onCurlPaste(parsed);
  };

  return (
    <div className="flex flex-col gap-1.5 border-b border-border-subtle bg-card px-3 py-2 shrink-0">
      <div className="flex items-center gap-2">
        <div
          className={cn(
            "flex h-8 min-w-0 flex-1 items-center rounded-md border bg-card transition-colors focus-within:border-primary focus-within:ring-2 focus-within:ring-primary/25",
            urlMissing
              ? "border-danger-fg ring-2 ring-danger-fg/20"
              : "border-input hover:border-border-hover",
            !canWrite && "opacity-75",
          )}
        >
          <div className="flex h-full w-24 shrink-0 items-center border-r border-border-subtle pl-2.5 pr-1">
            {isCustomMethod && (
              <input
                ref={customInputRef}
                type="text"
                value={customDraft}
                maxLength={16}
                disabled={!canWrite}
                spellCheck={false}
                autoComplete="off"
                aria-label={t("supplier.config.method")}
                style={{ color: methodColorVar(customDraft || "CUSTOM") }}
                onChange={handleCustomTyped}
                className="h-full min-w-0 flex-1 bg-transparent pr-1 text-xs font-semibold outline-none"
              />
            )}
            <Popover
              open={methodOpen}
              onOpenChange={(next) => {
                setMethodOpen(next);
                if (next) urlInputRef?.current?.blur();
              }}
              align="left"
              className={cn(isCustomMethod ? "shrink-0" : "w-full")}
              contentClassName="w-32 p-1"
              trigger={
                <button
                  type="button"
                  disabled={!canWrite}
                  aria-expanded={methodOpen}
                  aria-haspopup="listbox"
                  aria-label={t("supplier.config.method")}
                  onClick={() => setMethodOpen((v) => !v)}
                  className="inline-flex h-7 w-full items-center justify-between gap-1 rounded-md px-1 text-xs font-semibold outline-none hover:bg-hover-bg disabled:pointer-events-none cursor-pointer"
                >
                  {!isCustomMethod && (
                    <span
                      className="truncate"
                      style={{ color: methodColorVar(method) }}
                    >
                      {shownMethod}
                    </span>
                  )}
                  <ChevronDown className="h-3 w-3 shrink-0 text-muted-foreground" />
                </button>
              }
            >
              <div className="flex flex-col gap-0.5" role="listbox">
                {HTTP_METHODS.map((m) => {
                  const selected = m === shownMethod;
                  return (
                    <button
                      key={m}
                      type="button"
                      role="option"
                      aria-selected={selected}
                      style={{ color: methodColorVar(m) }}
                      onClick={() => handlePickMethod(m)}
                      className={cn(
                        "flex w-full items-center rounded-md px-2 py-1.5 text-left text-xs font-semibold transition-colors cursor-pointer",
                        selected ? "bg-primary-soft" : "hover:bg-hover-bg",
                      )}
                    >
                      {m}
                    </button>
                  );
                })}
              </div>
            </Popover>
          </div>

          <div className="min-w-0 flex-1 h-full">
            <WorkbenchEnvText
              ref={urlInputRef}
              compact
              value={urlDraft}
              env={env}
              maskPreview={!canWrite}
              disabled={!canWrite}
              placeholder={t("upstreamConnector.fields.upstreamUrl")}
              ariaLabel={t("upstreamConnector.fields.upstreamUrl")}
              onChange={onUrlInput}
              onFocus={onUrlFocus}
              onBlur={onUrlBlur}
              onPaste={handlePaste}
              onKeyDown={(ev) => {
                if (ev.key === "Enter") {
                  ev.preventDefault();
                  if (running) onCancel();
                  else onSend();
                }
              }}
            />
          </div>
        </div>

        {canWrite && (
          <div className="inline-flex shrink-0 items-center">
            <Tooltip
              content={t("upstreamConnector.sendProbeTooltip")}
              side="bottom"
            >
              <Button
                type="button"
                variant="primary"
                size="md"
                aria-label={
                  running
                    ? t("upstreamConnector.sendCancel")
                    : t("upstreamConnector.runBusiness")
                }
                onClick={running ? onCancel : onSend}
                className="h-8 rounded-r-none border-r border-primary-foreground/20 px-3"
              >
                {running ? (
                  <X className="h-3.5 w-3.5 shrink-0" />
                ) : (
                  <Send className="h-3.5 w-3.5 shrink-0" />
                )}
                <span>
                  {running
                    ? t("upstreamConnector.sendCancel")
                    : t("upstreamConnector.send")}
                </span>
              </Button>
            </Tooltip>

            <Popover
              open={sendMenuOpen}
              onOpenChange={setSendMenuOpen}
              align="right"
              contentClassName="min-w-[180px] p-1"
              trigger={
                <Button
                  type="button"
                  variant="primary"
                  size="md"
                  aria-label={t("upstreamConnector.sessionActions")}
                  aria-expanded={sendMenuOpen}
                  onClick={() => setSendMenuOpen((v) => !v)}
                  className="h-8 rounded-l-none px-2"
                >
                  <ChevronDown className="h-3.5 w-3.5" />
                </Button>
              }
            >
              <div className="flex flex-col gap-0.5">
                {hasLinkedSupplier && (
                  <button
                    type="button"
                    onClick={() => {
                      setSendMenuOpen(false);
                      onLiveRun();
                    }}
                    className="flex w-full items-center rounded-md px-2.5 py-1.5 text-left text-xs text-foreground hover:bg-hover-bg cursor-pointer"
                  >
                    {t("upstreamConnector.liveRun")}
                  </button>
                )}
                <button
                  type="button"
                  onClick={() => {
                    setSendMenuOpen(false);
                    onRunAuth();
                  }}
                  className="flex w-full items-center rounded-md px-2.5 py-1.5 text-left text-xs text-foreground hover:bg-hover-bg cursor-pointer"
                >
                  {t("upstreamConnector.runAuth")}
                </button>
                <button
                  type="button"
                  onClick={() => {
                    setSendMenuOpen(false);
                    onTransformPreview();
                  }}
                  className="flex w-full items-center rounded-md px-2.5 py-1.5 text-left text-xs text-foreground hover:bg-hover-bg cursor-pointer"
                >
                  {t("upstreamConnector.transformPreview")}
                </button>
                <div className="my-1 h-px bg-border-subtle" />
                <button
                  type="button"
                  onClick={() => {
                    setSendMenuOpen(false);
                    onCopyCurl();
                  }}
                  className="flex w-full items-center rounded-md px-2.5 py-1.5 text-left text-xs text-foreground hover:bg-hover-bg cursor-pointer"
                >
                  {t("upstreamConnector.copyCurl")}
                </button>
              </div>
            </Popover>
          </div>
        )}
      </div>

      {unresolved.length > 0 && (
        <div className="rounded-md border border-warning-border bg-warning-bg px-2.5 py-1 text-xs text-warning-fg">
          {t("upstreamConnector.unresolvedVars", {
            vars: unresolved.join(", "),
          })}
        </div>
      )}

      {staticCaptureWarnings.length > 0 && (
        <div className="rounded-md border border-warning-border bg-warning-bg px-2.5 py-1 text-xs text-warning-fg">
          {t("upstreamConnector.staticCaptureWarn", {
            vars: staticCaptureWarnings.join(", "),
          })}
        </div>
      )}
    </div>
  );
}
