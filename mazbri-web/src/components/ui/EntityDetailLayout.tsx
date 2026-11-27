import React from "react";
import { ArrowLeft } from "lucide-react";
import { cn } from "./cn";
import { CopyableText } from "./CopyableText";
import {
  Card,
  IconButton,
  StatusBadge,
  Tabs,
  type TabOption,
} from "./primitives";
import { useUiTx } from "./tx";

export interface EntityDetailMetaItem {
  readonly label: React.ReactNode;
  readonly value: React.ReactNode;
}

export interface EntityDetailLayoutProps<K extends string = string> {
  title: React.ReactNode;
  code?: string;
  status?: string | null;
  statusLabel?: React.ReactNode;
  badges?: React.ReactNode;
  meta?: readonly EntityDetailMetaItem[] | React.ReactNode;
  onBack?: () => void;
  backLabel?: string;
  actions?: React.ReactNode;
  tabs?: readonly TabOption<K>[];
  activeTab?: K;
  onTabChange?: (next: K) => void;
  children: React.ReactNode;
  className?: string;
  testId?: string;
}

function isMetaItemList(
  meta: readonly EntityDetailMetaItem[] | React.ReactNode,
): meta is readonly EntityDetailMetaItem[] {
  return Array.isArray(meta);
}

export function EntityDetailLayout<K extends string = string>({
  title,
  code,
  status,
  statusLabel,
  badges,
  meta,
  onBack,
  backLabel,
  actions,
  tabs,
  activeTab,
  onTabChange,
  children,
  className,
  testId,
}: EntityDetailLayoutProps<K>) {
  const { tx } = useUiTx();
  const resolvedBackLabel = backLabel ?? tx("返回", "Back");

  return (
    <div data-testid={testId} className={cn("space-y-4 min-w-0", className)}>
      <Card className="px-4 py-3.5 space-y-3">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="flex items-start gap-2.5 min-w-0">
            {onBack && (
              <IconButton
                size="md"
                onClick={onBack}
                aria-label={resolvedBackLabel}
                title={resolvedBackLabel}
                className="mt-0.5 border border-control-border bg-control-bg hover:bg-control-hover hover:border-border-hover"
              >
                <ArrowLeft className="h-4 w-4" />
              </IconButton>
            )}
            <div className="min-w-0 space-y-1">
              <div className="flex flex-wrap items-center gap-2">
                <h1 className="text-base font-semibold tracking-tight text-foreground truncate">
                  {title}
                </h1>
                {status !== undefined && status !== null && (
                  <StatusBadge status={status} label={statusLabel} />
                )}
                {badges}
              </div>
              {code && (
                <div className="text-xs text-muted-foreground">
                  <CopyableText value={code} mono />
                </div>
              )}
            </div>
          </div>

          {actions && (
            <div className="flex flex-wrap items-center gap-2 shrink-0">
              {actions}
            </div>
          )}
        </div>

        {meta && (
          <div className="flex flex-wrap items-center gap-x-5 gap-y-1.5 border-t border-border-subtle pt-2.5 text-xs">
            {isMetaItemList(meta)
              ? meta.map((item, idx) => (
                  <div
                    key={idx}
                    className="inline-flex items-center gap-1.5 min-w-0"
                  >
                    <span className="text-muted-foreground">{item.label}</span>
                    <span className="font-medium text-foreground">
                      {item.value}
                    </span>
                  </div>
                ))
              : meta}
          </div>
        )}
      </Card>

      {tabs && tabs.length > 0 && activeTab !== undefined && onTabChange && (
        <Tabs items={tabs} value={activeTab} onChange={onTabChange} />
      )}

      <div className="space-y-4 min-w-0">{children}</div>
    </div>
  );
}

export interface EntityDetailCardProps {
  title: React.ReactNode;
  icon?: React.ReactNode;
  actions?: React.ReactNode;
  children: React.ReactNode;
  className?: string;
  bodyClassName?: string;
  testId?: string;
}

export function EntityDetailCard({
  title,
  icon,
  actions,
  children,
  className,
  bodyClassName,
  testId,
}: EntityDetailCardProps) {
  return (
    <Card data-testid={testId} className={cn("overflow-hidden", className)}>
      <div className="flex flex-wrap items-center justify-between gap-2 border-b border-border-subtle px-4 py-3">
        <div className="flex items-center gap-2 min-w-0 text-sm font-semibold text-foreground">
          {icon && (
            <span className="shrink-0 text-muted-foreground">{icon}</span>
          )}
          <span className="truncate">{title}</span>
        </div>
        {actions && (
          <div className="flex flex-wrap items-center gap-2 shrink-0">
            {actions}
          </div>
        )}
      </div>
      <div className={cn("p-4", bodyClassName)}>{children}</div>
    </Card>
  );
}
