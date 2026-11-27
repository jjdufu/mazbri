import { AppRouteRecord } from "@/types/router";
import { router } from "@/router";
import { logger } from "@/utils/sys";
const ALLOWED_PROTOCOLS = ["http:", "https:"];
function isValidExternalUrl(url: string): boolean {
  try {
    const parsed = new URL(url);
    return ALLOWED_PROTOCOLS.includes(parsed.protocol);
  } catch {
    return false;
  }
}
export const safeOpenExternalUrl = (url: string): void => {
  if (!isValidExternalUrl(url)) {
    logger.warn(`Blocked unsafe external URL: ${url}`);
    return;
  }
  window.open(url, "_blank", "noopener,noreferrer");
};
export const openExternalLink = (link: string) => {
  safeOpenExternalUrl(link);
};
export const handleMenuJump = (
  item: AppRouteRecord,
  jumpToFirst: boolean = false,
) => {
  const { link, isIframe } = item.meta;
  if (link && !isIframe) {
    return openExternalLink(link);
  }
  if (!jumpToFirst || !item.children?.length) {
    return router.push(item.path);
  }
  const findFirstLeafMenu = (items: AppRouteRecord[]): AppRouteRecord => {
    for (const child of items) {
      if (!child.meta.isHide) {
        return child.children?.length
          ? findFirstLeafMenu(child.children)
          : child;
      }
    }
    return items[0];
  };
  const firstChild = findFirstLeafMenu(item.children);
  if (firstChild.meta?.link) {
    return openExternalLink(firstChild.meta.link);
  }
  router.push(firstChild.path);
};
