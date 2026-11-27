



export function formatFileSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) {
    return "0 B";
  }
  const units = ["B", "KB", "MB", "GB"];
  let size = bytes;
  let unitIndex = 0;
  while (size >= 1024 && unitIndex < units.length - 1) {
    size /= 1024;
    unitIndex += 1;
  }
  const precision = size >= 10 || unitIndex === 0 ? 0 : 1;
  return `${size.toFixed(precision)} ${units[unitIndex]}`;
}


export function matchesAccept(file: File, accept?: string): boolean {
  if (!accept || accept.trim() === "" || accept.trim() === "*") {
    return true;
  }
  const tokens = accept
    .split(",")
    .map((token) => token.trim().toLowerCase())
    .filter(Boolean);
  if (tokens.length === 0) {
    return true;
  }
  const name = file.name.toLowerCase();
  const type = (file.type || "").toLowerCase();
  return tokens.some((token) => {
    if (token.startsWith(".")) {
      return name.endsWith(token);
    }
    if (token.endsWith("/*")) {
      const prefix = token.slice(0, -1);
      return type.startsWith(prefix);
    }
    if (type && type === token) {
      return true;
    }

    if (!token.includes("/") && name.endsWith(`.${token}`)) {
      return true;
    }
    return false;
  });
}

export type FilePickerValidateResult = true | string;

export function validatePickedFile(
  file: File,
  options: {
    accept?: string;
    maxSize?: number;
    validate?: (file: File) => FilePickerValidateResult;
    invalidTypeMessage?: string;
    fileTooLargeMessage?: (maxSize: number) => string;
  },
): FilePickerValidateResult {
  if (!matchesAccept(file, options.accept)) {
    return options.invalidTypeMessage ?? "invalid type";
  }
  if (
    options.maxSize !== undefined &&
    Number.isFinite(options.maxSize) &&
    options.maxSize > 0 &&
    file.size > options.maxSize
  ) {
    return (
      options.fileTooLargeMessage?.(options.maxSize) ?? "file too large"
    );
  }
  if (options.validate) {
    return options.validate(file);
  }
  return true;
}
