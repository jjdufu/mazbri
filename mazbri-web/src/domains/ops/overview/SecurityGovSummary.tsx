import React, { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router";
import { Loader2 } from "lucide-react";
import { Badge, Button, Card, cn } from "@/components/ui";
import { postOlapQuery } from "@/api/olap";
import { useT } from "@/locales";
import { EMPTY_PLACEHOLDER } from "@/utils/format";
import { buildCallsRoute } from "@/utils/ops/opsRoutes";
import { toLocationHref } from "../shared/opsHelpers";

interface SecurityTopRow {
  dim_value: string;
  fail_count: number;
  req_count: number;
}

export interface SecurityGovSummaryProps {
  start: number;
  end: number;
  reloadTick: number;
}

export function SecurityGovSummary({
  start,
  end,
  reloadTick,
}: SecurityGovSummaryProps) {
  const { t } = useT();
  const navigate = useNavigate();
  const [loading, setLoading] = useState(false);
  const [topIp, setTopIp] = useState<SecurityTopRow[]>([]);
  const [topKey, setTopKey] = useState<SecurityTopRow[]>([]);
  const seqRef = useRef(0);

  useEffect(() => {
    if (!start || !end || end <= start) return;
    const mine = ++seqRef.current;
    setLoading(true);
    Promise.all([
      postOlapQuery<SecurityTopRow>({
        template: "security_top",
        params: { start, end, dim: "ip", limit: 5 },
      }),
      postOlapQuery<SecurityTopRow>({
        template: "security_top",
        params: { start, end, dim: "key", limit: 5 },
      }),
    ])
      .then(([ipRes, keyRes]) => {
        if (mine !== seqRef.current) return;
        setTopIp(ipRes.rows ?? []);
        setTopKey(keyRes.rows ?? []);
      })
      .catch(() => {
        if (mine !== seqRef.current) return;
        setTopIp([]);
        setTopKey([]);
      })
      .finally(() => {
        if (mine === seqRef.current) setLoading(false);
      });
  }, [start, end, reloadTick]);

  const openCalls = (filters?: { client_ip?: string; api_key_id?: string }) => {
    const target = buildCallsRoute({
      tab: "customer",
      start,
      end,
      filters,
    });
    void navigate(toLocationHref(target));
  };

  const renderPanel = (
    title: string,
    items: SecurityTopRow[],
    dim: "client_ip" | "api_key_id",
  ) => (
    <Card className="overflow-hidden">
      <div className="flex items-center justify-between border-b border-border px-4 py-2.5">
        <h3 className="text-xs font-semibold text-foreground">{title}</h3>
        <Button
          type="button"
          variant="link"
          size="xs"
          onClick={() => openCalls()}
        >
          {t("ops.overview.securityGov.openCalls")}
        </Button>
      </div>
      <div className="relative min-h-[120px] p-2">
        {loading && (
          <div className="absolute inset-0 z-10 flex items-center justify-center bg-card/65">
            <Loader2 className="h-4 w-4 animate-spin text-primary" />
          </div>
        )}
        {!loading && items.length === 0 ? (
          <div className="flex min-h-[104px] items-center justify-center text-xs text-muted-foreground">
            {t("ops.overview.securityGov.empty")}
          </div>
        ) : (
          <div className="divide-y divide-border-subtle">
            {items.map((row, idx) => (
              <button
                key={`${row.dim_value}-${idx}`}
                type="button"
                onClick={() => openCalls({ [dim]: row.dim_value })}
                className="flex w-full items-center justify-between gap-3 rounded-md px-2.5 py-2 text-left text-xs transition-colors hover:bg-hover-bg cursor-pointer"
              >
                <span className="flex items-center gap-2 min-w-0">
                  <span
                    className={cn(
                      "inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-md text-[11px] font-semibold font-mono-tech",
                      idx < 3
                        ? "bg-danger-fg/12 text-danger-fg"
                        : "bg-muted text-muted-foreground",
                    )}
                  >
                    {idx + 1}
                  </span>
                  <span className="truncate font-mono-tech text-foreground">
                    {row.dim_value || EMPTY_PLACEHOLDER}
                  </span>
                </span>
                <Badge variant="danger" className="font-mono-tech tabular-nums">
                  {row.fail_count}
                </Badge>
              </button>
            ))}
          </div>
        )}
      </div>
    </Card>
  );

  return (
    <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
      {renderPanel(t("ops.overview.securityGov.topIp"), topIp, "client_ip")}
      {renderPanel(t("ops.overview.securityGov.topKey"), topKey, "api_key_id")}
    </div>
  );
}
