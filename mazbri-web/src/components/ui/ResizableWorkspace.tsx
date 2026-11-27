import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Group,
  Panel,
  Separator,
  type GroupImperativeHandle,
  type Layout,
  type PanelImperativeHandle,
} from "react-resizable-panels";
import { cn } from "./cn";
import { useUiTx } from "./tx";

export interface WorkspacePaneSpec {
  readonly id: string;
  readonly defaultSize: number;
  readonly defaultPx?: number;
  readonly minSize?: number;
  readonly minPx?: number;
  readonly maxSize?: number;
  readonly collapsible?: boolean;
  readonly className?: string;
  readonly handleTestId?: string;
  readonly children: React.ReactNode;
}

export interface ResizableWorkspaceProps {
  readonly storageKey: string;
  readonly direction?: "horizontal" | "vertical";
  readonly panes: readonly WorkspacePaneSpec[];
  readonly className?: string;
  readonly testId?: string;
}

export function normalizeLayoutStorageKey(rawKey: string): string {
  const trimmed = rawKey.trim();
  return trimmed.startsWith("mazbri.layout.")
    ? trimmed
    : `mazbri.layout.${trimmed}`;
}

const LAYOUT_STORAGE_VERSION = 2;

function readPersistedLayout(
  storageKey: string,
  panes: readonly WorkspacePaneSpec[],
): Layout | undefined {
  try {
    if (typeof localStorage === "undefined") return undefined;
    const raw = localStorage.getItem(storageKey);
    if (!raw) return undefined;
    const parsed: unknown = JSON.parse(raw);
    if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
      return undefined;
    }
    const rec = parsed as Record<string, unknown>;
    if (rec.__v !== LAYOUT_STORAGE_VERSION) {
      return undefined;
    }
    const out: Layout = {};
    let total = 0;
    let paneKeyCount = 0;
    for (const k of Object.keys(rec)) {
      if (k !== "__v") paneKeyCount += 1;
    }
    for (const pane of panes) {
      const val = rec[pane.id];
      if (
        typeof val !== "number" ||
        !Number.isFinite(val) ||
        val < 0 ||
        val > 100 ||
        (!pane.collapsible && val < 2)
      ) {
        return undefined;
      }
      out[pane.id] = val;
      total += val;
    }
    if (paneKeyCount !== panes.length || total < 95 || total > 105) {
      return undefined;
    }
    return out;
  } catch {
    return undefined;
  }
}

function writePersistedLayout(storageKey: string, layout: Layout): void {
  try {
    if (typeof localStorage !== "undefined") {
      localStorage.setItem(
        storageKey,
        JSON.stringify({ __v: LAYOUT_STORAGE_VERSION, ...layout }),
      );
    }
  } catch {
    // Ignore storage quota or privacy-mode failures
  }
}

function resolveDefaultHandleTestId(
  direction: "horizontal" | "vertical",
  panes: readonly WorkspacePaneSpec[],
  index: number,
): string {
  const explicit = panes[index]?.handleTestId;
  if (explicit) return explicit;
  if (direction === "vertical") {
    return index === 0
      ? "resizable-handle-vertical"
      : `resizable-handle-vertical-${index}`;
  }
  if (panes.length >= 3) {
    if (index === 0) return "resizable-handle-left";
    if (index === panes.length - 2) return "resizable-handle-right";
    return `resizable-handle-${index}`;
  }
  const left = panes[index];
  const right = panes[index + 1];
  if (left && !left.collapsible && right?.collapsible) {
    return "resizable-handle-right";
  }
  return "resizable-handle-left";
}

