import { useRouter, type LocationQueryRaw } from "vue-router";

export interface DrilldownOptions {
  path: string;
  query?: LocationQueryRaw;
  /** Open in a new browser tab/window when true. */
  newWindow?: boolean;
}

/** Navigate to a detail page with filter query params preserved. */
export function useDrilldown() {
  const router = useRouter();

  const drilldown = (options: DrilldownOptions) => {
    const resolved = router.resolve({
      path: options.path,
      query: options.query,
    });
    if (options.newWindow) {
      window.open(resolved.href, "_blank", "noopener,noreferrer");
      return;
    }
    void router.push({ path: options.path, query: options.query });
  };

  return { drilldown };
}
