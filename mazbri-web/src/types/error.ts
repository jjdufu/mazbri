export interface ProblemDetails {
  type: string;
  title: string;
  status: number;
  detail?: string;
  instance?: string;
  trace_id?: string;
}
export function extractErrorCode(type: string): string {
  const segments = type.split("/");
  return segments[segments.length - 1] || "UNKNOWN";
}
