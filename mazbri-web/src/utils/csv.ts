export function parseCsvLine(line: string): string[] {
  const result: string[] = [];
  let current = "";
  let inQuotes = false;
  for (let i = 0; i < line.length; i++) {
    const char = line[i];
    const nextChar = line[i + 1];
    if (char === '"') {
      if (inQuotes && nextChar === '"') {
        current += '"';
        i++;
      } else {
        inQuotes = !inQuotes;
      }
    } else if (char === "," && !inQuotes) {
      result.push(current);
      current = "";
    } else {
      current += char;
    }
  }
  result.push(current);
  return result;
}
export function parseCsvRecords(content: string): string[] {
  const normalized = content.replace(/\r\n/g, "\n");
  const records: string[] = [];
  let current = "";
  let inQuotes = false;
  for (let i = 0; i < normalized.length; i++) {
    const char = normalized[i];
    const nextChar = normalized[i + 1];
    current += char;
    if (char === '"') {
      if (inQuotes && nextChar === '"') {
        current += nextChar;
        i++;
      } else {
        inQuotes = !inQuotes;
      }
    } else if (char === "\n" && !inQuotes) {
      records.push(current.slice(0, -1));
      current = "";
    }
  }
  if (current.trim()) {
    records.push(current);
  }
  return records.filter((record) => record.trim());
}
export function parseCsv<T extends Record<string, unknown>>(
  content: string,
  columnMapping: Record<string, keyof T>,
): T[] {
  const lines = parseCsvRecords(content);
  if (lines.length < 2) {
    return [];
  }
  const headerLine = lines[0].toLowerCase();
  const headers = parseCsvLine(headerLine);
  const columnIndices: Record<string, number> = {};
  for (const csvKey of Object.keys(columnMapping)) {
    const objectKey = columnMapping[csvKey];
    const index = headers.findIndex((h) => h.trim() === csvKey);
    if (index >= 0) {
      columnIndices[objectKey as string] = index;
    }
  }
  const results: T[] = [];
  for (let i = 1; i < lines.length; i++) {
    const line = lines[i];
    if (!line.trim()) continue;
    const columns = parseCsvLine(line);
    const obj: Record<string, unknown> = {};
    for (const [objectKey, csvKey] of Object.entries(columnMapping)) {
      const index = columnIndices[objectKey];
      if (index !== undefined && index < columns.length) {
        obj[objectKey] = columns[index] || "";
      }
    }
    results.push(obj as T);
  }
  return results;
}
export function generateCsv<T extends Record<string, unknown>>(
  data: T[],
  columns: {
    key: keyof T;
    label: string;
  }[],
  options: {
    withBOM?: boolean;
  } = {},
): string {
  const { withBOM = true } = options;
  let csv = withBOM ? "\uFEFF" : "";
  csv += columns.map((col) => col.label).join(",") + "\n";
  for (const item of data) {
    const row: string[] = [];
    for (const col of columns) {
      const value = item[col.key];
      if (value === undefined || value === null) {
        row.push("");
      } else if (typeof value === "string") {
        if (
          value.includes(",") ||
          value.includes("\n") ||
          value.includes('"')
        ) {
          row.push(`"${value.replace(/"/g, '""')}"`);
        } else {
          row.push(value);
        }
      } else if (Array.isArray(value)) {
        row.push(`"${value.join("\n")}"`);
      } else {
        row.push(String(value));
      }
    }
    csv += row.join(",") + "\n";
  }
  return csv;
}
export function downloadCsv(content: string, filename: string): void {
  const blob = new Blob([content], { type: "text/csv;charset=utf-8;" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  document.body.appendChild(link);
  link.click();
  document.body.removeChild(link);
  URL.revokeObjectURL(url);
}

export function serializeRawCsv(rows: unknown[][]): string {
  return rows
    .map((row) =>
      row
        .map((val) => {
          const text = String(val ?? "");
          return `"${text.replace(/"/g, '""')}"`;
        })
        .join(","),
    )
    .join("\n");
}
