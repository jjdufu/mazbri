




const DEFAULT_MAX_COMPACT = 120;
const DEFAULT_EXPAND = 360;

export interface ExpandOnFocusAutoOptions {

  maxCompactWidth?: number;

  expandWidth?: number;

  root?: ParentNode | null;
}

function isEligibleTextInput(el: Element): el is HTMLElement {
  if (!(el instanceof HTMLElement)) return false;
  if (!el.classList.contains("el-input")) return false;
  if (el.classList.contains("el-input--textarea")) return false;

  if (el.closest(".el-input-number")) return false;

  if (el.dataset.eofManaged === "component") return false;
  return true;
}

function applyExpandClass(
  el: HTMLElement,
  maxCompactWidth: number,
  expandWidth: number,
) {

  if (el.matches(":focus-within")) return;

  const width = el.getBoundingClientRect().width;
  if (width <= 0) return;

  if (width <= maxCompactWidth + 0.5) {
    el.classList.add("expand-on-focus-input");
    el.style.setProperty("--eof-compact-width", `${Math.round(width)}px`);
    el.style.setProperty("--eof-expand-width", `${expandWidth}px`);
    el.dataset.eofManaged = "auto";
    return;
  }


  if (el.dataset.eofManaged === "auto") {
    el.classList.remove("expand-on-focus-input");
    el.style.removeProperty("--eof-compact-width");
    el.style.removeProperty("--eof-expand-width");
    delete el.dataset.eofManaged;
  }
}

function scan(
  root: ParentNode,
  maxCompactWidth: number,
  expandWidth: number,
) {
  const nodes = root.querySelectorAll?.(".el-input") ?? [];
  nodes.forEach((node) => {
    if (!isEligibleTextInput(node)) return;
    applyExpandClass(node, maxCompactWidth, expandWidth);
  });
}





export function startExpandOnFocusInputAuto(
  options: ExpandOnFocusAutoOptions = {},
): () => void {
  const maxCompactWidth = options.maxCompactWidth ?? DEFAULT_MAX_COMPACT;
  const expandWidth = options.expandWidth ?? DEFAULT_EXPAND;
  const root = options.root ?? document.body;

  let scheduled = false;
  const scheduleScan = () => {
    if (scheduled) return;
    scheduled = true;
    requestAnimationFrame(() => {
      scheduled = false;
      scan(root, maxCompactWidth, expandWidth);
    });
  };

  scheduleScan();

  const mo = new MutationObserver(scheduleScan);
  mo.observe(root, {
    childList: true,
    subtree: true,
    attributes: true,
    attributeFilter: ["class", "style"],
  });

  window.addEventListener("resize", scheduleScan, { passive: true });

  return () => {
    mo.disconnect();
    window.removeEventListener("resize", scheduleScan);
  };
}
