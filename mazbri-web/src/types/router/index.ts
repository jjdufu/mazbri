export type LocationQueryValue = string | number | boolean | null | undefined;
export type LocationQueryRaw = Record<
  string,
  LocationQueryValue | LocationQueryValue[]
>;

export interface RouteLinkLocation {
  path: string;
  query?: Record<string, string>;
  hash?: string;
}

export type RouteLocationRaw = string | RouteLinkLocation;
