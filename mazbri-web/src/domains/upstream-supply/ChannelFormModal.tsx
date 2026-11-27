import { useCallback, useEffect, useMemo, useState } from "react";
import {
  createChannel,
  updateChannel,
  type Channel,
} from "@/api/route-management";
import { fetchSuppliers, type Supplier } from "@/api/supplier";
import {
  FormField,
  FormModal,
  Input,
  NumberInput,
  Select,
  type SelectOptionItem,
} from "@/components/ui";
import { useT } from "@/locales";
import { getUserFacingErrorMessage } from "@/utils/http/error";
import { isMfaStepUpCancelled } from "@/utils/http/mfa-cancelled";
import { showErrorToast, showSuccessToast } from "@/utils/ui/message-guard";
import {
  formatCostPrice,
  isValidCostPrice,
  normalizeCostPriceInput,
} from "./costPrice";

export interface ChannelFormModalProps {
  open: boolean;
  channel: Channel | null;
  onClose: () => void;
  onSuccess: (saved: Channel) => void | Promise<void>;
}

const CHANNEL_CODE_RE = /^[A-Z][A-Z0-9-]*$/;

export function ChannelFormModal({
  open,
  channel,
  onClose,
  onSuccess,
}: ChannelFormModalProps) {
  const { t } = useT();
  const isEdit = Boolean(channel?.id);

  const [submitting, setSubmitting] = useState(false);
  const [supplierOptions, setSupplierOptions] = useState<Supplier[]>([]);
  const [supplierId, setSupplierId] = useState("");
  const [channelCode, setChannelCode] = useState("");
  const [name, setName] = useState("");
  const [pathPrefix, setPathPrefix] = useState("/");
  const [costPrice, setCostPrice] = useState("0.00");
  const [qpsLimit, setQpsLimit] = useState(0);
  const [status, setStatus] = useState("active");
  const [errors, setErrors] = useState<Record<string, string>>({});

  const prependCurrentSupplierIfMissing = useCallback(
    (list: Supplier[]): Supplier[] => {
      if (!channel?.supplier_id) return list;
      if (list.some((item) => item.id === channel.supplier_id)) return list;
      return [
        {
          id: channel.supplier_id,
          supplier_code: channel.supplier_code,
          name: channel.supplier_name,
          upstream_url: "",
          status: "active",
          created_at: "",
          updated_at: "",
        },
        ...list,
      ];
    },
    [channel],
  );

  const loadSuppliers = useCallback(async () => {
    try {
      const res = await fetchSuppliers({
        status: "active",
        page: 1,
        page_size: 100,
      });
      setSupplierOptions(prependCurrentSupplierIfMissing(res.suppliers ?? []));
    } catch {
      setSupplierOptions(prependCurrentSupplierIfMissing([]));
    }
  }, [prependCurrentSupplierIfMissing]);

  useEffect(() => {
    if (!open) return;
    if (channel) {
      setSupplierId(channel.supplier_id);
      setChannelCode(channel.channel_code);
      setName(channel.name);
      setPathPrefix(channel.path_prefix);
      setCostPrice(formatCostPrice(channel.cost_price));
      setQpsLimit(channel.qps_limit ?? 0);
      setStatus(channel.status || "active");
    } else {
      setSupplierId("");
      setChannelCode("");
      setName("");
      setPathPrefix("/");
      setCostPrice("0.00");
      setQpsLimit(0);
      setStatus("active");
    }
    setErrors({});
    void loadSuppliers();
  }, [open, channel, loadSuppliers]);

  const supplierSelectOptions = useMemo<SelectOptionItem[]>(
    () =>
      supplierOptions.map((s) => ({
        value: s.id,
        label: `${s.name} (${s.supplier_code})`,
        text: `${s.name} ${s.supplier_code}`,
      })),
    [supplierOptions],
  );

  const validate = (normalizedPrice: string): boolean => {
    const next: Record<string, string> = {};
    if (!supplierId) {
      next.supplier_id = t("channels.validation.supplierRequired");
    }
    const code = channelCode.trim();
    if (!code) {
      next.channel_code = t("channels.validation.channelCodeRequired");
    } else if (!CHANNEL_CODE_RE.test(code)) {
      next.channel_code = t("channels.validation.channelCodePattern");
    }
    if (!name.trim()) {
      next.name = t("channels.validation.nameRequired");
    }
    if (!pathPrefix.trim()) {
      next.path_prefix = t("channels.validation.pathPrefixRequired");
    }
    if (!normalizedPrice) {
      next.cost_price = t("channels.validation.costPriceRequired");
    } else if (!isValidCostPrice(normalizedPrice)) {
      next.cost_price = t("channels.validation.costPricePattern");
    }
    setErrors(next);
    return Object.keys(next).length === 0;
  };

  const handleSubmit = async () => {
    if (submitting) return;
    const normalizedPrice = normalizeCostPriceInput(costPrice);
    setCostPrice(normalizedPrice);
    if (!validate(normalizedPrice)) return;

    setSubmitting(true);
    try {
      let saved: Channel;
      if (isEdit && channel) {
        saved = await updateChannel(channel.id, {
          supplier_id: supplierId,
          channel_code: channelCode.trim(),
          name: name.trim(),
          path_prefix: pathPrefix.trim(),
          cost_price: normalizedPrice,
          qps_limit: qpsLimit ?? 0,
          status,
        });
      } else {
        saved = await createChannel({
          supplier_id: supplierId,
          channel_code: channelCode.trim(),
          name: name.trim(),
          path_prefix: pathPrefix.trim(),
          cost_price: normalizedPrice,
          qps_limit: qpsLimit ?? 0,
        });
      }
      showSuccessToast(t("common.feedback.saveSuccess"));
      onClose();
      await onSuccess(saved);
    } catch (error) {
      if (!isMfaStepUpCancelled(error)) {
        showErrorToast(getUserFacingErrorMessage(error, t("common.failed")));
      }
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <FormModal
      open={open}
      onClose={onClose}
      onSubmit={handleSubmit}
      title={
        isEdit
          ? t("channels.dialog.editTitle")
          : t("channels.dialog.createTitle")
      }
      size="md"
      submitLabel={t("common.actions.save")}
      cancelLabel={t("common.cancel")}
      submitting={submitting}
      testId="channel-form-modal"
    >
      <FormField
        label={t("channels.fields.supplier")}
        required
        error={errors.supplier_id}
      >
        <Select
          value={supplierId}
          options={supplierSelectOptions}
          searchable
          placeholder={t("channels.placeholders.supplierRemote")}
          onChange={(e) => {
            setSupplierId(e.target.value);
            if (errors.supplier_id) {
              setErrors((prev) => ({ ...prev, supplier_id: "" }));
            }
          }}
          className="w-full"
        />
      </FormField>

      <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
        <FormField
          label={t("channels.fields.channelCode")}
          htmlFor="channel-code-input"
          required
          error={errors.channel_code}
        >
          <Input
            id="channel-code-input"
            value={channelCode}
            onChange={(e) => setChannelCode(e.target.value)}
            placeholder={t("channels.placeholders.channelCode")}
            className="font-mono-tech"
          />
        </FormField>

        <FormField
          label={t("channels.fields.name")}
          htmlFor="channel-name-input"
          required
          error={errors.name}
        >
          <Input
            id="channel-name-input"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder={t("channels.placeholders.name")}
          />
        </FormField>
      </div>

      <FormField
        label={t("channels.fields.pathPrefix")}
        htmlFor="channel-path-prefix"
        required
        error={errors.path_prefix}
      >
        <Input
          id="channel-path-prefix"
          value={pathPrefix}
          onChange={(e) => setPathPrefix(e.target.value)}
          placeholder={t("channels.placeholders.pathPrefix")}
          className="font-mono-tech"
        />
      </FormField>

      <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
        <FormField
          label={t("channels.fields.costPrice")}
          htmlFor="channel-cost-price"
          required
          error={errors.cost_price}
          hint={t("channels.fields.costPriceHint")}
        >
          <Input
            id="channel-cost-price"
            inputMode="decimal"
            value={costPrice}
            onChange={(e) => setCostPrice(e.target.value)}
            onBlur={() => setCostPrice(normalizeCostPriceInput(costPrice))}
            placeholder={t("channels.placeholders.costPrice")}
            className="font-mono-tech"
          />
        </FormField>

        <FormField
          label={t("channels.fields.qpsLimit")}
          hint={t("channels.fields.qpsLimitHint")}
        >
          <div className="flex items-center gap-2">
            <NumberInput
              value={qpsLimit}
              min={0}
              step={1}
              onChange={(val) => setQpsLimit(Math.max(0, Math.round(val)))}
            />
            <span className="text-xs text-muted-foreground shrink-0">
              {t("channels.fields.qpsUnit")}
            </span>
          </div>
        </FormField>
      </div>

      {isEdit && (
        <FormField label={t("common.fields.status")}>
          <Select
            value={status}
            onChange={(e) => setStatus(e.target.value)}
            className="w-full"
            options={[
              {
                value: "active",
                label: t("channels.status.active"),
                text: t("channels.status.active"),
              },
              {
                value: "inactive",
                label: t("channels.status.inactive"),
                text: t("channels.status.inactive"),
              },
              {
                value: "suspended",
                label: t("channels.status.suspended"),
                text: t("channels.status.suspended"),
              },
            ]}
          />
        </FormField>
      )}
    </FormModal>
  );
}
