<template>
  <div class="crypto-profile-editor" :class="{ disabled }">
    <div class="cpe-head">
      <CommonSwitchRow>
        <ElSwitch
          :model-value="enabled"
          :disabled="disabled"
          :aria-label="$t('upstreamConnector.cryptoEnable')"
          @update:model-value="onEnabledChange"
        />
        <span class="switch-text ml-2">{{
          $t("upstreamConnector.cryptoEnable")
        }}</span>
        <ElPopover
          placement="bottom-start"
          :width="300"
          trigger="click"
        >
          <div class="cpe-hint-popover">
            {{ $t("upstreamConnector.cryptoAuthBoundaryHint") }}
          </div>
          <template #reference>
            <button
              type="button"
              class="cpe-info-trigger"
              :aria-label="$t('upstreamConnector.cryptoAuthBoundaryHint')"
            >
              <ArtSvgIcon icon="ri:information-line" />
            </button>
          </template>
        </ElPopover>
      </CommonSwitchRow>
    </div>

    <template v-if="enabled && local">
      <div class="cpe-templates">
        <div class="field-label">{{ $t("upstreamConnector.cryptoImportTemplate") }}</div>
        <div class="cpe-template-row">
          <ElButton
            v-for="tpl in CRYPTO_PROFILE_TEMPLATES"
            :key="tpl.id"
            :disabled="disabled"
            @click="importTemplate(tpl.id)"
          >
            {{ $t(tpl.labelKey) }}
          </ElButton>
        </div>
      </div>

      <div class="field-block">
        <div class="field-label">{{ $t("upstreamConnector.cryptoOrder") }}</div>
        <ElSelect
          :model-value="local.order"
          :disabled="disabled"
          :aria-label="$t('upstreamConnector.cryptoOrder')"
          filterable
          @update:model-value="onOrderChange"
        >
          <ElOption
            v-for="ord in CRYPTO_ORDERS"
            :key="ord"
            :label="$t(`upstreamConnector.cryptoOrders.${ord}`)"
            :value="ord"
          />
        </ElSelect>
      </div>

      <div v-if="needsSign" class="field-block">
        <div class="field-label">
          {{ $t("upstreamConnector.cryptoSignAlg") }}
          <span class="required-mark">*</span>
        </div>
        <ElSelect
          :model-value="local.sign?.alg"
          :disabled="disabled"
          :aria-label="$t('upstreamConnector.cryptoSignAlg')"
          filterable
          clearable
          :placeholder="$t('upstreamConnector.cryptoSelectAlg')"
          @update:model-value="onSignAlgChange"
        >
          <ElOption
            v-for="alg in signOptions"
            :key="alg.id"
            :label="algOptionLabel(alg)"
            :value="alg.id"
          />
        </ElSelect>
        <div class="cpe-bound-slot" :class="{ unbound: !boundSignSlot }">
          {{
            boundSignSlot
              ? $t("upstreamConnector.cryptoBoundKeySlot", { slot: boundSignSlot })
              : $t("upstreamConnector.cryptoUnboundKeySlot", {
                  slot: local.sign?.key_slot || "crypto_mac_key",
                })
          }}
        </div>
      </div>

      <div v-if="needsCrypt" class="field-block">
        <div class="field-label">
          {{ $t("upstreamConnector.cryptoCryptAlg") }}
          <span class="required-mark">*</span>
        </div>
        <ElSelect
          :model-value="local.crypt?.alg"
          :disabled="disabled"
          :aria-label="$t('upstreamConnector.cryptoCryptAlg')"
          filterable
          clearable
          :placeholder="$t('upstreamConnector.cryptoSelectAlg')"
          @update:model-value="onCryptAlgChange"
        >
          <ElOption
            v-for="alg in cryptOptions"
            :key="alg.id"
            :label="algOptionLabel(alg)"
            :value="alg.id"
          />
        </ElSelect>
        <div class="cpe-bound-slot" :class="{ unbound: !boundCryptSlot }">
          {{
            boundCryptSlot
              ? $t("upstreamConnector.cryptoBoundKeySlot", { slot: boundCryptSlot })
              : $t("upstreamConnector.cryptoUnboundKeySlot", {
                  slot: local.crypt?.key_slot || "crypto_data_key",
                })
          }}
        </div>
      </div>

      <div class="field-block">
        <div class="field-label">{{ $t("upstreamConnector.cryptoCanon") }}</div>
        <ElSelect
          :model-value="local.canon"
          :disabled="disabled"
          :aria-label="$t('upstreamConnector.cryptoCanon')"
          @update:model-value="patch({ canon: $event })"
        >
          <ElOption
            v-for="c in CANON_STYLES"
            :key="c"
            :label="$t(`upstreamConnector.cryptoCanons.${c}`)"
            :value="c"
          />
        </ElSelect>
      </div>

      <div class="cpe-grid">
        <div class="field-block">
          <div class="field-label">{{ $t("upstreamConnector.cryptoEncPayload") }}</div>
          <ElSelect
            :model-value="wireEncodingToKind(local.encodings.payload)"
            :disabled="disabled"
            :aria-label="$t('upstreamConnector.cryptoEncPayload')"
            @update:model-value="onEncodingChange('payload', $event)"
          >
            <ElOption
              v-for="k in WIRE_ENCODING_KINDS"
              :key="k"
              :label="$t(`upstreamConnector.cryptoWireEncodings.${k}`)"
              :value="k"
            />
          </ElSelect>
        </div>
        <div class="field-block">
          <div class="field-label">{{ $t("upstreamConnector.cryptoEncSignature") }}</div>
          <ElSelect
            :model-value="wireEncodingToKind(local.encodings.signature)"
            :disabled="disabled"
            :aria-label="$t('upstreamConnector.cryptoEncSignature')"
            @update:model-value="onEncodingChange('signature', $event)"
          >
            <ElOption
              v-for="k in WIRE_ENCODING_KINDS"
              :key="k"
              :label="$t(`upstreamConnector.cryptoWireEncodings.${k}`)"
              :value="k"
            />
          </ElSelect>
        </div>
        <div class="field-block">
          <div class="field-label">{{ $t("upstreamConnector.cryptoEncIv") }}</div>
          <ElSelect
            :model-value="wireEncodingToKind(local.encodings.iv)"
            :disabled="disabled"
            :aria-label="$t('upstreamConnector.cryptoEncIv')"
            @update:model-value="onEncodingChange('iv', $event)"
          >
            <ElOption
              v-for="k in WIRE_ENCODING_KINDS"
              :key="k"
              :label="$t(`upstreamConnector.cryptoWireEncodings.${k}`)"
              :value="k"
            />
          </ElSelect>
        </div>
      </div>

      <div class="cpe-grid">
        <div class="field-block">
          <div class="field-label">{{ $t("upstreamConnector.cryptoIvSource") }}</div>
          <ElSelect
            :model-value="local.iv"
            :disabled="disabled"
            :aria-label="$t('upstreamConnector.cryptoIvSource')"
            @update:model-value="patch({ iv: $event })"
          >
            <ElOption
              v-for="s in IV_SOURCES"
              :key="s"
              :label="$t(`upstreamConnector.cryptoIvSources.${s}`)"
              :value="s"
            />
          </ElSelect>
        </div>
        <div class="field-block">
          <div class="field-label">{{ $t("upstreamConnector.cryptoScope") }}</div>
          <ElSelect
            :model-value="local.scope"
            :disabled="disabled"
            :aria-label="$t('upstreamConnector.cryptoScope')"
            @update:model-value="onScopeChange"
          >
            <ElOption
              v-for="s in CIPHER_SCOPES"
              :key="s"
              :label="$t(`upstreamConnector.cryptoScopes.${s}`)"
              :value="s"
            />
          </ElSelect>
        </div>
      </div>

      <div v-if="local.scope === 'named_fields' && needsCrypt" class="field-block">
        <div class="field-label">{{ $t("upstreamConnector.cryptoCryptFields") }}</div>
        <ElInput
          :model-value="(local.crypt?.fields || []).join(', ')"
          :disabled="disabled"
          :aria-label="$t('upstreamConnector.cryptoCryptFields')"
          :placeholder="$t('upstreamConnector.cryptoFieldsPlaceholder')"
          @update:model-value="onCryptFieldsChange"
        />
      </div>

      <div class="field-block">
        <div class="field-label-row">
          <div class="field-label">{{ $t("upstreamConnector.cryptoSlots") }}</div>
          <ElButton size="small" text :disabled="disabled" @click="addSlot">
            {{ $t("upstreamConnector.cryptoAddSlot") }}
          </ElButton>
        </div>
        <div v-if="!local.slots.length" class="cpe-hint">
          {{ $t("upstreamConnector.cryptoSlotsEmpty") }}
        </div>
        <div
          v-for="(slot, idx) in local.slots"
          :key="`slot-${idx}`"
          class="cpe-slot-row"
        >
          <ElInput
            :model-value="slot.role"
            :disabled="disabled"
            :aria-label="$t('upstreamConnector.cryptoSlotRole')"
            :placeholder="$t('upstreamConnector.cryptoSlotRole')"
            @update:model-value="updateSlot(idx, { role: String($event) })"
          />
          <ElSelect
            :model-value="slot.location"
            :disabled="disabled"
            :aria-label="$t('upstreamConnector.cryptoSlots')"
            @update:model-value="updateSlot(idx, { location: $event })"
          >
            <ElOption
              v-for="loc in SLOT_LOCATIONS"
              :key="loc"
              :label="loc"
              :value="loc"
            />
          </ElSelect>
          <ElInput
            :model-value="slot.name"
            :disabled="disabled"
            :aria-label="$t('upstreamConnector.cryptoSlotName')"
            :placeholder="$t('upstreamConnector.cryptoSlotName')"
            @update:model-value="updateSlot(idx, { name: String($event) })"
          />
          <ElButton
            size="small"
            text
            type="danger"
            :disabled="disabled"
            @click="removeSlot(idx)"
          >
            {{ $t("common.actions.delete") }}
          </ElButton>
        </div>
      </div>

      <ElCollapse v-model="advancedOpen" class="cpe-advanced">
        <ElCollapseItem
          name="response"
          :title="$t('upstreamConnector.cryptoResponseCrypt')"
        >
          <div class="field-block">
            <div class="field-label">{{ $t("upstreamConnector.cryptoCryptAlg") }}</div>
            <ElSelect
              :model-value="local.response_crypt?.alg"
              :disabled="disabled"
              :aria-label="$t('upstreamConnector.cryptoResponseCrypt')"
              clearable
              filterable
              :placeholder="$t('upstreamConnector.cryptoSelectAlgOptional')"
              @update:model-value="onResponseCryptAlgChange"
            >
              <ElOption
                v-for="alg in cryptOptions"
                :key="alg.id"
                :label="algOptionLabel(alg)"
                :value="alg.id"
              />
            </ElSelect>
          </div>
          <div
            v-if="local.scope === 'named_fields' && local.response_crypt?.alg"
            class="field-block"
          >
            <div class="field-label">{{ $t("upstreamConnector.cryptoResponseFields") }}</div>
            <ElInput
              :model-value="(local.response_crypt?.fields || []).join(', ')"
              :disabled="disabled"
              :aria-label="$t('upstreamConnector.cryptoResponseFields')"
              :placeholder="$t('upstreamConnector.cryptoFieldsPlaceholder')"
              @update:model-value="onResponseFieldsChange"
            />
          </div>
        </ElCollapseItem>
      </ElCollapse>

      <div class="field-block">
        <CommonSwitchRow>
          <ElSwitch
            :model-value="local.allow_insecure"
            :disabled="disabled || !canToggleInsecure"
            :aria-label="$t('upstreamConnector.cryptoAllowInsecure')"
            @update:model-value="onAllowInsecureChange"
          />
          <span class="switch-text ml-2">{{
            $t("upstreamConnector.cryptoAllowInsecure")
          }}</span>
        </CommonSwitchRow>
        <div class="cpe-hint warning">
          {{
            canToggleInsecure
              ? $t("upstreamConnector.cryptoAllowInsecureHint")
              : $t("upstreamConnector.cryptoAllowInsecureAdminOnly")
          }}
        </div>
      </div>

      <ElAlert
        v-if="issues.length"
        type="warning"
        :closable="false"
        show-icon
        class="cpe-issues"
        :title="$t('upstreamConnector.cryptoValidationTitle')"
      >
        <ul class="cpe-issue-list">
          <li v-for="(issue, i) in issues" :key="`${issue.code}-${i}`">
            {{ issue.message }}
          </li>
        </ul>
      </ElAlert>
    </template>
  </div>
