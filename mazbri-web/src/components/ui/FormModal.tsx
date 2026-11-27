import React, { useId } from "react";
import { cn } from "./cn";
import { Button, Modal } from "./primitives";
import { useUiTx } from "./tx";

export interface FormFieldProps {
  label: React.ReactNode;
  htmlFor?: string;
  required?: boolean;
  error?: React.ReactNode;
  hint?: React.ReactNode;
  children: React.ReactNode;
  className?: string;
}

export function FormField({
  label,
  htmlFor,
  required = false,
  error,
  hint,
  children,
  className,
}: FormFieldProps) {
  return (
    <div className={cn("flex flex-col gap-1.5 min-w-0", className)}>
      <label
        htmlFor={htmlFor}
        className="text-xs font-medium text-foreground leading-snug inline-flex items-center gap-1"
      >
        <span>{label}</span>
        {required && (
          <span aria-hidden="true" className="text-danger-fg">
            *
          </span>
        )}
      </label>
      <div className="min-w-0">{children}</div>
      {error ? (
        <p className="text-[11px] text-danger-fg leading-snug">{error}</p>
      ) : hint ? (
        <p className="text-[11px] text-muted-foreground leading-snug">{hint}</p>
      ) : null}
    </div>
  );
}

export interface FormModalProps {
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

export function FormModal({
  open,
  onClose,
  onSubmit,
  title,
  subtitle,
  size = "md",
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
}: FormModalProps) {
  const { tx } = useUiTx();
  const formId = useId();

  const handleSubmit = (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    void onSubmit(e);
  };

  return (
    <Modal
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
    </Modal>
  );
}
