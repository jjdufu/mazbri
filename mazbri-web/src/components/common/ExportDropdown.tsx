import React, { useState } from "react";
import { ChevronDown, Download } from "lucide-react";
import { Button, Popover, Tooltip, type ButtonProps } from "@/components/ui";
import { useT } from "@/locales";

export interface ExportDropdownItem {
  readonly command: string;
  readonly label: string;
  readonly disabled?: boolean;
}

export interface ExportDropdownProps {
  items: readonly ExportDropdownItem[];
  onCommand: (command: string) => void;
  label?: string;
  disabled?: boolean;
  disabledReason?: string;
  loading?: boolean;
  variant?: ButtonProps["variant"];
  size?: ButtonProps["size"];
  icon?: React.ReactNode;
  testId?: string;
}

export function ExportDropdown({
  items,
  onCommand,
  label,
  disabled = false,
  disabledReason,
  loading = false,
  variant = "outline",
  size = "md",
  icon,
  testId,
}: ExportDropdownProps) {
  const { t } = useT();
  const [open, setOpen] = useState(false);
  const resolvedLabel = label ?? t("common.actions.export");

  const triggerBtn = (
    <Button
      type="button"
      variant={variant}
      size={size}
      disabled={disabled}
      loading={loading}
      data-testid={testId}
      onClick={() => {
        if (!disabled) setOpen((prev) => !prev);
      }}
    >
      {!loading && (icon ?? <Download className="h-3.5 w-3.5" />)}
      <span>{resolvedLabel}</span>
      <ChevronDown className="h-3.5 w-3.5 opacity-70" />
    </Button>
  );

  if (disabled && disabledReason) {
    return <Tooltip content={disabledReason}>{triggerBtn}</Tooltip>;
  }

  return (
    <Popover
      open={open}
      onOpenChange={setOpen}
      align="right"
      trigger={triggerBtn}
      contentClassName="min-w-[152px] p-1"
    >
      <div className="flex flex-col gap-0.5">
        {items.map((item) => (
          <button
            key={item.command}
            type="button"
            disabled={item.disabled}
            onClick={() => {
              setOpen(false);
              onCommand(item.command);
            }}
            className="flex w-full items-center rounded-md px-2.5 py-1.5 text-left text-xs text-foreground transition-colors hover:bg-hover-bg disabled:pointer-events-none disabled:opacity-45 cursor-pointer whitespace-nowrap"
          >
            {item.label}
          </button>
        ))}
      </div>
    </Popover>
  );
}
