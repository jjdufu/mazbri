export interface ArchiveFile {
  name: string;
  content: Uint8Array;
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
  writeString(0, 100, name);
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

export const gzipBytes = async (bytes: Uint8Array): Promise<Uint8Array> => {
  const CompressionStreamCtor = (
    globalThis as unknown as {
      CompressionStream?: new (format: string) => CompressionStream;
    }
  ).CompressionStream;
  if (!CompressionStreamCtor) {
    return bytes;
  }
  const stream = new Blob([bytes])
    .stream()
    .pipeThrough(new CompressionStreamCtor("gzip"));
  const arrayBuffer = await new Response(stream).arrayBuffer();
  return new Uint8Array(arrayBuffer);
};
