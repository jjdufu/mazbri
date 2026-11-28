import { ScanLine } from "lucide-react";
import { Badge } from "@/components/ui";
import { useT } from "@/locales";

export interface InjectPreviewCardProps {
  injectTo?: string;
  injectKey?: string;
  injectValueFormat?: string;
  compact?: boolean;
}

const DUMMY_TOKEN = "<DYNAMIC_TOKEN>";

export function InjectPreviewCard({
  injectTo = "header",
  injectKey = "",
  injectValueFormat = "",
  compact = false,
}: InjectPreviewCardProps) {
  const { t } = useT();

  const format = injectValueFormat || "Bearer {}";
  const formattedHeaderToken = format.includes("{}")
    ? format.replace("{}", DUMMY_TOKEN)
    : `${format} ${DUMMY_TOKEN}`.trim();

  return (
    <div className="rounded-lg border border-border bg-card overflow-hidden">
      <div
        className={`flex items-center gap-1.5 border-b border-border bg-card font-semibold text-foreground ${
          compact ? "px-2.5 py-1 text-xs" : "px-3 py-1.5 text-xs"
        }`}
      >
        <ScanLine className="h-3.5 w-3.5 text-primary shrink-0" />
        <span>{t("supplier.config.injectPreviewTitle")}</span>
      </div>

      <div className={compact ? "p-2.5 space-y-1.5" : "p-3 space-y-2.5"}>
        <div className="flex items-center gap-1.5 text-xs">
          <span className="text-muted-foreground">
            {t("supplier.config.injectLocation")}:
          </span>
          <Badge variant="success">{injectTo.toUpperCase()}</Badge>
        </div>

        <div className="rounded-md border-l-3 border-primary bg-muted/55 px-3 py-2 font-mono-tech text-[11px] leading-relaxed text-foreground space-y-0.5 overflow-x-auto">
          {injectTo === "header" && (
            <>
              <div className="whitespace-nowrap">
                <span className="font-semibold text-primary mr-1.5">POST</span>
                <span className="mr-1.5">/api/v1/resource</span>
                <span className="text-muted-foreground">HTTP/1.1</span>
              </div>
              <div className="rounded bg-primary-soft/60 px-1 -mx-1 whitespace-nowrap">
                <span className="text-danger-fg mr-1.5">
                  {injectKey || "Authorization"}:
                </span>
                <span className="text-success-fg break-all">
                  {formattedHeaderToken}
                </span>
              </div>
              <div className="whitespace-nowrap">
                <span className="text-muted-foreground mr-1.5">Accept:</span>
                <span>application/json</span>
              </div>
              <div className="whitespace-nowrap">
                <span className="text-muted-foreground mr-1.5">
                  Content-Type:
                </span>
                <span>application/json</span>
              </div>
            </>
          )}

          {injectTo === "query" && (
            <>
              <div className="rounded bg-primary-soft/60 px-1 -mx-1 whitespace-nowrap">
                <span className="font-semibold text-primary mr-1.5">GET</span>
                <span>/api/v1/resource?</span>
                <span className="text-danger-fg">{injectKey || "token"}</span>
                <span className="mx-0.5">=</span>
                <span className="text-success-fg">{DUMMY_TOKEN}</span>
              </div>
              <div className="whitespace-nowrap">
                <span className="text-muted-foreground mr-1.5">Accept:</span>
                <span>application/json</span>
              </div>
            </>
          )}

          {injectTo === "body" && (
            <>
              <div className="whitespace-nowrap">
                <span className="font-semibold text-primary mr-1.5">POST</span>
                <span className="mr-1.5">/api/v1/resource</span>
                <span className="text-muted-foreground">HTTP/1.1</span>
              </div>
              <div className="whitespace-nowrap">
                <span className="text-muted-foreground mr-1.5">
                  Content-Type:
                </span>
                <span>application/json</span>
              </div>
              <div className="h-1.5" />
              <div className="rounded bg-primary-soft/60 p-1">
                <div>{"{"}</div>
                <div className="pl-3">
                  <span className="text-danger-fg">
                    &quot;{injectKey || "access_token"}&quot;
                  </span>
                  : <span className="text-success-fg">&quot;{DUMMY_TOKEN}&quot;</span>
                </div>
                <div>{"}"}</div>
              </div>
            </>
          )}
        </div>


      </div>
    </div>
  );
}
