import React, { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router";
import { Search } from "lucide-react";
import { useT } from "@/locales";
import { cn, Modal } from "@/components/ui";
import {
  displayMethodBadge,
  methodTone,
} from "../engine/workbenchHelpers";
import {
  selectActiveEp,
  selectActiveRuntime,
  selectCanWriteWorkbench,
  useWorkbenchStore,
  type RequestSubTabId,
} from "../store";

export interface WorkbenchCommandItem {
  id: string;
  label: string;
  hint?: string;
  keywords?: string;
  method?: string;
}

function WorkbenchCommandPaletteBody() {
  const { t } = useT();
  const navigate = useNavigate();
  const state = useWorkbenchStore();
  const activeEp = useMemo(() => selectActiveEp(state), [state]);
  const runtime = useMemo(() => selectActiveRuntime(state), [state]);
  const canWrite = useMemo(() => selectCanWriteWorkbench(state), [state]);

  const [q, setQ] = useState("");
  const [activeIdx, setActiveIdx] = useState(0);
  const inputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    const id = requestAnimationFrame(() => inputRef.current?.focus());
    return () => cancelAnimationFrame(id);
  }, []);

  const commands = useMemo<WorkbenchCommandItem[]>(() => {
    const items: WorkbenchCommandItem[] = [
      ...(canWrite
        ? [
            {
              id: "send",
              label: t("upstreamConnector.send"),
              hint: "Ctrl/⌘+Enter",
              keywords: "send run execute",
            },
            {
              id: "auth",
              label: t("upstreamConnector.runAuth"),
              keywords: "auth token ticket",
            },
            {
              id: "transform",
              label: t("upstreamConnector.transformPreview"),
              keywords: "transform preview payload",
            },
            {
              id: "save",
              label: t("upstreamConnector.syncDraftNow"),
              hint: "Ctrl/⌘+S",
              keywords: "save draft sync",
            },
            {
              id: "publish",
              label: t("upstreamConnector.publish"),
              keywords: "publish deploy",
            },
          ]
        : []),
      {
        id: "curl",
        label: t("upstreamConnector.copyCurl"),
        keywords: "copy curl command",
      },
      {
        id: "tab:params",
        label: t("upstreamConnector.paramsTab"),
        keywords: "params query",
      },
      {
        id: "tab:body",
        label: t("upstreamConnector.bodyTab"),
        keywords: "body json",
      },
      {
        id: "tab:headers",
        label: t("upstreamConnector.headersTab"),
        keywords: "headers",
      },
      {
        id: "tab:auth",
        label: t("upstreamConnector.authTab"),
        keywords: "auth",
      },
      {
        id: "tab:transform",
        label: t("upstreamConnector.transformTab"),
        keywords: "egress transform adapter",
      },
      {
        id: "tab:assert",
        label: t("upstreamConnector.assertTab"),
        keywords: "assert assertions",
      },
      {
        id: "tab:captures",
        label: t("upstreamConnector.capturesTab"),
        keywords: "captures extract",
      },
      {
        id: "tab:tls",
        label: t("upstreamConnector.tlsTab"),
        keywords: "tls mtls timeout",
      },
      {
        id: "drawer:env",
        label: t("upstreamConnector.envVars"),
        keywords: "env variables global",
      },
      ...(canWrite && activeEp
        ? (["GET", "POST", "PUT", "PATCH", "DELETE"] as const).map((m) => ({
            id: `method:${m}`,
            label: m,
            method: m,
            keywords: `method http ${m.toLowerCase()}`,
          }))
        : []),
    ];

    state.endpoints.forEach((ep, i) => {
      items.push({
        id: `ep:${i}`,
        label: ep.name || ep.path || `ep-${i + 1}`,
        hint: ep.path || "/",
        method: ep.method || "GET",
        keywords: `${ep.method || ""} ${ep.path || ""} ${ep.tag || ""}`,
      });
    });

    return items;
  }, [activeEp, canWrite, state.endpoints, t]);

  const filtered = useMemo(() => {
    const needle = q.trim().toLowerCase();
    if (!needle) return commands;
    return commands.filter((c) =>
      `${c.label} ${c.keywords || ""} ${c.id}`.toLowerCase().includes(needle),
    );
  }, [commands, q]);

  const safeActiveIdx =
    filtered.length === 0 ? 0 : Math.min(activeIdx, filtered.length - 1);

  const runCommand = (cmd: WorkbenchCommandItem) => {
    const id = cmd.id;
    state.setShowCommands(false);

    if (id === "send") {
      if (runtime.running) {
        state.abortActiveRun();
      } else {
        void state.doRun("business");
      }
      return;
    }
    if (id === "auth") {
      void state.doRun("auth_only");
      return;
    }
    if (id === "transform") {
      void state.doTransformPreview();
      return;
    }
    if (id === "curl") {
      void state.copyCurl();
      return;
    }
    if (id === "save") {
      void state.saveDraftExplicit(navigate);
      return;
    }
    if (id === "publish") {
      void state.openPublish(navigate);
      return;
    }
    if (id === "drawer:env") {
      state.setEnvDrawerOpen(true);
      return;
    }
    if (id.startsWith("tab:")) {
      state.setReqTab(id.slice(4) as RequestSubTabId);
      return;
    }
    if (id.startsWith("method:")) {
      const nextMethod = id.slice(7);
      state.updateActiveEndpoint((ep) => ({ ...ep, method: nextMethod }));
      return;
    }
    if (id.startsWith("ep:")) {
      const idx = Number(id.slice(3));
      if (Number.isFinite(idx)) {
        state.switchEndpointByIndex(idx);
      }
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      if (!filtered.length) return;
      setActiveIdx((prev) => (prev + 1) % filtered.length);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      if (!filtered.length) return;
      setActiveIdx((prev) => (prev - 1 + filtered.length) % filtered.length);
    } else if (e.key === "Enter") {
      e.preventDefault();
      const target = filtered[safeActiveIdx];
      if (target) runCommand(target);
    } else if (e.key === "Tab") {
      e.preventDefault();
      inputRef.current?.focus();
    }
  };

  return (
    <Modal
      open
      onClose={() => state.setShowCommands(false)}
      title={t("upstreamConnector.commandPalette")}
      size="sm"
    >
      <div className="space-y-2.5" onKeyDown={handleKeyDown}>
        <div className="flex h-8 items-center gap-2 rounded-md border border-input bg-card px-2.5 focus-within:border-primary focus-within:ring-2 focus-within:ring-primary/30">
          <Search className="h-3.5 w-3.5 text-muted-foreground shrink-0" />
          <input
            ref={inputRef}
            type="text"
            value={q}
            onChange={(e) => {
              setQ(e.target.value);
              setActiveIdx(0);
            }}
            placeholder={t("upstreamConnector.commandPalettePlaceholder")}
            aria-label={t("upstreamConnector.commandPalettePlaceholder")}
            spellCheck={false}
            className="flex-1 min-w-0 bg-transparent text-xs text-foreground placeholder:text-muted-foreground focus:outline-none"
          />
        </div>

        <div
          role="listbox"
          className="max-h-80 overflow-y-auto space-y-0.5"
        >
          {filtered.length === 0 ? (
            <div className="py-6 text-center text-xs text-muted-foreground">
              {t("upstreamConnector.commandPaletteEmpty")}
            </div>
          ) : (
            filtered.map((cmd, idx) => {
              const selected = idx === safeActiveIdx;
              return (
                <button
                  key={cmd.id}
                  type="button"
                  role="option"
                  aria-selected={selected}
                  onMouseEnter={() => setActiveIdx(idx)}
                  onClick={() => runCommand(cmd)}
                  className={cn(
                    "w-full flex items-center justify-between gap-2 rounded-md px-2.5 py-2 text-left text-xs transition-colors cursor-pointer",
                    selected
                      ? "bg-primary-soft text-primary font-medium"
                      : "text-foreground hover:bg-hover-bg",
                  )}
                >
                  <span className="inline-flex items-center gap-2 min-w-0 truncate">
                    {cmd.method && (
                      <span
                        className="w-11 shrink-0 font-mono-tech text-[10px] font-bold uppercase"
                        style={{
                          color: `var(--wb-method-${methodTone(cmd.method)}, var(--foreground))`,
                        }}
                      >
                        {displayMethodBadge(cmd.method)}
                      </span>
                    )}
                    <span className="truncate">{cmd.label}</span>
                  </span>
                  {cmd.hint && (
                    <kbd className="max-w-[160px] truncate rounded-md border border-border bg-muted px-1.5 py-0.5 font-mono-tech text-[10px] text-muted-foreground shrink-0">
                      {cmd.hint}
                    </kbd>
                  )}
                </button>
              );
            })
          )}
        </div>
      </div>
    </Modal>
  );
}

export function WorkbenchCommandPalette() {
  const open = useWorkbenchStore((s) => s.showCommands);
  if (!open) return null;
  return <WorkbenchCommandPaletteBody />;
}
