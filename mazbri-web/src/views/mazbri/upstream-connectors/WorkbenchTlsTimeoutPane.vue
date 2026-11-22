<template>
  <div class="wb-tls" :inert="!canWrite || undefined">
    <div class="wb-sheet">
      <div class="wb-sheet-row is-field">
        <span class="wb-sheet-row__label">{{
          $t("upstreamConnector.timeoutEndpoint")
        }}</span>
        <div class="wb-sheet-row__control">
          <input
            type="number"
            min="100"
            max="600000"
            class="wb-tls__input is-mono tabular-nums"
            :value="endpoint.timeout_ms ?? ''"
            placeholder="15000"
            :aria-label="$t('upstreamConnector.timeoutEndpoint')"
            @input="onEndpointTimeout($event)"
          />
        </div>
      </div>

      <div class="wb-sheet-row is-field">
        <span class="wb-sheet-row__label">{{
          $t("upstreamConnector.timeoutSupplier")
        }}</span>
        <div class="wb-sheet-row__control">
          <input
            type="number"
            min="100"
            max="600000"
            class="wb-tls__input is-mono tabular-nums"
            :value="supplierTimeoutMs"
            placeholder="15000"
            :aria-label="$t('upstreamConnector.timeoutSupplier')"
            @input="onSupplierTimeout($event)"
          />
        </div>
      </div>

      <div class="wb-sheet-row is-field">
        <span class="wb-sheet-row__label">{{
          $t("upstreamConnector.endpointBaseUrl")
        }}</span>
        <div class="wb-sheet-row__control">
          <input
            v-model="endpoint.base_url"
            type="text"
            class="wb-tls__input is-mono"
            placeholder="https://api.example.com"
            :aria-label="$t('upstreamConnector.endpointBaseUrl')"
            spellcheck="false"
          />
        </div>
      </div>

      <div class="wb-sheet-row is-field">
        <span class="wb-sheet-row__label">{{
          $t("upstreamConnector.requestIdHeader")
        }}</span>
        <div class="wb-sheet-row__control">
          <input
            v-model="endpoint.request_id_header"
            type="text"
            class="wb-tls__input is-mono"
            placeholder="X-Upstream-Req-Id"
            :aria-label="$t('upstreamConnector.requestIdHeader')"
            spellcheck="false"
          />
        </div>
      </div>
    </div>

    <div class="wb-tls__grid">
      <section class="wb-tls__card">
        <header class="wb-tls__card-head">
          <span class="wb-tls__card-title">{{
            $t("upstreamConnector.tlsSupplierSection")
          }}</span>
          <label class="wb-tls__switch">
            <input v-model="supplierTls.enabled" type="checkbox" />
            <span>{{ $t("upstreamConnector.tlsEnable") }}</span>
          </label>
        </header>

        <div v-if="supplierTls.enabled" class="wb-tls__body">
          <label
            class="wb-tls__check"
            :title="
              isSuperAdmin ? '' : $t('upstreamConnector.tlsInsecureAdminOnly')
            "
          >
            <input
              v-model="supplierTls.insecure_skip_verify"
              type="checkbox"
              :disabled="!isSuperAdmin"
            />
            <span>{{ $t("upstreamConnector.tlsInsecureSkip") }}</span>
          </label>

          <label class="wb-tls__pem-field">
            <span class="wb-tls__pem-label">{{
              $t("upstreamConnector.tlsCaCert")
            }}</span>
            <textarea
              v-model="supplierTls.ca_cert"
              rows="3"
              class="wb-tls__pem"
              placeholder="-----BEGIN CERTIFICATE-----"
              spellcheck="false"
            />
          </label>

          <label class="wb-tls__pem-field">
            <span class="wb-tls__pem-label">{{
              $t("upstreamConnector.tlsClientCert")
            }}</span>
            <textarea
              v-model="supplierTls.client_cert"
              rows="3"
              class="wb-tls__pem"
              placeholder="-----BEGIN CERTIFICATE-----"
              spellcheck="false"
            />
          </label>

          <label class="wb-tls__pem-field">
            <span class="wb-tls__pem-label">
              {{ $t("upstreamConnector.tlsClientKey") }}
              <span
                v-if="supplierTls.has_saved_key && !supplierTls.client_key.trim()"
                class="wb-tls__saved"
              >
                {{ $t("upstreamConnector.tlsKeySavedTag") }}
              </span>
            </span>
            <textarea
              v-model="supplierTls.client_key"
              rows="3"
              class="wb-tls__pem"
              :placeholder="$t('upstreamConnector.tlsKeyReplacePh')"
              spellcheck="false"
            />
          </label>
        </div>
      </section>

      <section class="wb-tls__card">
        <header class="wb-tls__card-head">
          <span class="wb-tls__card-title">{{
            $t("upstreamConnector.tlsEndpointSection")
          }}</span>
          <label class="wb-tls__switch">
            <input v-model="endpointTls.enabled" type="checkbox" />
            <span>{{ $t("upstreamConnector.tlsEnable") }}</span>
          </label>
        </header>

        <div v-if="endpointTls.enabled" class="wb-tls__body">
          <label
            class="wb-tls__check"
            :title="
              isSuperAdmin ? '' : $t('upstreamConnector.tlsInsecureAdminOnly')
            "
          >
            <input
              v-model="endpointTls.insecure_skip_verify"
              type="checkbox"
              :disabled="!isSuperAdmin"
            />
            <span>{{ $t("upstreamConnector.tlsInsecureSkip") }}</span>
          </label>

          <label class="wb-tls__pem-field">
            <span class="wb-tls__pem-label">{{
              $t("upstreamConnector.tlsCaCert")
            }}</span>
            <textarea
              v-model="endpointTls.ca_cert"
              rows="3"
              class="wb-tls__pem"
              placeholder="-----BEGIN CERTIFICATE-----"
              spellcheck="false"
            />
          </label>

          <label class="wb-tls__pem-field">
            <span class="wb-tls__pem-label">{{
              $t("upstreamConnector.tlsClientCert")
            }}</span>
            <textarea
              v-model="endpointTls.client_cert"
              rows="3"
              class="wb-tls__pem"
              placeholder="-----BEGIN CERTIFICATE-----"
              spellcheck="false"
            />
          </label>

          <label class="wb-tls__pem-field">
            <span class="wb-tls__pem-label">
              {{ $t("upstreamConnector.tlsClientKey") }}
              <span
                v-if="endpointTls.has_saved_key && !endpointTls.client_key.trim()"
                class="wb-tls__saved"
              >
                {{ $t("upstreamConnector.tlsKeySavedTag") }}
              </span>
            </span>
            <textarea
              v-model="endpointTls.client_key"
              rows="3"
              class="wb-tls__pem"
              :placeholder="$t('upstreamConnector.tlsKeyReplacePh')"
              spellcheck="false"
            />
          </label>
        </div>
      </section>
    </div>
  </div>
