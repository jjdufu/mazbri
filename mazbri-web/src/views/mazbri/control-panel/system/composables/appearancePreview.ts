import { watch, type WatchStopHandle } from "vue";

/**
 * Live preview controller for appearance settings.
 *
 * - Every change to `local` is applied immediately via `apply` (preview only,
 *   nothing is sent to the server here).
 * - `baseline` holds the last saved / server values; `commit()` adopts the
 *   current store state as the new baseline (on load and after save).
 * - `revert()` restores the baseline when the user leaves without saving.
 */
export interface AppearancePreviewOptions<T extends object> {
  /** Current values held by the global store (what is on screen now). */
  read: () => T;
  /** Apply the given values to the store + DOM (no persistence). */
  apply: (values: T) => void;
  /** Preview only while the settings page is active (default: always). */
  isActive?: () => boolean;
}

export function cloneAppearance<T extends object>(values: T): T {
  return { ...values };
}

export function isSameAppearance<T extends object>(a: T, b: T): boolean {
  const keys = new Set([...Object.keys(a), ...Object.keys(b)]) as Set<keyof T>;
  for (const key of keys) {
    if (a[key] !== b[key]) {
      return false;
    }
  }
  return true;
}

export function createAppearancePreview<T extends object>(
  local: T,
  options: AppearancePreviewOptions<T>,
) {
  let baseline: T = cloneAppearance(options.read());

  /** Push local values to the store/DOM unless they already match. */
  const preview = () => {
    if (options.isActive && !options.isActive()) {
      return;
    }
    if (!isSameAppearance(cloneAppearance(local), options.read())) {
      options.apply(cloneAppearance(local));
    }
  };

  const stop: WatchStopHandle = watch(() => cloneAppearance(local), preview, {
    deep: true,
  });

  /** Take the current store state as the saved baseline. */
  const commit = () => {
    baseline = cloneAppearance(options.read());
  };

  /** Restore store/DOM and local values to the saved baseline. */
  const revert = () => {
    const saved = cloneAppearance(baseline);
    Object.assign(local, saved);
    if (!isSameAppearance(options.read(), saved)) {
      options.apply(saved);
    }
  };

  const isDirty = () => !isSameAppearance(options.read(), baseline);

  return {
    preview,
    commit,
    revert,
    isDirty,
    stop,
    getBaseline: () => cloneAppearance(baseline),
  };
}
