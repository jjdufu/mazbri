<template>
  <div class="wb-pipe">
    <div v-if="!hasData" class="wb-pipe__empty">
      {{ $t("upstreamConnector.pipelineEmpty") }}
    </div>
    <div v-else class="wb-pipe__cols">
      <section class="wb-pipe__col">
        <header>
          <ArtSvgIcon icon="ri:send-plane-line" />
          {{ $t("upstreamConnector.pipeTenantReq") }}
        </header>
        <pre>{{ inboundText }}</pre>
      </section>
      <section class="wb-pipe__col">
        <header>
          <ArtSvgIcon icon="ri:flashlight-line" />
          {{ $t("upstreamConnector.pipeUpstreamReq") }}
        </header>
        <pre>{{ transformedText }}</pre>
      </section>
      <section class="wb-pipe__col">
        <header>
          <ArtSvgIcon icon="ri:exchange-line" />
          {{ $t("upstreamConnector.pipeUpstreamRes") }}
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
  if (rec.method) parts.push(String(rec.method));
  if (rec.url) parts.push(String(rec.url));
  if (rec.detail) parts.push(String(rec.detail));
  if (rec.status != null) parts.push(`status ${rec.status}`);
  if (rec.body_preview != null) parts.push(prettyJson(rec.body_preview));
  return parts.join("\n");
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
    body_preview: t.body,
  });
});
</script>

<style scoped lang="scss">
.wb-pipe {
  min-height: 0;
}
.wb-pipe__empty {
  font-size: 12px;
  color: var(--el-text-color-placeholder);
  padding: 12px 0;
}
.wb-pipe__cols {
  display: grid;
  grid-template-columns: 1fr 1fr 1fr;
  gap: 8px;
}
.wb-pipe__col {
  border: 1px solid var(--el-border-color-lighter);
  border-radius: var(--wb-radius, var(--art-form-control-radius));
  overflow: hidden;
  min-width: 0;
  header {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    font-weight: 600;
    padding: 6px 8px;
    background: var(--el-fill-color-light);
  }
  pre {
    margin: 0;
    padding: 8px;
    font-size: 11px;
    max-height: 220px;
    overflow: auto;
    white-space: pre-wrap;
    word-break: break-all;
  }
}
@media (max-width: 1100px) {
  .wb-pipe__cols {
    grid-template-columns: 1fr;
  }
}
</style>