</template>

<script setup lang="ts">
import type { ConnectorEndpoint } from "@/api/upstream-connectors";
import type { TlsFormState } from "./workbenchHelpers";

const supplierTimeoutMs = defineModel<number>("supplierTimeoutMs", {
  required: true,
});
const supplierTls = defineModel<TlsFormState>("supplierTls", {
  required: true,
});
const endpointTls = defineModel<TlsFormState>("endpointTls", {
  required: true,
});

const props = defineProps<{
  endpoint: ConnectorEndpoint;
  canWrite: boolean;
  isSuperAdmin: boolean;
}>();

function onEndpointTimeout(ev: Event) {
  const raw = (ev.target as HTMLInputElement).value.trim();
  if (!raw) {
    props.endpoint.timeout_ms = undefined;
    return;
  }
  const n = Number(raw);
  if (Number.isFinite(n) && n > 0) {
    props.endpoint.timeout_ms = Math.round(n);
  }
}

function onSupplierTimeout(ev: Event) {
  const n = Number((ev.target as HTMLInputElement).value);
  if (Number.isFinite(n) && n > 0) {
    supplierTimeoutMs.value = Math.round(n);
  }
}
</script>

<style scoped lang="scss">
.wb-tls {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.wb-tls__input {
  width: 100%;
  height: 28px;
  padding: 0 8px;
  font-size: 12.5px;
  color: var(--wb-text);
  background: transparent;
  border: 1px solid transparent;
  border-radius: var(--wb-radius);
  outline: none;

  &.is-mono {
    font-family: var(--wb-font-mono);
    font-size: 12px;
  }

  &:hover {
    background: var(--wb-surface-input);
  }

  &:focus {
    background: var(--wb-surface);
    border-color: var(--wb-divider);
  }
}

.wb-tls__grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(290px, 1fr));
  gap: 12px;
}

.wb-tls__card {
  display: flex;
  flex-direction: column;
  overflow: hidden;
  background: var(--wb-surface);
  border: 1px solid var(--wb-divider);
  border-radius: var(--wb-radius);
}

.wb-tls__card-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  min-height: 36px;
  padding: 0 12px;
  background: var(--wb-surface-subtle);
  border-bottom: 1px solid var(--wb-divider);
}

.wb-tls__card-title {
  font-size: 12.5px;
  font-weight: 600;
  color: var(--wb-text);
}

.wb-tls__switch,
.wb-tls__check {
  display: inline-flex;
  gap: 6px;
  align-items: center;
  font-size: 12px;
  color: var(--wb-text-muted);
  cursor: pointer;
  user-select: none;

  input {
    margin: 0;
  }
}

.wb-tls__body {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 12px;
}

.wb-tls__pem-field {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.wb-tls__pem-label {
  display: inline-flex;
  gap: 8px;
  align-items: center;
  font-size: 11.5px;
  font-weight: 500;
  color: var(--wb-text-muted);
}

.wb-tls__saved {
  padding: 0 6px;
  font-size: 10.5px;
  font-weight: 600;
  color: var(--wb-success);
  background: color-mix(in srgb, var(--wb-success) 12%, transparent);
  border-radius: 999px;
}

.wb-tls__pem {
  width: 100%;
  padding: 6px 8px;
  font-family: var(--wb-font-mono);
  font-size: 11.5px;
  line-height: 1.45;
  color: var(--wb-text);
  resize: vertical;
  background: var(--wb-surface-subtle);
  border: 1px solid var(--wb-divider);
  border-radius: var(--wb-radius);
  outline: none;

  &:focus {
    background: var(--wb-surface);
    border-color: var(--wb-text-muted);
  }

  &::placeholder {
    color: var(--wb-text-faint);
  }
}
</style>
