






function toValidDate(date: string | Date | null | undefined): Date | null {
  if (date == null || date === "") return null;
  const dateObj = typeof date === "string" ? new Date(date) : date;
  if (!(dateObj instanceof Date) || Number.isNaN(dateObj.getTime())) {
    return null;
  }
  return dateObj;
}





export function formatDate(date: string | Date | null | undefined): string {
  if (date == null || date === "") return "-";
  const dateObj = toValidDate(date);
  if (!dateObj) return String(date);
  return dateObj.toISOString().slice(0, 10);
}




export function formatDateTime(date: string | Date | null | undefined): string {
  if (date == null || date === "") return "-";
  const dateObj = toValidDate(date);
  if (!dateObj) return String(date);
  return dateObj.toISOString();
}

export function formatAmount(amount?: string | number | null): string {
  if (amount == null || amount === "") return "-";
  const numericValue = typeof amount === "string" ? parseFloat(amount) : amount;
  if (!Number.isFinite(numericValue)) return "0.00";

  return new Intl.NumberFormat("zh-CN", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  }).format(numericValue);
}

export function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(2))} ${sizes[i]}`;
}
