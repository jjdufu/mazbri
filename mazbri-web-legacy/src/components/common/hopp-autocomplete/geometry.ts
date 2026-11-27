/** Key-cell popper geometry for HoppAutoComplete. */

export type HoppAcHostRect = {
  top: number;
  left: number;
  width: number;
  height: number;
  bottom?: number;
  right?: number;
};

export type HoppAcPanelRect = {
  top: number;
  left: number;
  width: number;
};

/**
 * Hang the list from the cell bottom and overlay both vertical grid lines.
 * Floor/ceil (not round-on-width) so a fractional 键|值 divider is covered
 * instead of sitting 1px beside the popper border.
 */
export function hoppAcPanelRect(host: HoppAcHostRect): HoppAcPanelRect {
  const bottom = host.bottom ?? host.top + host.height;
  const hostRight = host.right ?? host.left + host.width;
  const left = Math.floor(host.left) - 1;
  const right = Math.ceil(hostRight);
  return {
    top: Math.round(bottom) - 1,
    left,
    width: Math.max(0, right - left),
  };
}

/**
 * When the host sits inside a flex/grid cell that draws the column divider
 * (assert editor), measure that cell. Header KV rows apply the divider on
 * the host itself, so they keep the host box.
 */
export function hoppAcAnchorEl(el: HTMLElement): HTMLElement {
  const parent = el.parentElement;
  if (!parent) return el;
  const cs = getComputedStyle(parent);
  if (
    parseFloat(cs.borderRightWidth) > 0 &&
    (cs.display === "flex" || cs.display === "grid")
  ) {
    return parent;
  }
  return el;
}
