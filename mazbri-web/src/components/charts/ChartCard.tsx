import React from "react";
import { AlertCircle, BarChart3, Info, Loader2 } from "lucide-react";
import { Card, Tooltip, cn } from "@/components/ui";
import { useT } from "@/locales";

export interface ChartCardProps {
  title: React.ReactNode;
  info?: string;
  extra?: React.ReactNode;
  loading?: boolean;
  error?: string | null;
  empty?: boolean;
  emptyText?: React.ReactNode;
  height?: number | string;
  borderless?: boolean;
  className?: string;
  bodyClassName?: string;
  testId?: string;
  children: React.ReactNode;
}

export function ChartCard({
  title,
  info,
  extra,
  loading = false,
  error = null,
  empty = false,
  emptyText,
  height = 220,
  borderless = false,
  className,
  bodyClassName,
  testId,
  children,
}: ChartCardProps) {
  const { t } = useT();
  const resolvedEmptyText = emptyText ?? t("ops.charts.empty");
  const styleHeight = typeof height === "number" ? `${height}px` : height;

  const content = (
    <>
      <div className="flex flex-wrap items-center justify-between gap-2 px-4 pt-3.5 pb-1">
        <div className="flex items-center gap-1.5 min-w-0">
          <h3 className="text-xs font-semibold text-foreground truncate">
            {title}
          </h3>
          {info && (
            <Tooltip content={info}>
              <span className="inline-flex items-center text-muted-foreground hover:text-foreground cursor-help shrink-0">
                <Info className="h-3.5 w-3.5" />
              </span>
            </Tooltip>
          )}
        </div>
        {extra && (
          <div className="flex items-center gap-2 shrink-0 text-xs text-muted-foreground">
            {extra}
          </div>
        )}
      </div>

      <div className={cn("relative p-3", bodyClassName)}>
        {error ? (
          <div
            style={{ height: styleHeight }}
            className="flex flex-col items-center justify-center gap-2 text-center text-xs text-danger-fg"
          >
            <AlertCircle className="h-5 w-5 shrink-0 opacity-80" />
            <span>{error}</span>
          </div>
        ) : loading && empty ? (
          <div
            style={{ height: styleHeight }}
            className="flex items-center justify-center gap-2 text-xs text-muted-foreground"
          >
            <Loader2 className="h-4 w-4 animate-spin text-primary" />
            <span>{t("layout.loading")}</span>
          </div>
        ) : empty ? (
          <div
            style={{ height: styleHeight }}
            className="flex flex-col items-center justify-center gap-1.5 text-center text-xs text-muted-foreground"
          >
            <BarChart3 className="h-4 w-4 opacity-45" />
            <span>{resolvedEmptyText}</span>
          </div>
        ) : (
          <div className="relative">
            {loading && (
              <div className="absolute inset-0 z-10 flex items-center justify-center rounded-md bg-card/65 backdrop-blur-[1px]">
                <Loader2 className="h-4 w-4 animate-spin text-primary" />
              </div>
            )}
            {children}
          </div>
        )}
      </div>
    </>
  );

  if (borderless) {
    return (
      <div
        data-testid={testId}
        className={cn(
          "rounded-lg border border-border-subtle bg-card text-card-foreground overflow-hidden",
          className,
        )}
      >
        {content}
      </div>
    );
  }

  return (
    <Card data-testid={testId} className={cn("overflow-hidden", className)}>
      {content}
    </Card>
  );
}
