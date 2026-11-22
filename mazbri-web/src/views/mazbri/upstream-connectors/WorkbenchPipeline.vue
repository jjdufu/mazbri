<template>
  <div class="wb-pipe">
    <div v-if="!hasData" class="wb-pipe__empty">
      {{ $t("upstreamConnector.pipelineEmpty") }}
    </div>
    <div v-else class="wb-pipe__cols">
      <section class="wb-pipe__col">
        <header>
          <ArtSvgIcon icon="ri:send-plane-line" />
          <span>{{ $t("upstreamConnector.pipeTenantReq") }}</span>
        </header>
        <pre>{{ inboundText }}</pre>
      </section>
      <section class="wb-pipe__col">
        <header>
          <ArtSvgIcon icon="ri:flashlight-line" />
          <span>{{ $t("upstreamConnector.pipeUpstreamReq") }}</span>
        </header>
        <pre>{{ transformedText }}</pre>
      </section>
      <section class="wb-pipe__col">
        <header>
          <ArtSvgIcon icon="ri:exchange-line" />
          <span>{{ $t("upstreamConnector.pipeUpstreamRes") }}</span>
        </header>
        <pre>{{ upstreamText }}</pre>
      </section>
    </div>
  </div>
</template>

<script setup lang="ts">
import type { ConnectorRun } from "@/api/upstream-connectors";
import { extractPipeline, prettyJson } from "./workbenchHelpers";

const props = defineProps<{
  run: ConnectorRun | null;
  inbound?: { method?: string; path?: string; body?: unknown };
}>();

const pipe = computed(() => extractPipeline(props.run));
const hasData = computed(
  () =>
    Boolean(pipe.value.transformedRequest) ||
    Boolean(pipe.value.upstreamResponse) ||
    Boolean(props.run),
);

function pack(rec?: Record<string, unknown> | null) {
  if (!rec) return "";
  const parts: string[] = [];
  if (rec.method || rec.url) {
    parts.push(`${String(rec.method || "")} ${String(rec.url || "")}`.trim());
  }
  if (rec.detail) parts.push(String(rec.detail));
  if (rec.status != null) parts.push(`HTTP ${rec.status}`);
  const hdrs = rec.headers_masked || rec.headers;
  if (hdrs && typeof hdrs === "object" && Object.keys(hdrs as object).length) {
    parts.push(prettyJson(hdrs));
  }
  if (rec.body_preview != null) parts.push(prettyJson(rec.body_preview));
  return parts.join("\n\n");
}

const inboundText = computed(() => {
  const i = props.inbound;
  if (!i) return "";
  return prettyJson({
    method: i.method,
    path: i.path,
    body: i.body,
  });
});

const transformedText = computed(() =>
  pack(pipe.value.transformedRequest as Record<string, unknown> | undefined),
);

const upstreamText = computed(() => {
  const rec = pipe.value.upstreamResponse as Record<string, unknown> | undefined;
  if (rec) return pack(rec);
  const t = pipe.value.tenantResponse;
  if (!t) return "";
  return pack({
    status: t.status,
    url: t.url,
    headers: t.headers,
    body_preview: t.body,
  });
});
</script>

<style scoped lang="scss">
.wb-pipe {
  min-height: 0;
}

.wb-pipe__empty {
  padding: 12px 0;
  font-size: 12px;
  color: var(--wb-text-faint);
}

.wb-pipe__cols {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 8px;
}

.wb-pipe__col {
  min-width: 0;
  overflow: hidden;
  background: var(--wb-surface);
  border: 1px solid var(--wb-divider);
  border-radius: var(--wb-radius);

  header {
    display: flex;
    gap: 6px;
    align-items: center;
    padding: 6px 10px;
    font-size: 11.5px;
    font-weight: 600;
    color: var(--wb-text);
    background: var(--wb-surface-subtle);
    border-bottom: 1px solid var(--wb-divider);
  }

  pre {
    max-height: 240px;
    padding: 8px 10px;
    margin: 0;
    overflow: auto;
    font-family: var(--wb-font-mono);
    font-size: 11px;
    line-height: 1.5;
    color: var(--wb-text);
    white-space: pre-wrap;
    word-break: break-all;
  }
}

@media (width <= 1100px) {
  .wb-pipe__cols {
    grid-template-columns: 1fr;
  }
}
</style>
