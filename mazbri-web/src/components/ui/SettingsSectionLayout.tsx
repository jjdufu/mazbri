import React from "react";
import { cn } from "./cn";
import { Button, Card, PageHeader } from "./primitives";
import { useUiTx } from "./tx";

export interface SettingsSectionLayoutProps {
  title?: React.ReactNode;
  subtitle?: React.ReactNode;
  actions?: React.ReactNode;
  columns?: 1 | 2;
  dirty?: boolean;
  saving?: boolean;
  onSave?: () => void;
  onReset?: () => void;
  dirtyLabel?: React.ReactNode;
  saveLabel?: React.ReactNode;
  resetLabel?: React.ReactNode;
  dirtyActions?: React.ReactNode;
  children: React.ReactNode;
  className?: string;
  testId?: string;
}

export function SettingsSectionLayout({
  title,
  subtitle,
  actions,
  columns = 2,
  dirty = false,
  saving = false,
  onSave,
  onReset,
  dirtyLabel,
  saveLabel,
  resetLabel,
  dirtyActions,
  children,
  className,
  testId,
}: SettingsSectionLayoutProps) {
  const { tx } = useUiTx();

  return (
    <div data-testid={testId} className={cn("space-y-4 min-w-0", className)}>
      {title && (
        <PageHeader title={title} subtitle={subtitle} actions={actions} />
      )}

      <div
        className={cn(
          "grid gap-4 items-stretch",
          columns === 2 ? "grid-cols-1 lg:grid-cols-2" : "grid-cols-1",
        )}
      >
        {children}
      </div>

      {dirty && (
        <div className="sticky bottom-3 z-20 flex justify-end pointer-events-none">
          <div className="pointer-events-auto inline-flex items-center gap-3 rounded-lg border border-primary/35 bg-card px-3.5 py-2 shadow-lg">
            <span className="text-xs font-medium text-foreground">
              {dirtyLabel ?? tx("有未保存改动", "Unsaved changes")}
            </span>
            <div className="flex items-center gap-2">
              {dirtyActions ?? (
                <>
                  {onReset && (
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      disabled={saving}
                      onClick={onReset}
                    >
                      {resetLabel ?? tx("重置", "Reset")}
                    </Button>
                  )}
                  {onSave && (
                    <Button
                      type="button"
                      variant="primary"
                      size="sm"
                      loading={saving}
                      onClick={onSave}
                    >
                      {saveLabel ?? tx("保存修改", "Save")}
                    </Button>
                  )}
                </>
              )}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

export interface SettingsSectionCardProps {
  title: React.ReactNode;
  icon?: React.ReactNode;
  actions?: React.ReactNode;
  children: React.ReactNode;
  className?: string;
  bodyClassName?: string;
  testId?: string;
}

export function SettingsSectionCard({
  title,
  icon,
  actions,
  children,
  className,
  bodyClassName,
  testId,
}: SettingsSectionCardProps) {
  return (
    <Card
      data-testid={testId}
      className={cn("flex flex-col h-full overflow-hidden", className)}
    >
      <div className="flex items-center justify-between gap-3 border-b border-border-subtle px-4 py-3 shrink-0">
        <div className="flex items-center gap-2 min-w-0 text-sm font-semibold text-foreground">
          {icon && (
            <span className="shrink-0 text-primary">{icon}</span>
          )}
          <span className="truncate">{title}</span>
        </div>
        {actions && (
          <div className="flex items-center gap-2 shrink-0">{actions}</div>
        )}
      </div>
      <div className={cn("p-4 flex-1 space-y-4", bodyClassName)}>
        {children}
      </div>
    </Card>
  );
}

export interface SettingRowProps {
  label: React.ReactNode;
  htmlFor?: string;
  hint?: React.ReactNode;
  error?: React.ReactNode;
  required?: boolean;
  layout?: "vertical" | "inline";
  children: React.ReactNode;
  className?: string;
}

export function SettingRow({
  label,
  htmlFor,
  hint,
  error,
  required = false,
  layout = "vertical",
  children,
  className,
}: SettingRowProps) {
  if (layout === "inline") {
    return (
      <div
        className={cn(
          "flex items-center justify-between gap-4 min-w-0",
          className,
        )}
      >
        <div className="min-w-0 space-y-0.5">
          <label
            htmlFor={htmlFor}
            className="text-xs font-medium text-foreground leading-snug inline-flex items-center gap-1"
          >
            <span>{label}</span>
            {required && (
              <span aria-hidden="true" className="text-danger-fg">
                *
              </span>
            )}
          </label>
          {hint && (
            <p className="text-[11px] text-muted-foreground leading-snug">
              {hint}
            </p>
          )}
          {error && (
            <p className="text-[11px] text-danger-fg leading-snug">{error}</p>
          )}
        </div>
        <div className="shrink-0">{children}</div>
      </div>
    );
  }

  return (
    <div className={cn("flex flex-col gap-1.5 min-w-0", className)}>
      <label
        htmlFor={htmlFor}
        className="text-xs font-medium text-foreground leading-snug inline-flex items-center gap-1"
      >
        <span>{label}</span>
        {required && (
          <span aria-hidden="true" className="text-danger-fg">
            *
          </span>
        )}
      </label>
      <div className="min-w-0">{children}</div>
      {error ? (
        <p className="text-[11px] text-danger-fg leading-snug">{error}</p>
      ) : hint ? (
        <p className="text-[11px] text-muted-foreground leading-snug">{hint}</p>
      ) : null}
    </div>
  );
}