</template>

<script setup lang="ts">
import { $t } from "@/locales";
import CommonSwitchRow from "@/components/common/switch-row/index.vue";
import {
  securityClassLabelKey,
  type CryptoAlgorithmMeta,
} from "@/components/crypto/cryptoCatalog";
import {
  CANON_STYLES,
  CIPHER_SCOPES,
  CRYPTO_ORDERS,
  CRYPTO_PROFILE_TEMPLATES,
  IV_SOURCES,
  SLOT_LOCATIONS,
  WIRE_ENCODING_KINDS,
  applyCryptoTemplate,
  applyOrderDefaults,
  dataOptionsForProfile,
  defaultCryptoProfile,
  kindToWireEncoding,
  macOptionsForProfile,
  parseCryptoProfile,
  serializeCryptoProfile,
  validateCryptoProfileClient,
  wireEncodingToKind,
  type CryptoOrderId,
  type CryptoProfileTemplateId,
  type SlotLocationId,
  type SlotSpecDraft,
  type UpstreamCryptoProfileDraft,
  type WireEncodingKind,
} from "./cryptoProfileHelpers";

const props = withDefaults(
  defineProps<{
    /** 端点上的 crypto 对象；null/undefined = 未启用 */
    modelValue: Record<string, unknown> | null | undefined;
    disabled?: boolean;
    canToggleInsecure?: boolean;
    credentialKeys?: string[];
  }>(),
  {
    disabled: false,
    canToggleInsecure: false,
    credentialKeys: () => [],
  },
);

