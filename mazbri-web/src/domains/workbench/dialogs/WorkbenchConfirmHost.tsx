import React from "react";
import { ConfirmDialog } from "@/components/ui";
import { useWorkbenchStore } from "../store";

export function WorkbenchConfirmHost() {
  const confirmState = useWorkbenchStore((s) => s.confirmState);
  const resolveConfirm = useWorkbenchStore((s) => s.resolveConfirm);

  if (!confirmState || !confirmState.open) return null;

  return (
    <ConfirmDialog
      open={confirmState.open}
      onClose={() => resolveConfirm(false)}
      onConfirm={() => resolveConfirm(true)}
      title={confirmState.title}
      description={confirmState.description}
      confirmLabel={confirmState.confirmLabel}
      danger={confirmState.danger ?? true}
    />
  );
}
