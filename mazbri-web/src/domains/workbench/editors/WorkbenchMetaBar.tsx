import { useMemo, useState } from "react";
import { ChevronDown } from "lucide-react";
import type { ConnectorEndpoint } from "@/api/upstream-connectors";
import { Badge, Popover, Tooltip, cn } from "@/components/ui";
import { useT } from "@/locales";
import {
  EGRESS_ADAPTERS,
  resolvedBodyEncoding,
  suggestChannelCode,
} from "../engine/workbenchHelpers";

export interface WorkbenchMetaBarProps {
  endpoint: ConnectorEndpoint;
  endpoints: ConnectorEndpoint[];
  activeIndex: number;
  canWrite: boolean;
  onUpdateEndpoint: (
    updater: (ep: ConnectorEndpoint) => ConnectorEndpoint,
  ) => void;
  onSelectAdapter: (adapter: string) => void;
  onEditAdapter: () => void;
}

export function WorkbenchMetaBar({
  endpoint,
  endpoints,
  activeIndex,
  canWrite,
  onUpdateEndpoint,
  onSelectAdapter,
  onEditAdapter,
}: WorkbenchMetaBarProps) {
  const { t } = useT();
  const [adapterMenuOpen, setAdapterMenuOpen] = useState(false);

  const existingTags = useMemo(() => {
    const s = new Set<string>();
    for (const ep of endpoints) {
      const tag = (ep.tag || "").trim();
      if (tag) s.add(tag);
    }
    return [...s];
  }, [endpoints]);

  const adapterCode = endpoint.adapter || "http_json";
  const encoding = resolvedBodyEncoding(endpoint);
  const isJobPoll = adapterCode === "job_poll";
  const isWsUnary = adapterCode === "ws_unary";

  const adapterLabel = (code: string): string => {
    const key = `upstreamConnector.adapterNames.${code}`;
    const label = t(key);
    return label === key ? code : label;
  };

  const displayName =
    adapterCode === "http_json" && encoding === "form"
      ? t("upstreamConnector.adapterEncodingForm")
      : adapterLabel(adapterCode);

  const gateOn = endpoint.gate !== false;

  return (
    <div
      className={cn(
        "flex shrink-0 flex-wrap items-center gap-1.5 border-b border-border-subtle bg-card px-3 py-1.5",
        !canWrite && "pointer-events-none opacity-75",
      )}
    >
      <label className="inline-flex min-w-0 flex-[0_1_130px] items-center gap-1.5">
        <span className="shrink-0 whitespace-nowrap text-[11px] font-medium text-muted-foreground">
          {t("upstreamConnector.tagLabel")}
        </span>
        <input
          type="text"
          list="wb-ep-tag-datalist"
          value={endpoint.tag || ""}
          disabled={!canWrite}
          placeholder={t("upstreamConnector.tagPlaceholder")}
          aria-label={t("upstreamConnector.tagLabel")}
          spellCheck={false}
          onChange={(e) => {
            const val = e.target.value;
            onUpdateEndpoint((ep) => ({ ...ep, tag: val }));
          }}
          className="h-6 min-w-[56px] flex-1 rounded-md border border-transparent bg-transparent px-1.5 text-xs text-foreground placeholder:text-muted-foreground/70 outline-none hover:bg-muted/60 focus:border-primary focus:bg-card"
        />
        <datalist id="wb-ep-tag-datalist">
          {existingTags.map((tag) => (
            <option key={tag} value={tag} />
          ))}
        </datalist>
      </label>

      <span className="h-3.5 w-px bg-border" aria-hidden="true" />

      <label className="inline-flex min-w-0 flex-[1_1_140px] items-center gap-1.5">
        <span className="shrink-0 whitespace-nowrap text-[11px] font-medium text-muted-foreground">
          {t("upstreamConnector.fields.epName")}
        </span>
        <input
          type="text"
          value={endpoint.name || ""}
          disabled={!canWrite}
          placeholder={t("upstreamConnector.fields.epName")}
          aria-label={t("upstreamConnector.fields.epName")}
          spellCheck={false}
          onChange={(e) => {
            const val = e.target.value;
            onUpdateEndpoint((ep) => ({ ...ep, name: val }));
          }}
          className="h-6 min-w-[60px] flex-1 rounded-md border border-transparent bg-transparent px-1.5 text-xs text-foreground placeholder:text-muted-foreground/70 outline-none hover:bg-muted/60 focus:border-primary focus:bg-card"
        />
      </label>

      <span className="h-3.5 w-px bg-border" aria-hidden="true" />

      <label className="inline-flex min-w-0 flex-[1_1_150px] items-center gap-1.5">
        <span className="shrink-0 whitespace-nowrap text-[11px] font-medium text-muted-foreground">
          {t("upstreamConnector.fields.channelCode")}
        </span>
        <input
          type="text"
          value={endpoint.channel_code || ""}
          disabled={!canWrite}
          placeholder={suggestChannelCode(endpoint, activeIndex)}
          aria-label={t("upstreamConnector.fields.channelCode")}
          spellCheck={false}
          onChange={(e) => {
            const val = e.target.value;
            onUpdateEndpoint((ep) => ({ ...ep, channel_code: val }));
          }}
          className="h-6 min-w-[60px] flex-1 rounded-md border border-transparent bg-transparent px-1.5 font-mono-tech text-[11.5px] text-foreground placeholder:text-muted-foreground/70 outline-none hover:bg-muted/60 focus:border-primary focus:bg-card"
        />
      </label>

      <span className="h-3.5 w-px bg-border" aria-hidden="true" />

      <label className="inline-flex min-w-0 flex-[1_1_150px] items-center gap-1.5">
        <Tooltip content={t("upstreamConnector.pathPrefixTooltip")} side="top">
          <span className="shrink-0 cursor-help whitespace-nowrap border-b border-dashed border-border text-[11px] font-medium text-muted-foreground">
            {t("upstreamConnector.gatewayPrefixLabel")}
          </span>
        </Tooltip>
        <input
          type="text"
          value={endpoint.path_prefix || ""}
          disabled={!canWrite}
          placeholder={endpoint.path || "/"}
          aria-label={t("upstreamConnector.fields.pathPrefix")}
          spellCheck={false}
          onChange={(e) => {
            const val = e.target.value;
            onUpdateEndpoint((ep) => ({ ...ep, path_prefix: val }));
          }}
          className="h-6 min-w-[60px] flex-1 rounded-md border border-transparent bg-transparent px-1.5 font-mono-tech text-[11.5px] text-foreground placeholder:text-muted-foreground/70 outline-none hover:bg-muted/60 focus:border-primary focus:bg-card"
        />
      </label>

      <div className="ml-auto flex shrink-0 items-center gap-1.5">
        <Tooltip
          content={
            gateOn
              ? t("upstreamConnector.gateOnHint")
              : t("upstreamConnector.gateOffHint")
          }
          side="top"
        >
          <button
            type="button"
            disabled={!canWrite}
            onClick={() =>
              onUpdateEndpoint((ep) => ({ ...ep, gate: ep.gate === false }))
            }
            className={cn(
              "inline-flex h-7 items-center gap-1.5 rounded-md border border-border bg-muted/50 px-2.5 text-[11.5px] font-medium transition-colors hover:bg-hover-bg cursor-pointer",
              gateOn ? "text-foreground" : "text-muted-foreground",
            )}
          >
            <span
              className={cn(
                "h-1.5 w-1.5 rounded-full border",
                gateOn
                  ? "border-success-fg bg-success-fg"
                  : "border-muted-foreground bg-transparent",
              )}
            />
            <span>{t("upstreamConnector.gate")}</span>
          </button>
        </Tooltip>

        <Popover
          open={adapterMenuOpen}
          onOpenChange={setAdapterMenuOpen}
          align="right"
          contentClassName="min-w-[190px] p-1"
          trigger={
            <button
              type="button"
              aria-expanded={adapterMenuOpen}
              aria-label={`${displayName} (${adapterCode})`}
              title={`${displayName} (${adapterCode})`}
              onClick={() => setAdapterMenuOpen((v) => !v)}
              className={cn(
                "inline-flex h-7 max-w-[260px] items-center gap-1.5 rounded-md border px-2.5 text-[11.5px] font-semibold transition-colors cursor-pointer",
                isJobPoll
                  ? "border-danger-fg/45 bg-danger-fg/10 text-danger-fg hover:bg-danger-fg/15"
                  : isWsUnary
                    ? "border-primary/45 bg-primary-soft text-primary hover:bg-primary-soft-hover"
                    : "border-border bg-muted/60 text-foreground hover:bg-hover-bg",
              )}
            >
              <span className="truncate">{displayName}</span>
              {isJobPoll && (
                <Badge variant="danger" className="px-1.5 py-0 text-[10px]">
                  {t("upstreamConnector.jobPollRiskTag")}
                </Badge>
              )}
              {isWsUnary && (
                <Badge variant="xblue" className="px-1.5 py-0 text-[10px]">
                  {t("upstreamConnector.wsUnaryPoolTag")}
                </Badge>
              )}
              <ChevronDown className="h-3 w-3 shrink-0 opacity-75" />
            </button>
          }
        >
          <div className="flex flex-col gap-0.5">
            {EGRESS_ADAPTERS.map((item) => {
              const current = item === adapterCode;
              return (
                <button
                  key={item}
                  type="button"
                  disabled={!canWrite}
                  onClick={() => {
                    setAdapterMenuOpen(false);
                    if (canWrite) onSelectAdapter(item);
                  }}
                  className={cn(
                    "flex w-full items-center justify-between rounded-md px-2 py-1.5 text-left text-xs transition-colors disabled:opacity-45 cursor-pointer",
                    current
                      ? "bg-primary-soft text-primary font-semibold"
                      : "text-foreground hover:bg-hover-bg",
                  )}
                >
                  <span>{adapterLabel(item)}</span>
                </button>
              );
            })}
            <div className="my-1 h-px bg-border-subtle" />
            <button
              type="button"
              onClick={() => {
                setAdapterMenuOpen(false);
                onEditAdapter();
              }}
              className="flex w-full items-center rounded-md px-2 py-1.5 text-left text-xs text-muted-foreground hover:bg-hover-bg hover:text-foreground cursor-pointer"
            >
              {t("upstreamConnector.adapterChipHint")}
            </button>
          </div>
        </Popover>
      </div>
    </div>
  );
}