const emit = defineEmits<{
  "update:modelValue": [Record<string, unknown> | null];
}>();

const advancedOpen = ref<string[]>([]);

const local = ref<UpstreamCryptoProfileDraft | null>(null);

watch(
  () => props.modelValue,
  (val) => {
    local.value = parseCryptoProfile(val);
  },
  { immediate: true, deep: true },
);

const enabled = computed(() => local.value != null);

const needsSign = computed(() => {
  const o = local.value?.order;
  return o === "sign_only" || o === "sign_then_encrypt" || o === "encrypt_then_sign";
});
const needsCrypt = computed(() => {
  const o = local.value?.order;
  return (
    o === "encrypt_only" ||
    o === "aead_only" ||
    o === "sign_then_encrypt" ||
    o === "encrypt_then_sign"
  );
});

const signOptions = computed(() =>
  macOptionsForProfile(Boolean(local.value?.allow_insecure)),
);
const cryptOptions = computed(() =>
  dataOptionsForProfile(
    Boolean(local.value?.allow_insecure),
    local.value?.order === "aead_only",
  ),
);

const issues = computed(() => validateCryptoProfileClient(local.value));

const credKeySet = computed(
  () => new Set((props.credentialKeys || []).map((k) => k.trim()).filter(Boolean)),
);

