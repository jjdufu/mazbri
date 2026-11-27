import React, { useEffect, useMemo, useRef, useState } from "react";
import { Search } from "lucide-react";
import { useT } from "@/locales";
import { Modal } from "@/components/ui";
import { useWorkbenchStore } from "../store";

export interface ShortcutRow {
  id: string;
  label: string;
  keys: string;
}

function WorkbenchShortcutsModalBody() {
  const { t } = useT();
  const setShowShortcuts = useWorkbenchStore((s) => s.setShowShortcuts);
  const [q, setQ] = useState("");
  const inputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    const id = requestAnimationFrame(() => inputRef.current?.focus());
    return () => cancelAnimationFrame(id);
  }, []);

  const rows = useMemo<ShortcutRow[]>(
    () => [
      {
        id: "send",
        label: t("upstreamConnector.send"),
        keys: "Ctrl/⌘+Enter",
      },
      {
        id: "save",
        label: t("upstreamConnector.syncDraftNow"),
        keys: "Ctrl/⌘+S",
      },
      {
        id: "sidebar",
        label: t("upstreamConnector.collapseSidebar"),
        keys: "Ctrl/⌘+B",
      },
      {
        id: "commands",
        label: t("upstreamConnector.commandPalette"),
        keys: "Ctrl/⌘+K",
      },
      {
        id: "shortcuts",
        label: t("upstreamConnector.shortcutsTitle"),
        keys: "Ctrl/⌘+/",
      },
      {
        id: "close",
        label: t("upstreamConnector.shortcutsClose"),
        keys: "Esc",
      },
    ],
    [t],
  );

  const filtered = useMemo(() => {
    const needle = q.trim().toLowerCase();
    if (!needle) return rows;
    return rows.filter((r) =>
      `${r.label} ${r.keys}`.toLowerCase().includes(needle),
    );
  }, [q, rows]);

  return (
    <Modal
      open
      onClose={() => setShowShortcuts(false)}
      title={t("upstreamConnector.shortcutsTitle")}
      size="sm"
    >
      <div className="space-y-3">
        <div className="flex h-8 items-center gap-2 rounded-md border border-input bg-card px-2.5 focus-within:border-primary focus-within:ring-2 focus-within:ring-primary/30">
          <Search className="h-3.5 w-3.5 text-muted-foreground shrink-0" />
          <input
            ref={inputRef}
            type="text"
            value={q}
            onChange={(e) => setQ(e.target.value)}
            placeholder={t("upstreamConnector.shortcutsSearch")}
            aria-label={t("upstreamConnector.shortcutsSearch")}
            spellCheck={false}
            className="flex-1 min-w-0 bg-transparent text-xs text-foreground placeholder:text-muted-foreground focus:outline-none"
          />
        </div>

        <div className="max-h-72 overflow-y-auto divide-y divide-border-subtle">
          {filtered.length === 0 ? (
            <div className="py-6 text-center text-xs text-muted-foreground">
              {t("upstreamConnector.commandPaletteEmpty")}
            </div>
          ) : (
            filtered.map((row) => (
              <div
                key={row.id}
                className="flex items-center justify-between gap-3 py-2 text-xs text-foreground"
              >
                <span>{row.label}</span>
                <kbd className="rounded-md border border-border bg-muted px-2 py-0.5 font-mono-tech text-[11px] text-muted-foreground shrink-0">
                  {row.keys}
                </kbd>
              </div>
            ))
          )}
        </div>
      </div>
    </Modal>
  );
}

export function WorkbenchShortcutsModal() {
  const open = useWorkbenchStore((s) => s.showShortcuts);
  if (!open) return null;
  return <WorkbenchShortcutsModalBody />;
}
