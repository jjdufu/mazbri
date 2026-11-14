export type WaterfallDurMode = "outside-right" | "inside-bar" | "outside-left";

export interface WaterfallDurPlacement {
  mode: WaterfallDurMode;
  style: {
    left?: string;
    right?: string;
  };
}

const RIGHT_SAFE_LIMIT_PCT = 70;
const INSIDE_MIN_WIDTH_PCT = 30;

/**
 * Computes non-overflowing horizontal placement for a span duration badge inside
 * `.ops-trace__track` given `left` (0..100) and `width` (0..100).
 *
 * 1. `left + width <= 70`: enough room on the right -> place outside right of bar.
 * 2. `left + width > 70 && width >= 30`: wide bar reaching right zone -> place inside right edge of bar.
 * 3. `left + width > 70 && width < 30`: short tail bar (`left > 40`) -> place outside left of bar.
 */
export function resolveWaterfallDurPlacement(
  left: number,
  width: number,
): WaterfallDurPlacement {
  const safeLeft = Math.max(0, Math.min(100, left));
  const safeWidth = Math.max(0, Math.min(100 - safeLeft, width));
  const end = safeLeft + safeWidth;

  if (end <= RIGHT_SAFE_LIMIT_PCT) {
    return {
      mode: "outside-right",
      style: { left: `calc(${end}% + 6px)` },
    };
  }

  if (safeWidth >= INSIDE_MIN_WIDTH_PCT) {
    const rightGap = Math.max(0, 100 - end);
    return {
      mode: "inside-bar",
      style: { right: `calc(${rightGap}% + 6px)` },
    };
  }

  const fromRightToBarStart = Math.max(0, 100 - safeLeft);
  return {
    mode: "outside-left",
    style: { right: `calc(${fromRightToBarStart}% + 6px)` },
  };
}
