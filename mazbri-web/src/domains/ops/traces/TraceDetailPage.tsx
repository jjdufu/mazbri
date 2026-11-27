import React, { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate, useParams, useSearchParams } from "react-router";
import { AlertTriangle, Loader2 } from "lucide-react";
import {
  Badge,
  Button,
  Card,
  CopyableText,
  DataTablePage,
  EmptyState,
  EntityDetailLayout,
  SegmentedControl,
  Tabs,
  cn,
  type DataTableColumn,
  type EntityDetailMetaItem,
} from "@/components/ui";
import { fetchCallStages } from "@/api/call-monitor";
import { fetchTrace, fetchTraceBilling } from "@/api/traces";
import { useT } from "@/locales";
import type { CallStageAccessRow } from "@/types/api/call-monitor";
import type {
  LinkedSpan,
  TraceBillingRow,
  TraceDetail,
  TraceSpan,
  TraceSpanEvent,
  TraceSpanLink,
} from "@/types/api/traces";
import {
  formatCodeFilePath,
  formatDateTime,
  formatMoney,
} from "@/utils/format";
import { HttpError } from "@/utils/http/error";
import {
  buildCallsRoute,
  buildTraceRoute,
  buildTraceSearchRoute,
} from "@/utils/ops/opsRoutes";
import { showErrorToast } from "@/utils/ui/message-guard";
import { toLocationHref } from "../shared/opsHelpers";
import {
  computeSelfSegments,
  computeTreeConnectors,
  createWaterfallProjector,
  findFoldableUpstream,
  formatTraceMs,
  resolveWaterfallDurPlacement,
  type WaterfallScaleMode,
} from "./waterfall-layout";

const SELF_BADGE_THRESHOLD_NS = 50_000;

const PALETTE = [
  "#5d87ff",
  "#059669",
  "#d97706",
  "#7c3aed",
  "#0d9488",
  "#ea580c",
  "#0284c7",
  "#dc2626",
  "#64748b",
  "#14b8a6",
  "#9333ea",
];

function KeyValueList({ value }: { value: Record<string, unknown> }) {
  const entries = Object.entries(value ?? {}).sort(([a], [b]) =>
    a.localeCompare(b),
  );
  if (!entries.length) {
    return <div className="text-xs text-muted-foreground">—</div>;
  }
  return (
    <dl className="grid grid-cols-[max-content_minmax(0,1fr)] gap-x-3 gap-y-1 text-xs">
      {entries.map(([k, v]) => {
        const raw = typeof v === "string" ? v : JSON.stringify(v);
        const display =
          k === "code.file.path" && typeof v === "string"
            ? formatCodeFilePath(v)
            : raw;
        return (
          <React.Fragment key={k}>
            <dt className="text-muted-foreground font-mono-tech">{k}</dt>
            <dd
              title={display !== raw ? raw : undefined}
              className="m-0 break-all font-mono-tech text-foreground"
            >
              {display}
            </dd>
          </React.Fragment>
        );
      })}
    </dl>
  );
}

function isErrorEvent(ev: TraceSpanEvent): boolean {
  const lower = (ev.name ?? "").toLowerCase();
  return lower.includes("exception") || lower.includes("error");
}

function defaultSelection(d: TraceDetail, wantedSpan: string): string {
  if (wantedSpan && d.spans.some((s) => s.span_id === wantedSpan)) {
    return wantedSpan;
  }
  const firstError = d.spans.find((s) => s.error);
  if (firstError) return firstError.span_id;
  const longest = (list: TraceSpan[]) =>
    list.reduce<TraceSpan | null>(
      (best, s) => (!best || s.duration_ns > best.duration_ns ? s : best),
      null,
    );
  const critical = d.spans.filter((s) => s.critical && s.depth > 0);
  const client = longest(
    critical.filter((s) => s.span_kind === "SPAN_KIND_CLIENT"),
  );
  return (
    client?.span_id ?? longest(critical)?.span_id ?? d.spans[0]?.span_id ?? ""
  );
}

