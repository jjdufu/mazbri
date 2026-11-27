import { useEffect } from "react";
import { useNavigate } from "react-router";
import { useLockScreenStore } from "@/stores/lockscreen";
import {
  selectActiveRuntime,
  selectCanWriteWorkbench,
  useWorkbenchStore,
} from "../store";

export interface UseWorkbenchHotkeysOptions {
  enabled?: boolean;
}

export function useWorkbenchHotkeys(
  options: UseWorkbenchHotkeysOptions = {},
): void {
  const { enabled = true } = options;
  const navigate = useNavigate();

  useEffect(() => {
    if (!enabled || typeof window === "undefined") return;

    const onKeyDown = (e: KeyboardEvent) => {
      if (useLockScreenStore.getState().isLocked) return;

      const store = useWorkbenchStore.getState();

      if (e.key === "Escape") {
        if (store.showCommands) {
          e.preventDefault();
          store.setShowCommands(false);
          return;
        }
        if (store.showShortcuts) {
          e.preventDefault();
          store.setShowShortcuts(false);
          return;
        }
        return;
      }

      const meta = e.ctrlKey || e.metaKey;
      if (!meta) return;

      if (e.key === "Enter") {
        e.preventDefault();
        if (!selectCanWriteWorkbench(store)) return;
        const rt = selectActiveRuntime(store);
        if (rt.running) {
          store.abortActiveRun();
        } else {
          void store.doRun("business");
        }
        return;
      }

      const lower = e.key.toLowerCase();
      if (lower === "s") {
        e.preventDefault();
        if (selectCanWriteWorkbench(store)) {
          void store.saveDraftExplicit(navigate);
        }
      } else if (lower === "b") {
        e.preventDefault();
        store.toggleSidebar();
      } else if (lower === "k") {
        e.preventDefault();
        store.setShowCommands(!store.showCommands);
        store.setShowShortcuts(false);
      } else if (e.key === "/") {
        e.preventDefault();
        store.setShowShortcuts(!store.showShortcuts);
        store.setShowCommands(false);
      }
    };

    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [enabled, navigate]);
}
