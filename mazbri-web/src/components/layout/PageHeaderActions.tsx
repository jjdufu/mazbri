import { createContext, useContext, type ReactNode } from "react";
import { createPortal } from "react-dom";

export interface PageHeaderSlotContextValue {
  readonly hasHost: boolean;
  readonly slotEl: HTMLDivElement | null;
}

export const PageHeaderSlotContext = createContext<PageHeaderSlotContextValue>({
  hasHost: false,
  slotEl: null,
});

export interface PageHeaderActionsProps {
  children: ReactNode;
}

export function PageHeaderActions({ children }: PageHeaderActionsProps) {
  const { hasHost, slotEl } = useContext(PageHeaderSlotContext);
  if (hasHost) {
    return slotEl ? createPortal(children, slotEl) : null;
  }
  return <div className="flex items-center justify-end shrink-0">{children}</div>;
}
