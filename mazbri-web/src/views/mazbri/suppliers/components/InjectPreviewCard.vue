<template>
  <div class="inject-preview-card" :class="{ 'is-compact': compact }">
    <div class="preview-header">
      <ArtSvgIcon icon="ri:scan-2-line" class="header-icon mr-1.5" />
      <span>{{ $t("supplier.config.injectPreviewTitle") }}</span>
    </div>

    <div class="preview-body">
      <div class="preview-meta">
        <span class="meta-label">{{ $t("supplier.config.injectLocation") }}:</span>
        <ElTag size="small" type="success" effect="dark">
          {{ injectTo.toUpperCase() }}
        </ElTag>
      </div>

      <div class="preview-code-block">
        <template v-if="injectTo === 'header'">
          <div class="code-line header-style">
            <span class="http-verb">POST</span>
            <span class="http-path">/api/v1/resource</span>
            <span class="http-version">HTTP/1.1</span>
          </div>
          <div class="code-line header-key-val highlight">
            <span class="key">{{ injectKey || "Authorization" }}:</span>
            <span class="val">{{ formattedHeaderToken }}</span>
          </div>
          <div class="code-line header-key-val">
            <span class="key">Accept:</span>
            <span class="val">application/json</span>
          </div>
          <div class="code-line header-key-val">
            <span class="key">Content-Type:</span>
            <span class="val">application/json</span>
          </div>
        </template>

        <template v-else-if="injectTo === 'query'">
          <div class="code-line header-style highlight">
            <span class="http-verb">GET</span>
            <span class="http-path">/api/v1/resource?</span>
            <span class="key">{{ injectKey || "token" }}</span>
            <span class="operator">=</span>
            <span class="val">{{ dummyToken }}</span>
          </div>
          <div class="code-line header-key-val">
            <span class="key">Accept:</span>
            <span class="val">application/json</span>
          </div>
        </template>

        <template v-else-if="injectTo === 'body'">
          <div class="code-line header-style">
            <span class="http-verb">POST</span>
            <span class="http-path">/api/v1/resource</span>
            <span class="http-version">HTTP/1.1</span>
          </div>
          <div class="code-line header-key-val">
            <span class="key">Content-Type:</span>
            <span class="val">application/json</span>
          </div>
          <div class="code-spacer" />
          <div class="body-json highlight">
            <div class="json-line">{</div>
            <div class="json-line indent">
              <span class="json-key">"{{ injectKey || "access_token" }}"</span>:
              <span class="json-val">"{{ dummyToken }}"</span>
            </div>
            <div class="json-line">}</div>
          </div>
        </template>
      </div>

      <div v-if="!compact" class="preview-tip-bar">
        <ArtSvgIcon icon="ri:information-line" class="tip-icon" />
        <span>{{ $t("supplier.config.injectPreviewTip") }}</span>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";
import { $t } from "@/locales";

interface Props {
  injectTo: string;
  injectKey: string;
  injectValueFormat: string;

  compact?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  injectTo: "header",
  injectKey: "",
  injectValueFormat: "",
  compact: false,
});

const dummyToken = "<DYNAMIC_TOKEN>";

const formattedHeaderToken = computed(() => {
  const format = props.injectValueFormat || "Bearer {}";
  if (format.includes("{}")) {
    return format.replace("{}", dummyToken);
  }
  return `${format} ${dummyToken}`.trim();
});
</script>

<style scoped lang="scss">
  .inject-preview-card {
    border: 1px solid var(--el-border-color-light);
    border-radius: var(--el-border-radius-base);
    background-color: var(--el-fill-color-blank);
    overflow: hidden;
    box-shadow: none;

    &.is-compact {
      .preview-header {
        padding: 4px 10px;
        font-size: 12px;
      }

      .preview-body {
        padding: 8px 10px;
      }

      .preview-code-block {
        padding: 8px 10px;
        font-size: 11px;
        line-height: 1.4;
      }

      .preview-meta {
        margin-bottom: 6px;
      }
    }

    .preview-header {
      display: flex;
      align-items: center;
      padding: 6px 12px;
      background-color: var(--el-fill-color-light);
      border-bottom: 1px solid var(--el-border-color-light);
      font-size: 12px;
      font-weight: 600;
      color: var(--el-text-color-primary);

      .header-icon {
        color: var(--el-color-primary);
      }
    }

    .preview-body {
      padding: 12px;
    }

    .preview-meta {
      display: flex;
      align-items: center;
      margin-bottom: 10px;
      font-size: 12px;

      .meta-label {
        color: var(--el-text-color-secondary);
        margin-right: 6px;
      }
    }

    .preview-code-block {

      --preview-bg: var(--el-fill-color-light);
      --preview-text: var(--el-text-color-primary);
      --preview-verb: #005cc5;
      --preview-path: #032f62;
      --preview-version: #6a737d;
      --preview-key: #d73a49;
      --preview-val: #22863a;
      --preview-operator: var(--el-text-color-regular);
      --preview-highlight-bg: rgba(64, 158, 255, 0.08);

      background-color: var(--preview-bg);
      color: var(--preview-text);
      padding: 10px 14px;
      border-radius: var(--el-border-radius-base);
      font-family: "Fira Code", "SF Mono", Monaco, Menlo, Consolas, monospace;
      font-size: 11px;
      line-height: 1.5;
      border-left: 3px solid var(--el-color-primary);
      transition: background-color 0.3s, color 0.3s;

    .code-spacer {
      height: 8px;
    }

    .code-line {
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;

      &.highlight {
        background-color: var(--preview-highlight-bg);
        border-radius: 2px;
        padding: 0 4px;
        margin: 0 -4px;
      }
    }

    .http-verb {
      color: var(--preview-verb);
      font-weight: bold;
      margin-right: 6px;
    }

    .http-path {
      color: var(--preview-path);
      margin-right: 6px;
    }

    .http-version {
      color: var(--preview-version);
    }

    .key {
      color: var(--preview-key);
      margin-right: 6px;
    }

    .val {
      color: var(--preview-val);
      word-break: break-all;
    }

    .operator {
      color: var(--preview-operator);
      margin: 0 4px;
    }

    .body-json {
      &.highlight {
        background-color: var(--preview-highlight-bg);
        border-radius: 2px;
        padding: 4px;
      }

      .indent {
        padding-left: 14px;
      }

      .json-key {
        color: var(--preview-key);
      }

      .json-val {
        color: var(--preview-val);
      }
    }
  }

  .preview-tip-bar {
    display: flex;
    align-items: flex-start;
    margin-top: 10px;
    padding: 8px 12px;
    background-color: var(--el-color-info-light-9);
    border-radius: var(--el-border-radius-base);
    font-size: 11px;
    color: var(--el-text-color-secondary);
    line-height: 1.4;

    .tip-icon {
      color: var(--el-color-info);
      margin-right: 6px;
      margin-top: 1px;
      flex-shrink: 0;
    }
  }
}

:deep(html.dark) {
  .preview-code-block {
    --preview-bg: #1e1e1e;
    --preview-text: #d4d4d4;
    --preview-verb: #569cd6;
    --preview-path: #dcdcaa;
    --preview-version: #808080;
    --preview-key: #9cdcfe;
    --preview-val: #ce9178;
    --preview-operator: #d4d4d4;
    --preview-highlight-bg: rgba(64, 158, 255, 0.15);
  }
}
</style>
