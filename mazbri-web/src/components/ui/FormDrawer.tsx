import React, { useId } from "react";
import { Button, Drawer } from "./primitives";
import { useUiTx } from "./tx";

export interface FormDrawerProps {
  open: boolean;
  onClose: () => void;
  onSubmit: (e: React.FormEvent<HTMLFormElement>) => void | Promise<void>;
  title: React.ReactNode;
  subtitle?: React.ReactNode;
  size?: "sm" | "md" | "lg";
  submitLabel?: React.ReactNode;
  cancelLabel?: React.ReactNode;
  submitting?: boolean;
  submitDisabled?: boolean;
  danger?: boolean;
  closeOnBackdrop?: boolean;
  extraFooter?: React.ReactNode;
  testId?: string;
  closeTestId?: string;
  children: React.ReactNode;
}

export function FormDrawer({
  open,
  onClose,
  onSubmit,
  title,
  subtitle,
  size = "lg",
  submitLabel,
  cancelLabel,
  submitting = false,
  submitDisabled = false,
  danger = false,
  closeOnBackdrop = false,
  extraFooter,
  testId,
  closeTestId,
  children,
}: FormDrawerProps) {
  const { tx } = useUiTx();
  const formId = useId();

  const handleSubmit = (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    void onSubmit(e);
  };

  return (
    <Drawer
      open={open}
      onClose={submitting ? () => {} : onClose}
      title={title}
      subtitle={subtitle}
      size={size}
      closeOnBackdrop={closeOnBackdrop && !submitting}
      testId={testId}
      closeTestId={closeTestId}
      footer={
        <>
          {extraFooter}
          <Button
            type="button"
            variant="outline"
            onClick={onClose}
            disabled={submitting}
          >
            {cancelLabel ?? tx("取消", "Cancel")}
          </Button>
          <Button
            type="submit"
            form={formId}
            variant={danger ? "danger" : "primary"}
            loading={submitting}
            disabled={submitDisabled}
          >
            {submitLabel ?? tx("保存", "Save")}
          </Button>
        </>
      }
    >
      <form id={formId} onSubmit={handleSubmit} className="space-y-4">
        {children}
      </form>
    </Drawer>
  );
}