const boundSignSlot = computed(() => {
  const explicit = local.value?.sign?.key_slot?.trim();
  if (explicit && credKeySet.value.has(explicit)) return explicit;
  for (const candidate of ["crypto_mac_key", "mac_key", "client_secret", "app_secret", "secret"]) {
    if (credKeySet.value.has(candidate)) return candidate;
  }
  return "";
});

const boundCryptSlot = computed(() => {
  const explicit = local.value?.crypt?.key_slot?.trim();
  if (explicit && credKeySet.value.has(explicit)) return explicit;
  for (const candidate of ["crypto_data_key", "data_key"]) {
    if (credKeySet.value.has(candidate)) return candidate;
  }
  return "";
});

function emitSerialized(profile: UpstreamCryptoProfileDraft | null) {
  emit("update:modelValue", serializeCryptoProfile(profile));
}

function patch(partial: Partial<UpstreamCryptoProfileDraft>) {
  if (!local.value) return;
  const next = { ...local.value, ...partial };
  applyOrderDefaults(next);
  local.value = next;
  emitSerialized(next);
}

function onEnabledChange(value: boolean | string | number) {
  if (value) {
    const next = defaultCryptoProfile("aead_only");
    local.value = next;
    emitSerialized(next);
    return;
  }
  local.value = null;
  emit("update:modelValue", null);
}

