import React, { useMemo, useState } from "react";
import { createTwoFilesPatch } from "diff";
import { html as diff2html } from "diff2html";
import "diff2html/bundles/css/diff2html.min.css";
import DOMPurify from "dompurify";
import { cn } from "./cn";
import { EmptyState, SegmentedControl } from "./primitives";
import { useUiTx } from "./tx";

export type DiffOutputFormat = "line-by-line" | "side-by-side";

export interface DiffViewerProps {
  oldValue: string;
  newValue: string;
  fileName?: string;
  oldLabel?: string;
  newLabel?: string;
  outputFormat?: DiffOutputFormat;
  hideFormatToggle?: boolean;
  className?: string;
  testId?: string;
}

export function DiffViewer({
  oldValue,
  newValue,
  fileName = "config",
  oldLabel,
  newLabel,
  outputFormat: initialFormat = "side-by-side",
  hideFormatToggle = false,
  className,
  testId,
}: DiffViewerProps) {
  const { tx } = useUiTx();
  const [format, setFormat] = useState<DiffOutputFormat>(initialFormat);

  const sanitizedHtml = useMemo(() => {
    if (oldValue === newValue) return "";
    const patch = createTwoFilesPatch(
      fileName,
      fileName,
      oldValue,
      newValue,
      oldLabel ?? tx("变更前", "Before"),
      newLabel ?? tx("变更后", "After"),
      { context: 3 },
    );
    const rawHtml = diff2html(patch, {
      drawFileList: false,
      matching: "lines",
      outputFormat: format,
    });
    return DOMPurify.sanitize(rawHtml);
  }, [oldValue, newValue, fileName, oldLabel, newLabel, format, tx]);

  return (
    <div data-testid={testId} className={cn("space-y-2.5 min-w-0", className)}>
      {!hideFormatToggle && (
        <div className="flex items-center justify-end">
          <SegmentedControl<DiffOutputFormat>
            value={format}
            onChange={setFormat}
            items={[
              { key: "side-by-side", label: tx("并排对比", "Side by side") },
              { key: "line-by-line", label: tx("逐行对比", "Line by line") },
            ]}
          />
        </div>
      )}

      {oldValue === newValue ? (
        <EmptyState title={tx("内容无差异", "No differences")} />
      ) : (
        <div
          className="overflow-x-auto rounded-lg"
          dangerouslySetInnerHTML={{ __html: sanitizedHtml }}
        />
      )}
    </div>
  );
}