export function ResizableWorkspace({
  storageKey,
  direction = "horizontal",
  panes,
  className,
  testId,
}: ResizableWorkspaceProps) {
  const { tx } = useUiTx();
  const effectiveStorageKey = normalizeLayoutStorageKey(storageKey);
  const shortId = effectiveStorageKey.replace(/^mazbri\.layout\./, "");

  const groupRef = useRef<GroupImperativeHandle | null>(null);
  const panelRefs = useRef<Record<string, PanelImperativeHandle | null>>({});
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [containerExtent, setContainerExtent] = useState(0);

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const measure = () => {
      const next =
        direction === "horizontal" ? el.clientWidth : el.clientHeight;
      if (next > 0) setContainerExtent(next);
    };
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, [direction]);

  const initialLayout = useMemo(
    () => readPersistedLayout(effectiveStorageKey, panes),
    [effectiveStorageKey, panes],
  );

  const defaultPercentLayout = useMemo<Layout>(() => {
    const hasDefaultPx =
      containerExtent > 0 &&
      panes.some((p) => typeof p.defaultPx === "number" && p.defaultPx > 0);
    if (!hasDefaultPx) {
      const map: Layout = {};
      for (const p of panes) {
        map[p.id] = p.defaultSize;
      }
      return map;
    }
    const map: Layout = {};
    let fixedPctSum = 0;
    const flexPanes: WorkspacePaneSpec[] = [];
    for (const p of panes) {
      if (typeof p.defaultPx === "number" && p.defaultPx > 0) {
        const minPct = p.minSize ?? 12;
        const maxPct = p.maxSize ?? 60;
        const pct = Math.min(
          maxPct,
          Math.max(
            minPct,
            Math.round((p.defaultPx / containerExtent) * 1000) / 10,
          ),
        );
        map[p.id] = pct;
        fixedPctSum += pct;
      } else {
        flexPanes.push(p);
      }
    }
    const remaining = Math.max(10, 100 - fixedPctSum);
    const flexWeightSum = flexPanes.reduce(
      (s, p) => s + Math.max(1, p.defaultSize),
      0,
    );
    for (const p of flexPanes) {
      map[p.id] =
        flexWeightSum > 0
          ? Math.round((Math.max(1, p.defaultSize) / flexWeightSum) * remaining * 10) /
            10
          : remaining / Math.max(1, flexPanes.length);
    }
    return map;
  }, [containerExtent, panes]);

  const appliedDefaultPxKeyRef = useRef<string>("");
  const paneShapeKey = panes
    .map((p) => `${p.id}:${p.defaultPx ?? ""}`)
    .join("|");
  useEffect(() => {
    if (containerExtent <= 0 || !groupRef.current) return;
    if (initialLayout) return;
    if (
      !panes.some((p) => typeof p.defaultPx === "number" && p.defaultPx > 0)
    ) {
      return;
    }
    const sig = `${effectiveStorageKey}|${paneShapeKey}`;
    if (appliedDefaultPxKeyRef.current === sig) return;
    appliedDefaultPxKeyRef.current = sig;
    groupRef.current.setLayout(defaultPercentLayout);
  }, [
    containerExtent,
    defaultPercentLayout,
    effectiveStorageKey,
    initialLayout,
    paneShapeKey,
    panes,
  ]);

  const handlePersist = useCallback(
    (layout: Layout) => {
      writePersistedLayout(effectiveStorageKey, layout);
    },
    [effectiveStorageKey],
  );

  const handleSeparatorDoubleClick = useCallback(
    (leftIndex: number) => {
      const leftPane = panes[leftIndex];
      const rightPane = panes[leftIndex + 1];
      if (!leftPane || !rightPane) return;

      const targetPane = leftPane.collapsible
        ? leftPane
        : rightPane.collapsible
          ? rightPane
          : leftPane.defaultSize <= rightPane.defaultSize
            ? leftPane
            : rightPane;

      const handle = panelRefs.current[targetPane.id];
      if (targetPane.collapsible && handle) {
        const sizeInfo = handle.getSize();
        if (handle.isCollapsed() || sizeInfo.asPercentage < 2) {
          handle.expand();
          const afterExpand = handle.getSize();
          if (afterExpand.asPercentage < 2) {
            handle.resize(`${targetPane.defaultSize}%`);
          }
        } else {
          handle.collapse();
        }
        requestAnimationFrame(() => {
          const current = groupRef.current?.getLayout();
          if (current) {
            writePersistedLayout(effectiveStorageKey, current);
          }
        });
        return;
      }

      if (groupRef.current) {
        const next = groupRef.current.setLayout(defaultPercentLayout);
        writePersistedLayout(effectiveStorageKey, next);
      }
    },
    [defaultPercentLayout, effectiveStorageKey, panes],
  );

  return (
    <div
      ref={containerRef}
      className={cn("h-full w-full min-h-0 min-w-0", className)}
    >
      <Group
        id={`workspace-group-${shortId}`}
        data-testid={testId ?? `resizable-workspace-${shortId}`}
        orientation={direction}
        groupRef={groupRef}
        defaultLayout={initialLayout}
        onLayoutChange={handlePersist}
        onLayoutChanged={(layout, meta) => {
          handlePersist(meta.requestedLayout ?? layout);
        }}
        className="h-full w-full min-h-0 min-w-0"
      >
        {panes.map((pane, idx) => {
          const isLast = idx === panes.length - 1;
          const handleId = resolveDefaultHandleTestId(direction, panes, idx);
          const inferredMinPx =
            pane.minPx ??
            (direction === "horizontal" &&
            !pane.collapsible &&
            pane.defaultSize <= 38
              ? 280
              : direction === "horizontal" &&
                  pane.collapsible &&
                  pane.defaultSize <= 25
                ? 210
                : 0);
          const pxMinPct =
            inferredMinPx > 0 && containerExtent > 0
              ? Math.ceil((inferredMinPx / containerExtent) * 100)
              : 0;
          const baseMinPct = pane.minSize ?? 12;
          const cappedMax = pane.maxSize ?? 90;
          const effectiveMinPct = Math.min(
            cappedMax - 5,
            Math.max(baseMinPct, pxMinPct),
          );
          return (
            <React.Fragment key={pane.id}>
              <Panel
                id={pane.id}
                data-testid={`resizable-pane-${shortId}-${pane.id}`}
                defaultSize={`${pane.defaultSize}%`}
                minSize={`${effectiveMinPct}%`}
                maxSize={
                  pane.maxSize !== undefined ? `${pane.maxSize}%` : undefined
                }
                collapsible={Boolean(pane.collapsible)}
                collapsedSize="0%"
                panelRef={(inst) => {
                  panelRefs.current[pane.id] = inst;
                }}
                className={cn(
                  "h-full min-h-0 min-w-0 flex flex-col overflow-hidden @container",
                  pane.className,
                )}
              >
                {pane.children}
              </Panel>

              {!isLast && (
                <Separator
                  id={`${shortId}-sep-${idx}`}
                  data-testid={handleId}
                  data-handle-id={`${shortId}-${idx}`}
                  disableDoubleClick
                  aria-label={tx("调整分栏尺寸", "Resize pane")}
                  title={tx("调整分栏", "Resize")}
                  onDoubleClick={() => handleSeparatorDoubleClick(idx)}
                  className={cn(
                    "group relative z-10 flex shrink-0 items-center justify-center outline-none select-none",
                    direction === "horizontal"
                      ? "w-2.5 h-full cursor-col-resize"
                      : "h-2.5 w-full cursor-row-resize",
                  )}
                >
                  <span
                    aria-hidden="true"
                    className={cn(
                      "relative flex items-center justify-center rounded-full bg-[var(--border-subtle,var(--border))] transition-all duration-150",
                      direction === "horizontal"
                        ? "h-full w-px group-hover:w-[3px] group-hover:bg-[var(--primary)] group-focus-visible:w-[3px] group-focus-visible:bg-[var(--primary)] group-data-[separator=hover]:w-[3px] group-data-[separator=hover]:bg-[var(--primary)] group-data-[separator=active]:w-[3px] group-data-[separator=active]:bg-[var(--primary)]"
                        : "w-full h-px group-hover:h-[3px] group-hover:bg-[var(--primary)] group-focus-visible:h-[3px] group-focus-visible:bg-[var(--primary)] group-data-[separator=hover]:h-[3px] group-data-[separator=hover]:bg-[var(--primary)] group-data-[separator=active]:h-[3px] group-data-[separator=active]:bg-[var(--primary)]",
                    )}
                  />
                </Separator>
              )}
            </React.Fragment>
          );
        })}
      </Group>
    </div>
  );
}
