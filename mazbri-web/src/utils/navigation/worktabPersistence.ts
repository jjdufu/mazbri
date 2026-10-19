import type { WorkTab } from "@/types";
export interface WorktabStateSnapshot {
  current: Partial<WorkTab>;
  opened: WorkTab[];
  keepAliveExclude: string[];
}
export function createWorktabStateSnapshot(options: {
  current: Partial<WorkTab>;
  opened: WorkTab[];
  keepAliveExclude: string[];
}): WorktabStateSnapshot {
  const { current, opened, keepAliveExclude } = options;
  return {
    current: { ...current },
    opened: [...opened],
    keepAliveExclude: [...keepAliveExclude],
  };
}
export function getWorktabPersistOptions() {
  return {
    key: "worktab",
    storage: localStorage,
  };
}
