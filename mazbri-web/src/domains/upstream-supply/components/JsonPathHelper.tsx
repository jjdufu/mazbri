import { useState } from "react";
import { ChevronDown, ChevronRight, Compass } from "lucide-react";
import { Button, Popover, Textarea } from "@/components/ui";
import { useT } from "@/locales";
import { showErrorToast, showWarningToast } from "@/utils/ui/message-guard";

export interface JsonPathHelperProps {
  plain?: boolean;
  disabled?: boolean;
  seedJson?: string;
  onSelect: (path: string) => void;
}

interface TreeNode {
  name: string;
  jsonPath: string;
  valuePreview?: string;
  children?: TreeNode[];
}

function valueToTreeNodes(val: unknown, path = "$"): TreeNode[] {
  if (val === null) {
    return [{ name: "null", jsonPath: path, valuePreview: "null" }];
  }
  if (Array.isArray(val)) {
    return val.map((item, idx) => {
      const currentPath = `${path}[${idx}]`;
      const isPrimitive = typeof item !== "object" || item === null;
      return {
        name: `[${idx}]`,
        jsonPath: currentPath,
        valuePreview: isPrimitive ? String(item) : undefined,
        children: isPrimitive ? undefined : valueToTreeNodes(item, currentPath),
      };
    });
  }
  if (typeof val === "object") {
    return Object.entries(val as Record<string, unknown>).map(([key, item]) => {
      const needsBrackets = /[^a-zA-Z0-9_]/.test(key);
      const currentPath = needsBrackets
        ? `${path}['${key}']`
        : `${path}.${key}`;
      const isPrimitive = typeof item !== "object" || item === null;
      return {
        name: key,
        jsonPath: currentPath,
        valuePreview: isPrimitive ? String(item) : undefined,
        children: isPrimitive ? undefined : valueToTreeNodes(item, currentPath),
      };
    });
  }
  return [];
}

function TreeNodeRow({
  node,
  depth,
  onSelect,
}: {
  node: TreeNode;
  depth: number;
  onSelect: (path: string) => void;
}) {
  const [expanded, setExpanded] = useState(true);
  const hasChildren = Boolean(node.children && node.children.length > 0);

  return (
    <div>
      <div
        className="flex items-center gap-1 rounded-md px-1.5 py-1 text-xs hover:bg-hover-bg cursor-pointer"
        style={{ paddingLeft: `${depth * 14 + 6}px` }}
        onClick={() => onSelect(node.jsonPath)}
      >
        {hasChildren ? (
          <button
            type="button"
            className="inline-flex h-4 w-4 items-center justify-center text-muted-foreground hover:text-foreground shrink-0 cursor-pointer"
            onClick={(e) => {
              e.stopPropagation();
              setExpanded((prev) => !prev);
            }}
          >
            {expanded ? (
              <ChevronDown className="h-3 w-3" />
            ) : (
              <ChevronRight className="h-3 w-3" />
            )}
          </button>
        ) : (
          <span className="w-4 shrink-0" />
        )}
        <span className="font-mono-tech font-semibold text-foreground shrink-0">
          {node.name}
        </span>
        {node.valuePreview !== undefined && (
          <span className="text-muted-foreground truncate italic">
            : {node.valuePreview}
          </span>
        )}
      </div>
      {hasChildren && expanded && (
        <div>
          {node.children!.map((child) => (
            <TreeNodeRow
              key={child.jsonPath}
              node={child}
              depth={depth + 1}
              onSelect={onSelect}
            />
          ))}
        </div>
      )}
    </div>
  );
}

export function JsonPathHelper({
  plain = false,
  disabled = false,
  seedJson = "",
  onSelect,
}: JsonPathHelperProps) {
  const { t } = useT();
  const [open, setOpen] = useState(false);
  const [step, setStep] = useState<"input" | "tree">("input");
  const [rawJsonInput, setRawJsonInput] = useState("");
  const [treeData, setTreeData] = useState<TreeNode[]>([]);

  const handleOpenChange = (next: boolean) => {
    if (disabled) return;
    setOpen(next);
    if (next) {
      setStep("input");
      setRawJsonInput(seedJson || "");
      setTreeData([]);
      if (seedJson.trim()) {
        try {
          const parsed: unknown = JSON.parse(seedJson.trim());
          if (typeof parsed === "object" && parsed !== null) {
            setTreeData(valueToTreeNodes(parsed, "$"));
            setStep("tree");
          }
        } catch {
          // fall back to input step
        }
      }
    }
  };

  const handleParse = () => {
    try {
      const parsed: unknown = JSON.parse(rawJsonInput.trim());
      if (typeof parsed !== "object" || parsed === null) {
        showWarningToast(t("supplier.jsonPath.invalidJson"));
        return;
      }
      setTreeData(valueToTreeNodes(parsed, "$"));
      setStep("tree");
    } catch {
      showErrorToast(t("supplier.jsonPath.parseError"));
    }
  };

  const handleSelect = (path: string) => {
    onSelect(path);
    setOpen(false);
  };

  return (
    <Popover
      open={open}
      onOpenChange={handleOpenChange}
      align="right"
      contentClassName="w-[380px] p-3"
      trigger={
        <button
          type="button"
          disabled={disabled}
          onClick={() => handleOpenChange(!open)}
          className={
            plain
              ? "inline-flex items-center gap-1 text-xs font-medium text-primary hover:text-primary-hover disabled:opacity-45 disabled:pointer-events-none cursor-pointer"
              : "inline-flex items-center gap-1 rounded-md bg-primary-soft px-2 py-0.5 text-xs font-medium text-primary hover:bg-primary-soft-hover disabled:opacity-45 disabled:pointer-events-none cursor-pointer"
          }
        >
          <Compass className="h-3.5 w-3.5" />
          <span>{t("supplier.jsonPath.trigger")}</span>
        </button>
      }
    >
      <div className="space-y-2.5">
        <div className="text-xs font-semibold text-foreground">
          {t("supplier.jsonPath.title")}
        </div>

        {step === "input" ? (
          <div className="space-y-2.5">
            <p className="text-[11px] text-muted-foreground leading-relaxed">
              {t("supplier.jsonPath.inputHint")}
            </p>
            <Textarea
              value={rawJsonInput}
              onChange={(e) => setRawJsonInput(e.target.value)}
              rows={6}
              placeholder='{ "code": 200, "data": { "token": "secret_token_123" } }'
              className="font-mono-tech"
            />
            <div className="flex justify-end">
              <Button
                type="button"
                variant="primary"
                size="sm"
                disabled={!rawJsonInput.trim()}
                onClick={handleParse}
              >
                {t("supplier.jsonPath.parse")}
              </Button>
            </div>
          </div>
        ) : (
          <div className="space-y-2.5">
            <p className="text-[11px] text-muted-foreground leading-relaxed">
              {t("supplier.jsonPath.treeHint")}
            </p>
            <div className="max-h-56 overflow-y-auto rounded-md border border-border bg-card p-1">
              {treeData.map((node) => (
                <TreeNodeRow
                  key={node.jsonPath}
                  node={node}
                  depth={0}
                  onSelect={handleSelect}
                />
              ))}
            </div>
            <div className="flex justify-end">
              <Button
                type="button"
                variant="outline"
                size="sm"
                onClick={() => setStep("input")}
              >
                {t("supplier.jsonPath.repaste")}
              </Button>
            </div>
          </div>
        )}
      </div>
    </Popover>
  );
}