function onOrderChange(order: CryptoOrderId) {
  if (!local.value) return;
  const next = { ...local.value, order };
  applyOrderDefaults(next);
  local.value = next;
  emitSerialized(next);
}

function onSignAlgChange(alg: string | null | undefined) {
  if (!local.value) return;
  const next = {
    ...local.value,
    sign: alg ? { ...(local.value.sign || { alg: "" }), alg } : null,
  };
  local.value = next;
  emitSerialized(next);
}

function onCryptAlgChange(alg: string | null | undefined) {
  if (!local.value) return;
  const next = {
    ...local.value,
    crypt: alg
      ? {
          ...(local.value.crypt || { alg: "", fields: [] }),
          alg,
          fields: local.value.crypt?.fields || [],
        }
      : null,
  };
  local.value = next;
  emitSerialized(next);
}

function onResponseCryptAlgChange(alg: string | null | undefined) {
  if (!local.value) return;
  const next = {
    ...local.value,
    response_crypt: alg
      ? {
          ...(local.value.response_crypt || { alg: "", fields: [] }),
          alg,
          fields: local.value.response_crypt?.fields || [],
        }
      : null,
  };
  if (alg && !advancedOpen.value.includes("response")) {
    advancedOpen.value = ["response"];
  }
  local.value = next;
  emitSerialized(next);
}

function onEncodingChange(
  field: "payload" | "signature" | "iv",
  kind: WireEncodingKind,
) {
  if (!local.value) return;
  patch({
    encodings: {
      ...local.value.encodings,
      [field]: kindToWireEncoding(kind),
    },
  });
}

function onScopeChange(scope: UpstreamCryptoProfileDraft["scope"]) {
  patch({ scope });
}

function parseFieldsCsv(raw: string): string[] {
  return String(raw || "")
    .split(/[,;\s]+/)
    .map((s) => s.trim())
    .filter(Boolean);
}

function onCryptFieldsChange(raw: string) {
  if (!local.value?.crypt) return;
  const next = {
    ...local.value,
    crypt: { ...local.value.crypt, fields: parseFieldsCsv(raw) },
  };
  local.value = next;
  emitSerialized(next);
}

function onResponseFieldsChange(raw: string) {
  if (!local.value?.response_crypt) return;
  const next = {
    ...local.value,
    response_crypt: {
      ...local.value.response_crypt,
      fields: parseFieldsCsv(raw),
    },
  };
  local.value = next;
  emitSerialized(next);
}

