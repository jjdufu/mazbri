<template>
  <div class="wb-auth-inv">
    <div class="wb-auth-inv__head">
      <span class="wb-auth-inv__title">{{
        $t("upstreamConnector.invalidation")
      }}</span>
      <span v-if="source" class="wb-auth-inv__source">
        {{ $t("upstreamConnector.invalidationSource") }}: {{ sourceLabel }}
      </span>
      <ElSwitch v-model="form.enabled" :disabled="disabled" />
    </div>
    <p class="wb-auth__hint">{{ $t("upstreamConnector.invalidationDesc") }}</p>
    <p v-if="source === 'channel'" class="wb-auth__warn">
      {{ $t("upstreamConnector.invalidationChannelOverride") }}
    </p>

    <template v-if="form.enabled">
      <label class="wb-auth-inv__label">{{
        $t("upstreamConnector.invalidationHttpStatus")
      }}</label>
      <ElSelect
        v-model="form.http_status"
        multiple
        filterable
        allow-create
        default-first-option
        :reserve-keyword="false"
        :disabled="disabled"
        :placeholder="$t('upstreamConnector.invalidationHttpStatusDesc')"
        class="wb-auth-inv__full"
        @change="normalizeNumbers('http_status')"
      >
        <ElOption
          v-for="c in statusPresets"
          :key="c"
          :label="String(c)"
          :value="c"
        />
      </ElSelect>

      <template v-if="fields.jsonrpc">
        <label class="wb-auth-inv__label">
          {{ $t("upstreamConnector.invalidationJsonrpcCodes") }}
        </label>
        <ElSelect
          v-model="form.jsonrpc_codes"
          multiple
          filterable
          allow-create
          default-first-option
          :reserve-keyword="false"
          :disabled="disabled"
          :placeholder="$t('upstreamConnector.invalidationJsonrpcCodesDesc')"
          class="wb-auth-inv__full"
          @change="normalizeNumbers('jsonrpc_codes')"
        />
      </template>

      <label class="wb-auth-inv__label">{{
        $t("upstreamConnector.invalidationBodyMatch")
      }}</label>
      <p class="wb-auth__hint">
        {{ $t("upstreamConnector.invalidationBodyMatchDesc") }}
      </p>
      <div
        v-for="(row, i) in form.body_match"
        :key="i"
        class="wb-auth-inv__row"
      >
        <ElInput v-model="row.path" placeholder="$.code" :disabled="disabled" />
        <ElInput
          v-model="row.equals"
          placeholder='"TOKEN_EXPIRED", 40101'
          :disabled="disabled"
        />
        <ElButton
          link
          type="danger"
          :disabled="disabled"
          @click="form.body_match.splice(i, 1)"
        >
          {{ $t("common.actions.delete") }}
        </ElButton>
      </div>
      <ElButton
        link
        type="primary"
        :disabled="disabled"
        @click="form.body_match.push({ path: '', equals: '' })"
      >
        {{ $t("upstreamConnector.invalidationAddBodyMatch") }}
      </ElButton>

      <div v-if="fields.wsClose" class="wb-auth-inv__switch">
        <ElSwitch
          v-model="form.ws_close_before_reply"
         
          :disabled="disabled"
        />
        <span>{{ $t("upstreamConnector.invalidationWsClose") }}</span>
        <p class="wb-auth__hint">
          {{ $t("upstreamConnector.invalidationWsCloseDesc") }}
        </p>
      </div>

      <p v-if="issueText" class="wb-auth__warn">{{ issueText }}</p>
    </template>
  </div>
</template>

<script setup lang="ts">
import { $t } from "@/locales";
import {
  createEmptyInvalidation,
  invalidationFields,
  type InvalidationForm,
  type InvalidationSource,
  validateInvalidation,
} from "../authInvalidation";

const props = withDefaults(
  defineProps<{
    adapter?: string;
    source?: InvalidationSource | "";
    disabled?: boolean;
  }>(),
  { adapter: "", source: "", disabled: false },
);

const form = defineModel<InvalidationForm>({
  default: () => createEmptyInvalidation(),
});

const statusPresets = [401, 403, 419, 440, 498];

const fields = computed(() => invalidationFields(props.adapter));

const sourceLabel = computed(() => {
  switch (props.source) {
    case "channel":
      return $t("upstreamConnector.invalidationSourceChannel");
    case "supplier":
      return $t("upstreamConnector.invalidationSourceSupplier");
    default:
      return $t("upstreamConnector.invalidationSourceDefault");
  }
});

function normalizeNumbers(key: "http_status" | "jsonrpc_codes") {
  const seen = new Set<number>();
  form.value[key] = form.value[key]
    .map(Number)
    .filter((n) => Number.isInteger(n) && !seen.has(n) && (seen.add(n), true));
}

const issueText = computed(() => {
  const issue = validateInvalidation(form.value);
  if (!issue) return "";
  switch (issue.kind) {
    case "status_range":
      return $t("upstreamConnector.invalidationErrStatusRange", {
        code: issue.value,
      });
    case "status_duplicate":
    case "jsonrpc_duplicate":
      return $t("upstreamConnector.invalidationErrDuplicate", {
        code: issue.value,
      });
    case "body_path":
      return $t("upstreamConnector.invalidationErrBodyPath", {
        index: issue.index + 1,
      });
    case "body_equals":
      return $t("upstreamConnector.invalidationErrBodyEquals", {
        index: issue.index + 1,
      });
    default:
      return "";
  }
});
</script>

<style scoped lang="scss">
.wb-auth-inv {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-top: 12px;
  padding-top: 8px;
  border-top: 1px dashed var(--el-border-color-light);
}
.wb-auth-inv__head {
  display: flex;
  align-items: center;
  gap: 8px;
}
.wb-auth-inv__title {
  font-weight: 600;
}
.wb-auth-inv__source {
  margin-left: auto;
  color: var(--el-text-color-secondary);
  font-size: 12px;
}
.wb-auth-inv__label {
  font-size: 12px;
  color: var(--el-text-color-regular);
}
.wb-auth-inv__full {
  width: 100%;
}
.wb-auth-inv__row {
  display: flex;
  gap: 8px;
  align-items: center;
}
.wb-auth-inv__switch {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
}
</style>
