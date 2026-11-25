export interface ArchiveFile {
  name: string;
  content: Uint8Array;
}

export interface PackagedTarArchive {
  filename: string;
  data: Uint8Array;
  mimeType: "application/gzip" | "application/x-tar";
  compressed: boolean;
}

const TAR_NAME_MAX_BYTES = 99;
const CONTROL_CHARS_RE = /[\u0000-\u001f\u007f-\u009f]+/g;
const PRESERVED_TAIL_RE =
  /(-?[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12})?(\.[A-Za-z0-9]{1,16})?$/;

function truncateUtf8ByCodePoints(input: string, maxBytes: number): string {
  if (maxBytes <= 0 || !input) return "";
  const encoder = new TextEncoder();
  if (encoder.encode(input).length <= maxBytes) {
    return input;
  }
  let result = "";
  let usedBytes = 0;
  for (const ch of input) {
    const chBytes = encoder.encode(ch).length;
    if (usedBytes + chBytes > maxBytes) {
      break;
    }
    result += ch;
    usedBytes += chBytes;
  }
  return result;
}

export function sanitizeTarEntryName(
  rawName: string,
  maxBytes = TAR_NAME_MAX_BYTES,
): string {
  let cleaned = rawName.replace(CONTROL_CHARS_RE, "");
  // Strip path separators and '..' segments iteratively so nested tricks cannot reassemble traversal
  let previous = "";
  while (cleaned !== previous) {
    previous = cleaned;
    cleaned = cleaned.replace(/[\\/]+/g, "").replace(/\.\.+/g, "");
  }
  cleaned = cleaned.replace(/^\.+/, "").trim();
  if (!cleaned) {
    cleaned = "entry";
  }

  const encoder = new TextEncoder();
  if (encoder.encode(cleaned).length <= maxBytes) {
    return cleaned;
  }

  const tailMatch = cleaned.match(PRESERVED_TAIL_RE);
  const suffix = tailMatch?.[0] ?? "";
  const suffixBytes = encoder.encode(suffix).length;
  if (!suffix || suffixBytes >= maxBytes) {
    return truncateUtf8ByCodePoints(cleaned, maxBytes) || "entry";
  }

  const prefixSource = cleaned.slice(0, cleaned.length - suffix.length);
  const truncatedPrefix = truncateUtf8ByCodePoints(
    prefixSource,
    maxBytes - suffixBytes,
  );
  return `${truncatedPrefix}${suffix}` || "entry";
}

export const downloadBinary = (
  filename: string,
  data: BlobPart,
  mimeType = "application/octet-stream",
) => {
  const blob = new Blob([data], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  link.click();
  URL.revokeObjectURL(url);
};

export const createTarHeader = (
  name: string,
  size: number,
  mtime: number,
): Uint8Array => {
  const safeName = sanitizeTarEntryName(name, TAR_NAME_MAX_BYTES);
  const header = new Uint8Array(512);
  const encoder = new TextEncoder();
  const writeString = (offset: number, length: number, value: string) => {
    const bytes = encoder.encode(value);
    header.set(bytes.slice(0, length), offset);
  };
  const writeOctal = (offset: number, length: number, value: number) => {
    const octal = Math.max(0, Math.trunc(value)).toString(8);
    const padded = octal.padStart(length - 1, "0");
    writeString(offset, length - 1, padded);
    header[offset + length - 1] = 0;
  };
  writeString(0, 100, safeName);
  writeOctal(100, 8, 0o644);
  writeOctal(108, 8, 0);
  writeOctal(116, 8, 0);
  writeOctal(124, 12, size);
  writeOctal(136, 12, mtime);
  header.fill(0x20, 148, 156);
  header[156] = 0x30;
  writeString(257, 6, "ustar\0");
  writeString(263, 2, "00");
  let checksum = 0;
  for (const byte of header) {
    checksum += byte;
  }
  const checksumOctal = checksum.toString(8).padStart(6, "0");
  writeString(148, 6, checksumOctal);
  header[154] = 0;
  header[155] = 0x20;
  return header;
};

export const concatBytes = (chunks: Uint8Array[]): Uint8Array => {
  const totalSize = chunks.reduce((sum, chunk) => sum + chunk.length, 0);
  const merged = new Uint8Array(totalSize);
  let offset = 0;
  for (const chunk of chunks) {
    merged.set(chunk, offset);
    offset += chunk.length;
  }
  return merged;
};

export const createTarArchive = (files: ArchiveFile[]): Uint8Array => {
  const parts: Uint8Array[] = [];
  const now = Math.floor(Date.now() / 1000);
  for (const file of files) {
    const data = file.content;
    const header = createTarHeader(file.name, data.length, now);
    parts.push(header, data);
    const padding = (512 - (data.length % 512)) % 512;
    if (padding > 0) {
      parts.push(new Uint8Array(padding));
    }
  }
  parts.push(new Uint8Array(1024));
  return concatBytes(parts);
};

function getCompressionStreamCtor():
  | (new (format: string) => CompressionStream)
  | undefined {
  return (
    globalThis as unknown as {
      CompressionStream?: new (format: string) => CompressionStream;
    }
  ).CompressionStream;
}

export const gzipBytes = async (bytes: Uint8Array): Promise<Uint8Array> => {
  const CompressionStreamCtor = getCompressionStreamCtor();
  if (!CompressionStreamCtor) {
    return bytes;
  }
  const stream = new Blob([bytes])
    .stream()
    .pipeThrough(new CompressionStreamCtor("gzip"));
  const arrayBuffer = await new Response(stream).arrayBuffer();
  return new Uint8Array(arrayBuffer);
};

function stripArchiveExtension(filename: string): string {
  return filename.replace(/\.(?:tar\.gz|tgz|tar)$/i, "");
}

export const packageTarArchive = async (
  files: ArchiveFile[],
  archiveName: string,
): Promise<PackagedTarArchive> => {
  const base = sanitizeTarEntryName(stripArchiveExtension(archiveName), 180);
  const tarBytes = createTarArchive(files);
  if (!getCompressionStreamCtor()) {
    return {
      filename: `${base}.tar`,
      data: tarBytes,
      mimeType: "application/x-tar",
      compressed: false,
    };
  }
  const gzBytes = await gzipBytes(tarBytes);
  return {
    filename: `${base}.tar.gz`,
    data: gzBytes,
    mimeType: "application/gzip",
    compressed: true,
  };
};
