/** Prefer compose service / k8s container|pod / docker name / image repo name; else short id. */
export function containerDisplayName(
  labels: Record<string, string> | null | undefined,
): string {
  const l = labels || {};
  const compose = l.container_label_com_docker_compose_service?.trim();
  if (compose) return compose;
  const k8sContainer = l.container_label_io_kubernetes_container_name?.trim();
  if (k8sContainer) return k8sContainer;
  const k8sPod = l.container_label_io_kubernetes_pod_name?.trim();
  if (k8sPod) return k8sPod;
  const name = l.name?.trim();
  if (name && name !== "/" && !looksLikeContainerId(name)) {
    return name.replace(/^\//, "");
  }
  const fromImage = imageBaseName(l.image);
  if (fromImage) return fromImage;
  const id = (l.id || name || "").trim();
  if (!id) return "unknown";
  const leaf = id.split("/").filter(Boolean).pop() || id;
  return leaf.slice(0, 12);
}

/** True when the series carries a human identity label (not id-only). */
export function hasContainerIdentity(
  labels: Record<string, string> | null | undefined,
): boolean {
  const l = labels || {};
  if (l.container_label_com_docker_compose_service?.trim()) return true;
  if (l.container_label_io_kubernetes_container_name?.trim()) return true;
  if (l.container_label_io_kubernetes_pod_name?.trim()) return true;
  const name = l.name?.trim();
  if (name && name !== "/" && !looksLikeContainerId(name)) return true;
  if (imageBaseName(l.image)) return true;
  return false;
}

function looksLikeContainerId(value: string): boolean {
  const v = value.replace(/^\//, "");
  return /^[0-9a-f]{12,64}$/i.test(v);
}

/** `docker.io/dex2all/mazbri-gateway:0.2.11` → `mazbri-gateway` */
function imageBaseName(image: string | undefined): string | null {
  if (!image) return null;
  const noDigest = image.split("@")[0] || "";
  const noTag = noDigest.split(":")[0] || "";
  const leaf = noTag.split("/").filter(Boolean).pop() || "";
  if (!leaf || leaf === "pause" || leaf === "POD") return null;
  return leaf;
}

/** Remap timeseries labels so charts show a single readable container name; drop id-only. */
export function withContainerDisplayName<
  T extends { labels: Record<string, string> },
>(series: T[]): T[] {
  return series
    .filter((s) => hasContainerIdentity(s.labels))
    .map((s) => ({
      ...s,
      labels: { container: containerDisplayName(s.labels) },
    }));
}
