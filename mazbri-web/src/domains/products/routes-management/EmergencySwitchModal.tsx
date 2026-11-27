import { FormField, FormModal, Input, Textarea } from "@/components/ui";
import { useT } from "@/locales";

export interface EmergencySwitchModalProps {
  open: boolean;
  onClose: () => void;
  saving: boolean;
  channelName: string;
  reason: string;
  onReasonChange: (next: string) => void;
  onConfirm: () => void | Promise<void>;
}

export function EmergencySwitchModal({
  open,
  onClose,
  saving,
  channelName,
  reason,
  onReasonChange,
  onConfirm,
}: EmergencySwitchModalProps) {
  const { t } = useT();

  return (
    <FormModal
      open={open}
      onClose={onClose}
      onSubmit={() => onConfirm()}
      title={t("routes.dialogs.emergencySwitchTitle")}
      size="sm"
      submitLabel={t("routes.actions.confirmSwitch")}
      cancelLabel={t("common.cancel")}
      submitting={saving}
      danger
      testId="emergency-switch-modal"
    >
      <FormField label={t("routes.fields.targetBinding")}>
        <Input value={channelName} disabled />
      </FormField>
      <FormField label={t("routes.fields.switchReason")} required>
        <Textarea
          value={reason}
          onChange={(e) => onReasonChange(e.target.value)}
          rows={4}
          placeholder={t("routes.placeholders.switchReason")}
        />
      </FormField>
    </FormModal>
  );
}