function onAllowInsecureChange(value: boolean | string | number) {
  if (!local.value) return;
  const allow = Boolean(value);
  const next = { ...local.value, allow_insecure: allow };
  if (!allow) {
    const signMeta = next.sign?.alg
      ? macOptionsForProfile(true).find((a) => a.id === next.sign?.alg)
      : undefined;
    const cryptMeta = next.crypt?.alg
      ? dataOptionsForProfile(true, false).find((a) => a.id === next.crypt?.alg)
      : undefined;
    if (signMeta?.legacy) next.sign = { alg: "hmac-sha256" };
    if (cryptMeta?.legacy) {
      next.crypt = {
        alg: next.order === "aead_only" ? "aes-256-gcm" : "aes-256-cbc",
        fields: next.crypt?.fields || [],
      };
    }
  }
  local.value = next;
  emitSerialized(next);
}

function addSlot() {
  if (!local.value) return;
  const slot: SlotSpecDraft = { role: "sign", location: "form", name: "sign" };
  patch({ slots: [...local.value.slots, slot] });
}

function updateSlot(
  idx: number,
  partial: Partial<{ role: string; location: SlotLocationId; name: string }>,
) {
  if (!local.value) return;
  const slots = local.value.slots.map((s, i) =>
    i === idx ? { ...s, ...partial } : s,
  );
  patch({ slots });
}

function removeSlot(idx: number) {
  if (!local.value) return;
  patch({ slots: local.value.slots.filter((_, i) => i !== idx) });
}

function importTemplate(id: CryptoProfileTemplateId) {
  const next = applyCryptoTemplate(id);
  if (!props.canToggleInsecure) next.allow_insecure = false;
  local.value = next;
  emitSerialized(next);
}

function algOptionLabel(alg: CryptoAlgorithmMeta): string {
  return `${alg.label} · ${$t(securityClassLabelKey(alg.security))}`;
}
</script>

<style scoped lang="scss">
.crypto-profile-editor {
  display: flex;
  flex-direction: column;
  gap: 12px;
  width: 100%;

  &.disabled {
    opacity: 0.75;
  }
}

.cpe-head {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.field-block {
  display: flex;
  flex-direction: column;
  gap: 6px;
  width: 100%;
}

.field-label {
  font-size: 13px;
  font-weight: 600;
  color: var(--el-text-color-regular);
}

.field-label-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.cpe-hint {
  font-size: 12px;
  color: var(--el-text-color-secondary);
  line-height: 1.4;

  &.warning {
    color: var(--el-color-warning);
  }
}

.cpe-info-trigger {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  padding: 0;
  margin-left: 4px;
  border: none;
  border-radius: 50%;
  background: transparent;
  color: var(--el-text-color-secondary);
  font-size: 13px;
  cursor: pointer;
  transition: color 0.15s ease, background-color 0.15s ease;

  &:hover {
    color: var(--el-color-primary);
    background: var(--el-fill-color-light);
  }
}

.cpe-hint-popover {
  font-size: 12px;
  color: var(--el-text-color-regular);
  line-height: 1.5;
}

.required-mark {
  color: var(--el-color-danger);
  margin-left: 2px;
}

.cpe-grid {
  display: grid;
  grid-template-columns: 1fr;
  gap: 12px;

  @media (min-width: 720px) {
    grid-template-columns: 1fr 1fr;
  }
}

.cpe-template-row {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.cpe-slot-row {
  display: grid;
  grid-template-columns: 1fr 110px 1fr auto;
  gap: 8px;
  margin-bottom: 8px;
}

.cpe-advanced {
  border: none;

  :deep(.el-collapse-item__header) {
    font-size: 13px;
    font-weight: 600;
    height: 36px;
    line-height: 36px;
  }

  :deep(.el-collapse-item__wrap),
  :deep(.el-collapse-item__header) {
    border: none;
    background: transparent;
  }
}

.cpe-issues {
  margin-top: 4px;
}

.cpe-issue-list {
  margin: 0;
  padding-left: 18px;
  font-size: 12px;
}

.switch-text {
  font-size: 13px;
}

.cpe-bound-slot {
  font-size: 12px;
  color: var(--el-color-success);
  &.unbound {
    color: var(--el-text-color-secondary);
  }
}
</style>