export function TraceDetailPage() {
  const { t } = useT();
  const navigate = useNavigate();
  const params = useParams<{ traceId: string }>();
  const [searchParams, setSearchParams] = useSearchParams();

  const traceId = (params.traceId ?? "").toLowerCase();
  const [loading, setLoading] = useState(false);
  const [missing, setMissing] = useState(false);
  const [detail, setDetail] = useState<TraceDetail | null>(null);
  const [selectedId, setSelectedId] = useState("");
  const [collapsed, setCollapsed] = useState<Set<string>>(() => new Set());
  const [scaleMode, setScaleMode] = useState<WaterfallScaleMode>("linear");
  const [panelTab, setPanelTab] = useState<"attributes" | "events" | "links">(
    "attributes",
  );
  const [bottomTab, setBottomTab] = useState<"logs" | "billing">("logs");

  const [callsLoading, setCallsLoading] = useState(false);
  const [calls, setCalls] = useState<CallStageAccessRow[]>([]);
  const [billingLoading, setBillingLoading] = useState(false);
  const [billing, setBilling] = useState<TraceBillingRow[]>([]);
  const [billingError, setBillingError] = useState("");

  const seqRef = useRef(0);

  const startHint = useMemo<{ start?: number; end?: number }>(() => {
    const rawStart = Number(searchParams.get("start"));
    const rawEnd = Number(searchParams.get("end"));
    if (!Number.isFinite(rawStart) || rawStart <= 0) return {};
    const start = Math.max(1, Math.floor(rawStart) - 900);
    const end =
      Number.isFinite(rawEnd) && rawEnd > start
        ? Math.floor(rawEnd)
        : Math.floor(rawStart) + 3 * 3600;
    return { start, end };
  }, [searchParams]);

  useEffect(() => {
    if (!/^[0-9a-f]{32}$/.test(traceId)) {
      showErrorToast(t("ops.traces.invalidId"));
      return;
    }

    const mine = ++seqRef.current;
    const ctrl = new AbortController();
    setMissing(false);
    setDetail(null);
    setCollapsed(new Set());
    setScaleMode("linear");
    setLoading(true);
    setCallsLoading(true);
    setBilling([]);
    setBillingError("");

    const wantedSpan = searchParams.get("span") ?? "";
    const fallbackReq = searchParams.get("request_id") ?? "";

    fetchTrace(traceId, startHint, ctrl.signal)
      .then((res) => {
        if (mine !== seqRef.current || ctrl.signal.aborted) return;
        setDetail(res);
        setSelectedId(defaultSelection(res, wantedSpan));
      })
      .catch((error: unknown) => {
        if (mine !== seqRef.current || ctrl.signal.aborted) return;
        if (error instanceof HttpError && error.code === 404) {
          setMissing(true);
        } else {
          showErrorToast(t("ops.traces.loadFailed"));
        }
      })
      .finally(() => {
        if (mine === seqRef.current) setLoading(false);
      });

    (async () => {
      try {
        let res = await fetchCallStages({ trace_id: traceId }, ctrl.signal);
        if (!res.calls.length && fallbackReq) {
          res = await fetchCallStages({ request_id: fallbackReq }, ctrl.signal);
        }
        if (mine !== seqRef.current || ctrl.signal.aborted) return;
        const rows = res.calls ?? [];
        setCalls(rows);
        const ids = [...new Set(rows.map((c) => c.request_id).filter(Boolean))];
        if (ids.length > 0) {
          setBillingLoading(true);
          try {
            const billRows = await fetchTraceBilling(
              ids.slice(0, 50),
              ctrl.signal,
            );
            if (mine === seqRef.current && !ctrl.signal.aborted) {
              setBilling(billRows ?? []);
            }
          } catch (err: unknown) {
            if (mine === seqRef.current && !ctrl.signal.aborted) {
              setBillingError(
                err instanceof HttpError && err.code === 403
                  ? t("ops.traces.billing.forbidden")
                  : t("ops.traces.billing.loadFailed"),
              );
            }
          } finally {
            if (mine === seqRef.current) setBillingLoading(false);
          }
        }
      } catch {
        if (mine === seqRef.current) setCalls([]);
      } finally {
        if (mine === seqRef.current) setCallsLoading(false);
      }
    })();

    return () => ctrl.abort();
  }, [traceId]);

  const serviceIndex = useMemo(() => {
    const map = new Map<string, number>();
    (detail?.services ?? []).forEach((s, i) => map.set(s, i));
    return map;
  }, [detail?.services]);

  const serviceColor = (service: string): string => {
    const idx = serviceIndex.get(service);
    if (idx !== undefined) return PALETTE[idx % PALETTE.length]!;
    let hash = 0;
    for (const ch of service) hash = (hash * 31 + ch.charCodeAt(0)) >>> 0;
    return PALETTE[hash % PALETTE.length]!;
  };

  const errorCount = useMemo(
    () => detail?.spans.filter((s) => s.error).length ?? 0,
    [detail?.spans],
  );

  const rootOperationName = useMemo(() => {
    const spans = detail?.spans ?? [];
    const root = spans.find((s) => !s.parent_span_id) ?? spans[0];
    return root ? `${root.service_name} · ${root.span_name}` : "";
  }, [detail?.spans]);

  const childrenByParent = useMemo(() => {
    const map = new Map<string, TraceSpan[]>();
    for (const span of detail?.spans ?? []) {
      if (!span.parent_span_id) continue;
      const list = map.get(span.parent_span_id);
      if (list) list.push(span);
      else map.set(span.parent_span_id, [span]);
    }
    return map;
  }, [detail?.spans]);

  const foldableUpstream = useMemo(() => {
    if (!detail) return null;
    return findFoldableUpstream(detail.start_ns, detail.end_ns, detail.spans);
  }, [detail]);

  const projector = useMemo(() => {
    if (!detail) return createWaterfallProjector(0, 1, "linear", null);
    return createWaterfallProjector(
      detail.start_ns,
      detail.end_ns,
      scaleMode,
      foldableUpstream,
    );
  }, [detail, scaleMode, foldableUpstream]);

  const visibleRows = useMemo(() => {
    if (!detail) return [];
    const visibleSpans: TraceSpan[] = [];
    let hideBelow: number | null = null;
    for (const span of detail.spans) {
      if (hideBelow !== null) {
        if (span.depth > hideBelow) continue;
        hideBelow = null;
      }
      visibleSpans.push(span);
      if (collapsed.has(span.span_id)) hideBelow = span.depth;
    }

    const indents = computeTreeConnectors(visibleSpans.map((s) => s.depth));
    const selfPrefix = t("ops.traces.detail.selfShort");

    return visibleSpans.map((span, idx) => {
      const startPct = projector.project(span.start_ns);
      const endPct = projector.project(span.end_ns);
      const safeLeft = Math.max(0, Math.min(100, startPct));
      const safeWidth = Math.min(
        Math.max(0.3, endPct - startPct),
        100 - safeLeft,
      );
      const isContainer = span.child_count > 0 && !collapsed.has(span.span_id);
      const directChildren = isContainer
        ? (childrenByParent.get(span.span_id) ?? [])
        : [];
      const selfSegments = isContainer
        ? computeSelfSegments(span, directChildren, projector.project)
        : [];
      const baseDur = formatTraceMs(span.duration_ns);
      const durLabel =
        span.child_count > 0 && span.self_duration_ns >= SELF_BADGE_THRESHOLD_NS
          ? `${baseDur} · ${selfPrefix} ${formatTraceMs(span.self_duration_ns)}`
          : baseDur;

      return {
        span,
        indent: indents[idx]!,
        left: safeLeft,
        width: safeWidth,
        isContainer,
        selfSegments,
        durLabel,
        durPlacement: resolveWaterfallDurPlacement(safeLeft, safeWidth),
      };
    });
  }, [detail, collapsed, projector, childrenByParent, t]);

  const selected = useMemo(
    () => detail?.spans.find((s) => s.span_id === selectedId) ?? null,
    [detail?.spans, selectedId],
  );

  const spanEvents = useMemo<TraceSpanEvent[]>(() => {
    const ev = selected?.events;
    return Array.isArray(ev) ? (ev as TraceSpanEvent[]) : [];
  }, [selected?.events]);

  const spanLinks = useMemo<TraceSpanLink[]>(() => {
    const links = selected?.links;
    if (!Array.isArray(links)) return [];
    return (links as TraceSpanLink[]).filter((l) => l && l.trace_id && l.span_id);
  }, [selected?.links]);

  const spanLinkedFrom = useMemo<LinkedSpan[]>(
    () =>
      (detail?.linked_from ?? []).filter(
        (l) => l.linked_span_id === selectedId,
      ),
    [detail?.linked_from, selectedId],
  );

  const selectSpan = (id: string) => {
    setSelectedId(id);
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        next.set("span", id);
        return next;
      },
      { replace: true },
    );
  };

  const openLinked = (targetTraceId: string, spanId: string) => {
    if (targetTraceId === traceId) {
      selectSpan(spanId);
      return;
    }
    void navigate(
      toLocationHref(
        buildTraceRoute(targetTraceId, {
          span: spanId,
          start: startHint.start,
          end: startHint.end,
        }),
      ),
    );
  };

  const metaItems = useMemo<EntityDetailMetaItem[]>(() => {
    if (!detail) return [];
    const items: EntityDetailMetaItem[] = [
      {
        label: t("ops.traces.detail.startedAt"),
        value: (
          <span className="font-mono-tech tabular-nums">
            {formatDateTime(Math.floor(detail.start_ns / 1e6))}
          </span>
        ),
      },
      {
        label: t("ops.traces.columns.duration"),
        value: (
          <span className="font-mono-tech tabular-nums">
            {detail.duration_ms.toFixed(2)} ms
          </span>
        ),
      },
    ];
    if (detail.upstream_wire_ns !== null) {
      items.push({
        label: t("ops.traces.detail.upstreamWire"),
        value: (
          <span className="font-mono-tech tabular-nums">
            {formatTraceMs(detail.upstream_wire_ns)}
          </span>
        ),
      });
    }
    if (detail.platform_overhead_ns !== null) {
      items.push({
        label: t("ops.traces.detail.platformOverhead"),
        value: (
          <span className="font-mono-tech tabular-nums">
            {formatTraceMs(detail.platform_overhead_ns)}
          </span>
        ),
      });
    }
    items.push(
      {
        label: t("ops.traces.columns.spans"),
        value: (
          <span className="font-mono-tech tabular-nums">
            {detail.spans.length}
          </span>
        ),
      },
      {
        label: t("ops.traces.columns.errors"),
        value: (
          <span
            className={cn(
              "font-mono-tech tabular-nums",
              errorCount > 0 && "text-danger-fg font-semibold",
            )}
          >
            {errorCount}
          </span>
        ),
      },
    );
    return items;
  }, [detail, errorCount, t]);

  const logColumns = useMemo<DataTableColumn<CallStageAccessRow>[]>(
    () => [
      {
        id: "timestamp",
        header: t("ops.calls.stages.columns.time"),
        minWidth: 164,
        mono: true,
        cell: (r) => formatDateTime(r.timestamp),
      },
      {
        id: "target",
        header: t("ops.calls.stages.columns.source"),
        minWidth: 130,
        accessorKey: "target",
      },
      {
        id: "request_id",
        header: t("ops.traces.columns.requestId"),
        minWidth: 240,
        cell: (r) => (
          <div className="flex items-center gap-1.5">
            <Button
              type="button"
              variant="link"
              size="xs"
              className="px-1 font-mono-tech"
              onClick={() =>
                void navigate(
                  toLocationHref(
                    buildCallsRoute({
                      tab: "customer",
                      filters: { request_id: r.request_id },
                    }),
                  ),
                )
              }
            >
              {r.request_id}
            </Button>
            <CopyableText value={r.request_id} displayText="" />
          </div>
        ),
      },
      {
        id: "supplier_code",
        header: t("ops.calls.stages.columns.supplier"),
        minWidth: 100,
        accessorKey: "supplier_code",
      },
      {
        id: "channel_code",
        header: t("ops.calls.stages.columns.channel"),
        minWidth: 150,
        cell: (r) =>
          r.channel_code ? (
            <CopyableText value={r.channel_code} mono />
          ) : (
            "—"
          ),
      },
      {
        id: "status_code",
        header: t("ops.calls.stages.columns.status"),
        width: 84,
        align: "right",
        mono: true,
        accessorKey: "status_code",
      },
      {
        id: "duration_ms",
        header: t("ops.calls.stages.columns.duration"),
        width: 100,
        align: "right",
        mono: true,
        cell: (r) => `${r.duration_ms} ms`,
      },
      {
        id: "upstream_service_time_ms",
        header: t("ops.traces.detail.upstreamDuration"),
        width: 120,
        align: "right",
        mono: true,
        cell: (r) => `${r.upstream_service_time_ms} ms`,
      },
      {
        id: "error_code",
        header: t("ops.calls.stages.columns.errorCode"),
        minWidth: 120,
        accessorKey: "error_code",
      },
    ],
    [t, navigate],
  );

  const billingColumns = useMemo<DataTableColumn<TraceBillingRow>[]>(
    () => [
      {
        id: "request_id",
        header: t("ops.traces.columns.requestId"),
        minWidth: 230,
        cell: (r) => <CopyableText value={r.request_id} mono />,
      },
      {
        id: "outbox_status",
        header: t("ops.traces.detail.outbox"),
        width: 110,
        accessorKey: "outbox_status",
      },
      {
        id: "retry_count",
        header: t("ops.traces.billing.retries"),
        width: 84,
        align: "right",
        mono: true,
        accessorKey: "retry_count",
      },
      {
        id: "product_id",
        header: t("ops.traces.form.product"),
        minWidth: 120,
        cell: (r) =>
          r.product_id ? <CopyableText value={r.product_id} mono /> : "—",
      },
      {
        id: "billable_units",
        header: t("ops.traces.billing.units"),
        width: 90,
        align: "right",
        mono: true,
        accessorKey: "billable_units",
      },
      {
        id: "billable_amount",
        header: t("ops.traces.billing.amount"),
        width: 120,
        align: "right",
        mono: true,
        cell: (r) => formatMoney(r.billable_amount),
      },
      {
        id: "charge_status",
        header: t("ops.traces.billing.charge"),
        width: 100,
        accessorKey: "charge_status",
      },
      {
        id: "processed_at",
        header: t("ops.traces.billing.processedAt"),
        minWidth: 164,
        mono: true,
        cell: (r) => (r.processed_at ? formatDateTime(r.processed_at) : "—"),
      },
      {
        id: "error_message",
        header: t("ops.traces.billing.error"),
        minWidth: 160,
        accessorKey: "error_message",
      },
    ],
    [t],
  );

  return (
    <EntityDetailLayout
      title={t("ops.traces.detail.title")}
      code={traceId}
      status={detail ? (errorCount > 0 ? "ERROR" : "OK") : undefined}
      badges={
        rootOperationName ? (
          <Badge variant="outline" className="font-mono-tech">
            {rootOperationName}
          </Badge>
        ) : undefined
      }
      meta={metaItems}
      onBack={() =>
        void navigate(toLocationHref(buildTraceSearchRoute()))
      }
    >
      {missing && (
        <div className="flex items-start gap-2.5 rounded-lg border border-warning-border bg-warning-bg px-4 py-3 text-xs text-warning-fg">
          <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
          <div className="space-y-0.5">
            <div className="font-semibold">
              {t("ops.traces.fallback.title")}
            </div>
            <div>{t("ops.traces.fallback.description")}</div>
          </div>
        </div>
      )}

      {loading && !detail ? (
        <Card className="flex items-center justify-center gap-2 py-16 text-xs text-muted-foreground">
          <Loader2 className="h-4 w-4 animate-spin text-primary" />
          <span>{t("layout.loading")}</span>
        </Card>
      ) : detail ? (
        <div className="grid grid-cols-1 gap-4 xl:grid-cols-[minmax(0,1fr)_380px] items-start">
          {/* Waterfall panel */}
          <Card className="p-3.5 space-y-2.5 overflow-x-auto">
            <div className="flex flex-wrap items-center gap-3 text-xs text-muted-foreground">
              {detail.services.map((svc) => (
                <span key={svc} className="inline-flex items-center gap-1.5">
                  <span
                    className="h-2.5 w-2.5 rounded-sm shrink-0"
                    style={{ backgroundColor: serviceColor(svc) }}
                  />
                  <span>{svc}</span>
                </span>
              ))}
              <span className="inline-flex items-center gap-1.5">
                <span className="h-2.5 w-2.5 rounded-sm border-2 border-warning-fg shrink-0" />
                <span>{t("ops.traces.detail.criticalPath")}</span>
              </span>
              <span className="inline-flex items-center gap-1.5">
                <span className="h-2.5 w-2.5 rounded-sm bg-danger-fg shrink-0" />
                <span>{t("ops.traces.detail.error")}</span>
              </span>

              <div className="ml-auto flex flex-wrap items-center gap-2">
                <Button
                  type="button"
                  variant="ghost"
                  size="xs"
                  onClick={() => setCollapsed(new Set())}
                >
                  {t("ops.traces.detail.expandAll")}
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  size="xs"
                  onClick={() =>
                    setCollapsed(
                      new Set(
                        detail.spans
                          .filter((s) => s.depth >= 1 && s.child_count > 0)
                          .map((s) => s.span_id),
                      ),
                    )
                  }
                >
                  {t("ops.traces.detail.collapseAll")}
                </Button>
                {foldableUpstream && (
                  <SegmentedControl
                    value={scaleMode}
                    onChange={setScaleMode}
                    items={[
                      {
                        key: "linear",
                        label: t("ops.traces.detail.scaleLinear"),
                      },
                      {
                        key: "fold-upstream",
                        label: t("ops.traces.detail.scaleFoldUpstream"),
                      },
                    ]}
                  />
                )}
              </div>
            </div>

            {/* Axis header */}
            <div className="grid grid-cols-[clamp(220px,35%,340px)_minmax(240px,1fr)] items-center">
              <div />
              <div className="relative h-5 border-b border-border text-[11px] font-mono-tech tabular-nums text-muted-foreground">
                {projector.ticks.map((tick, idx) => (
                  <span
                    key={`${tick.percent}-${idx}`}
                    style={
                      idx === projector.ticks.length - 1
                        ? { right: 0 }
                        : { left: `${tick.percent}%` }
                    }
                    className={cn(
                      "absolute whitespace-nowrap",
                      idx > 0 &&
                        idx < projector.ticks.length - 1 &&
                        "-translate-x-1/2",
                    )}
                  >
                    {tick.label}
                  </span>
                ))}
              </div>
            </div>

            {/* Span rows */}
            <div className="divide-y divide-border-subtle/50">
              {visibleRows.map((row) => {
                const isSelected = selectedId === row.span.span_id;
                const barColor = row.span.error
                  ? "var(--danger-fg)"
                  : serviceColor(row.span.service_name);
                return (
                  <div
                    key={row.span.span_id}
                    onClick={() => selectSpan(row.span.span_id)}
                    className={cn(
                      "grid grid-cols-[clamp(220px,35%,340px)_minmax(240px,1fr)] items-center min-h-[26px] text-xs cursor-pointer border-l-3 transition-colors",
                      row.span.critical
                        ? "border-l-warning-fg"
                        : "border-l-transparent",
                      isSelected
                        ? "bg-primary-soft/65"
                        : "hover:bg-hover-bg/65",
                    )}
                  >
                    <div className="flex items-center gap-1 h-6 pl-1 pr-2 overflow-hidden whitespace-nowrap">
                      <span className="inline-flex items-stretch h-6 shrink-0">
                        {row.indent.guides.map((g, gi) => (
                          <span
                            key={gi}
                            className="relative inline-block w-3.5 h-6"
                          >
                            {g === "pipe" && (
                              <span className="absolute inset-y-0 left-1.5 border-l border-border" />
                            )}
                          </span>
                        ))}
                        {row.indent.connector !== "none" && (
                          <span className="relative inline-block w-3.5 h-6">
                            <span
                              className={cn(
                                "absolute top-0 left-1.5 border-l border-border",
                                row.indent.connector === "tee" ? "h-6" : "h-3",
                              )}
                            />
                            <span className="absolute top-3 left-1.5 w-2 border-t border-border" />
                          </span>
                        )}
                      </span>

                      {row.span.child_count > 0 ? (
                        <button
                          type="button"
                          onClick={(e) => {
                            e.stopPropagation();
                            setCollapsed((prev) => {
                              const next = new Set(prev);
                              if (next.has(row.span.span_id)) {
                                next.delete(row.span.span_id);
                              } else {
                                next.add(row.span.span_id);
                              }
                              return next;
                            });
                          }}
                          className="w-3.5 text-center text-muted-foreground hover:text-foreground cursor-pointer shrink-0"
                        >
                          {collapsed.has(row.span.span_id) ? "▸" : "▾"}
                        </button>
                      ) : (
                        <span className="w-3.5 text-center text-muted-foreground/45 shrink-0">
                          ·
                        </span>
                      )}

                      <span
                        className="h-2 w-2 rounded-full shrink-0"
                        style={{
                          backgroundColor: serviceColor(row.span.service_name),
                        }}
                      />
                      <span className="text-muted-foreground shrink-0">
                        {row.span.service_name}
                      </span>
                      <span
                        title={row.span.span_name}
                        className={cn(
                          "truncate font-mono-tech",
                          row.span.error
                            ? "text-danger-fg font-medium"
                            : "text-foreground",
                        )}
                      >
                        {row.span.span_name}
                      </span>
                    </div>

                    <div className="relative h-[22px] overflow-hidden rounded-sm bg-muted/60">
                      {projector.breakZone && (
                        <span
                          style={{
                            left: `${projector.breakZone.left}%`,
                            width: `${projector.breakZone.width}%`,
                          }}
                          className="pointer-events-none absolute inset-y-0 border-x border-dashed border-border bg-muted/80"
                        />
                      )}

                      <span
                        style={{
                          left: `${row.left}%`,
                          width: `${row.width}%`,
                          backgroundColor: row.isContainer
                            ? "transparent"
                            : barColor,
                          borderColor: barColor,
                        }}
                        className={cn(
                          "absolute top-[5px] block h-3 rounded-sm",
                          row.isContainer ? "border" : "opacity-90",
                          row.span.critical && "ring-1 ring-warning-fg",
                        )}
                      />

                      {row.selfSegments.map((seg, si) => (
                        <span
                          key={si}
                          style={{
                            left: `${seg.left}%`,
                            width: `${seg.width}%`,
                            backgroundColor: barColor,
                          }}
                          className="pointer-events-none absolute top-[5px] block h-3 rounded-xs opacity-90"
                        />
                      ))}

                      <span
                        style={row.durPlacement.style}
                        className={cn(
                          "absolute top-[3px] z-10 rounded-xs px-1 text-[11px] leading-4 font-mono-tech tabular-nums whitespace-nowrap text-foreground bg-card/90",
                          row.durPlacement.mode === "inside-bar" &&
                            "border border-border-subtle",
                        )}
                      >
                        {row.durLabel}
                      </span>
                    </div>
                  </div>
                );
              })}
            </div>
          </Card>

          {/* Sticky Span Inspector */}
          <Card className="p-4 space-y-3 xl:sticky xl:top-4 xl:max-h-[calc(100vh-120px)] xl:overflow-y-auto">
            {selected ? (
              <>
                <div className="space-y-1.5">
                  <h3 className="text-sm font-semibold text-foreground break-all font-mono-tech">
                    {selected.span_name}
                  </h3>
                  <div className="flex flex-wrap items-center gap-1.5">
                    <span className="inline-flex items-center gap-1.5 rounded-md border border-border bg-muted/50 px-2 py-0.5 text-[11px] font-medium text-foreground">
                      <span
                        className="h-2 w-2 rounded-full"
                        style={{
                          backgroundColor: serviceColor(selected.service_name),
                        }}
                      />
                      {selected.service_name}
                    </span>
                    <Badge variant="outline">{selected.span_kind}</Badge>
                    {selected.error && (
                      <Badge variant="danger">{selected.status_code}</Badge>
                    )}
                    {selected.critical && (
                      <Badge variant="warning">
                        {t("ops.traces.detail.criticalPath")}
                      </Badge>
                    )}
                  </div>
                </div>

                <dl className="divide-y divide-border-subtle rounded-lg border border-border overflow-hidden text-xs">
                  <SpanDescRow label={t("ops.traces.detail.spanId")}>
                    <CopyableText value={selected.span_id} mono />
                  </SpanDescRow>
                  <SpanDescRow label={t("ops.traces.detail.parentSpan")}>
                    {selected.parent_span_id ? (
                      <CopyableText value={selected.parent_span_id} mono />
                    ) : (
                      "—"
                    )}
                  </SpanDescRow>
                  <SpanDescRow label={t("ops.traces.columns.duration")}>
                    <span className="font-mono-tech tabular-nums">
                      {formatTraceMs(selected.duration_ns)}
                    </span>
                  </SpanDescRow>
                  {selected.child_count > 0 && (
                    <SpanDescRow label={t("ops.traces.detail.selfDuration")}>
                      <span className="font-mono-tech tabular-nums">
                        {formatTraceMs(selected.self_duration_ns)}
                      </span>
                    </SpanDescRow>
                  )}
                  {selected.transit_req_ns !== null && (
                    <SpanDescRow label={t("ops.traces.detail.transitReq")}>
                      <span className="font-mono-tech tabular-nums">
                        {formatTraceMs(selected.transit_req_ns)}
                      </span>
                    </SpanDescRow>
                  )}
                  {selected.transit_resp_ns !== null && (
                    <SpanDescRow label={t("ops.traces.detail.transitResp")}>
                      <span className="font-mono-tech tabular-nums">
                        {formatTraceMs(selected.transit_resp_ns)}
                      </span>
                    </SpanDescRow>
                  )}
                  <SpanDescRow label={t("ops.traces.detail.offset")}>
                    <span className="font-mono-tech tabular-nums">
                      {formatTraceMs(selected.start_ns - detail.start_ns)}
                    </span>
                  </SpanDescRow>
                  {selected.status_message && (
                    <SpanDescRow label={t("ops.traces.detail.status")}>
                      {selected.status_message}
                    </SpanDescRow>
                  )}
                </dl>

                <Tabs
                  value={panelTab}
                  onChange={setPanelTab}
                  items={[
                    {
                      key: "attributes",
                      label: t("ops.traces.detail.attributes"),
                    },
                    {
                      key: "events",
                      label: t("ops.traces.detail.events"),
                      count: spanEvents.length,
                    },
                    {
                      key: "links",
                      label: t("ops.traces.detail.links"),
                      count: spanLinks.length + spanLinkedFrom.length,
                    },
                  ]}
                />

                {panelTab === "attributes" && (
                  <div className="space-y-3">
                    <KeyValueList value={selected.attributes} />
                    <div className="border-t border-border-subtle pt-2.5 space-y-1.5">
                      <h4 className="text-xs font-semibold text-foreground">
                        {t("ops.traces.detail.resource")}
                      </h4>
                      <KeyValueList value={selected.resource} />
                    </div>
                  </div>
                )}

                {panelTab === "events" && (
                  <div className="space-y-2">
                    {spanEvents.length === 0 ? (
                      <EmptyState title={t("ops.charts.empty")} />
                    ) : (
                      spanEvents.map((ev, i) => (
                        <div
                          key={i}
                          className={cn(
                            "rounded-md border border-border p-2.5 space-y-1.5 text-xs",
                            isErrorEvent(ev) && "border-l-3 border-l-danger-fg",
                          )}
                        >
                          <div className="flex items-center justify-between gap-2">
                            <span className="font-semibold text-foreground">
                              {ev.name || t("ops.traces.detail.event")}
                            </span>
                            <span className="font-mono-tech text-[11px] text-muted-foreground">
                              {typeof ev.time === "number"
                                ? `+${formatTraceMs(ev.time - detail.start_ns)}`
                                : (ev.time ?? "")}
                            </span>
                          </div>
                          <KeyValueList value={ev.attributes ?? {}} />
                        </div>
                      ))
                    )}
                  </div>
                )}

                {panelTab === "links" && (
                  <div className="space-y-2 text-xs">
                    {spanLinks.length === 0 && spanLinkedFrom.length === 0 ? (
                      <EmptyState title={t("ops.charts.empty")} />
                    ) : (
                      <>
                        {spanLinks.map((link) => (
                          <div
                            key={`${link.trace_id}-${link.span_id}`}
                            className="flex flex-wrap items-center gap-1.5"
                          >
                            <span className="text-muted-foreground">
                              {t("ops.traces.detail.linksTo")}
                            </span>
                            <Button
                              type="button"
                              variant="link"
                              size="xs"
                              className="px-1 font-mono-tech"
                              onClick={() =>
                                openLinked(link.trace_id, link.span_id)
                              }
                            >
                              {link.trace_id} / {link.span_id}
                            </Button>
                          </div>
                        ))}
                        {spanLinkedFrom.map((link) => (
                          <div
                            key={`${link.trace_id}-${link.span_id}`}
                            className="flex flex-wrap items-center gap-1.5"
                          >
                            <span className="text-muted-foreground">
                              {t("ops.traces.detail.linkedFrom")}
                            </span>
                            <Button
                              type="button"
                              variant="link"
                              size="xs"
                              className="px-1"
                              onClick={() =>
                                openLinked(link.trace_id, link.span_id)
                              }
                            >
                              {link.service_name} · {link.span_name}
                            </Button>
                          </div>
                        ))}
                      </>
                    )}
                  </div>
                )}
              </>
            ) : (
              <EmptyState title={t("ops.traces.detail.selectSpan")} />
            )}
          </Card>
        </div>
      ) : null}

      {detail && detail.linked_from.length > 0 && (
        <Card className="p-4 space-y-2">
          <h3 className="text-xs font-semibold text-foreground">
            {t("ops.traces.detail.asyncTitle")}
          </h3>
          <div className="space-y-1.5">
            {detail.linked_from.map((link) => (
              <div
                key={`${link.trace_id}-${link.span_id}`}
                className="flex flex-wrap items-center gap-2 text-xs"
              >
                <Badge variant="outline">{link.service_name}</Badge>
                <Button
                  type="button"
                  variant="link"
                  size="xs"
                  onClick={() => openLinked(link.trace_id, link.span_id)}
                >
                  {link.span_name} · {link.trace_id}
                </Button>
                <span className="font-mono-tech text-muted-foreground">
                  ← {link.linked_span_id}
                </span>
              </div>
            ))}
          </div>
        </Card>
      )}

      {/* Related Logs & Billing */}
      <Card className="overflow-hidden">
        <div className="px-4 pt-2">
          <Tabs
            value={bottomTab}
            onChange={setBottomTab}
            items={[
              {
                key: "logs",
                label: t("ops.traces.detail.logs"),
                count: calls.length,
              },
              {
                key: "billing",
                label: t("ops.traces.detail.billing"),
                count: billing.length,
              },
            ]}
          />
        </div>

        {bottomTab === "logs" ? (
          <DataTablePage
            columns={logColumns}
            data={calls}
            getRowId={(r, i) => `${r.request_id}-${i}`}
            loading={callsLoading}
            className="[&>div]:border-0 [&>div]:rounded-none"
          />
        ) : billingError ? (
          <div className="p-4 text-xs text-muted-foreground">
            {billingError}
          </div>
        ) : (
          <DataTablePage
            columns={billingColumns}
            data={billing}
            getRowId={(r, i) => `${r.request_id}-${r.event_id}-${i}`}
            loading={billingLoading}
            className="[&>div]:border-0 [&>div]:rounded-none"
          />
        )}
      </Card>
    </EntityDetailLayout>
  );
}

function SpanDescRow({
  label,
  children,
}: {
  label: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className="grid grid-cols-12 gap-2 px-3 py-1.5">
      <dt className="col-span-4 text-muted-foreground">{label}</dt>
      <dd className="col-span-8 min-w-0 text-foreground break-words">
        {children}
      </dd>
    </div>
  );
}
