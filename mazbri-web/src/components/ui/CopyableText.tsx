import React, { useEffect, useRef, useState } from "react";
import { Check, Copy } from "lucide-react";
import { copyWithFeedback } from "@/utils/ui/copy-feedback";
import { cn } from "./cn";
import { Tooltip } from "./primitives";
import { useUiTx } from "./tx";

export interface CopyableTextProps {
  value: string;
  displayText?: React.ReactNode;
  mono?: boolean;
  truncate?: boolean;
  maxWidthClass?: string;
  copyTooltip?: string;
  copiedTooltip?: string;
  className?: string;
  textClassName?: string;
}

export function CopyableText({
  value,
  displayText,
  mono = true,
  truncate = false,
  maxWidthClass,
  copyTooltip,
  copiedTooltip,
  className,
  textClassName,
}: CopyableTextProps) {
  const { tx } = useUiTx();
  const [copied, setCopied] = useState(false);
  const timerRef = useRef<number | null>(null);

  useEffect(() => {
    return () => {
      if (timerRef.current !== null) {
        window.clearTimeout(timerRef.current);
      }
    };
  }, []);

  const handleCopy = async (e: React.MouseEvent<HTMLButtonElement>) => {
    e.stopPropagation();
    const ok = await copyWithFeedback(value);
    if (!ok) return;
    setCopied(true);
    if (timerRef.current !== null) {
      window.clearTimeout(timerRef.current);
    }
    timerRef.current = window.setTimeout(() => {
      setCopied(false);
      timerRef.current = null;
    }, 1500);
  };

  const label = copied
    ? (copiedTooltip ?? tx("已复制", "Copied"))
    : (copyTooltip ?? tx("复制", "Copy"));

  return (
    <span
      className={cn(
        "group/copy inline-flex items-center gap-1 min-w-0 max-w-full",
        className,
      )}
    >
      <span
        className={cn(
          mono && "font-mono-tech",
          truncate && "truncate",
          maxWidthClass,
          textClassName,
        )}
        title={truncate ? value : undefined}
      >
        {displayText ?? value}
      </span>
      <Tooltip content={label}>
        <button
          type="button"
          onClick={handleCopy}
          aria-label={label}
          className={cn(
            "inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-hover-bg hover:text-foreground transition-opacity duration-150 cursor-pointer",
            copied
              ? "opacity-100"
              : "opacity-0 group-hover/copy:opacity-100 group-hover/row:opacity-75 hover:!opacity-100 focus-visible:opacity-100",
          )}
        >
          {copied ? (
            <Check className="h-3 w-3 text-success-fg" />
          ) : (
            <Copy className="h-3 w-3" />
          )}
        </button>
      </Tooltip>
    </span>
  );
}
