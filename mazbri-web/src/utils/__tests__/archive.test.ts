import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  createTarHeader,
  packageTarArchive,
  sanitizeTarEntryName,
} from "../archive";

function readNullTerminatedUtf8(bytes: Uint8Array): string {
  const zeroIdx = bytes.indexOf(0);
  const slice = zeroIdx >= 0 ? bytes.subarray(0, zeroIdx) : bytes;
  return new TextDecoder("utf-8", { fatal: true }).decode(slice);
}

describe("archive security and tar packaging helpers", () => {
  it("strips path separators, parent traversal segments, and control characters", () => {
    const dirty = "../../etc/passwd\\..\\.\u0000\u001fbill\r\n-01.csv";
    const sanitized = sanitizeTarEntryName(dirty);
    assert.equal(sanitized.includes("/"), false);
    assert.equal(sanitized.includes("\\"), false);
    assert.equal(sanitized.includes(".."), false);
    assert.equal(/[\u0000-\u001f\u007f-\u009f]/.test(sanitized), false);
    assert.equal(sanitized, "etcpasswdbill-01.csv");
  });

  it("truncates long multibyte prefixes on UTF-8 codepoint boundaries while preserving trailing UUID and .csv", () => {
    const uuid = "550e8400-e29b-41d4-a716-446655440000";
    const longTenantPrefix = "超长租户名称测试甲乙丙丁戊己庚辛壬癸子丑寅卯辰巳午未申酉戌亥".repeat(
      2,
    );
    const rawName = `${longTenantPrefix}-20261101-20261130-${uuid}.csv`;
    const sanitized = sanitizeTarEntryName(rawName);
    const encoded = new TextEncoder().encode(sanitized);

    assert.ok(encoded.length <= 99, `expected <= 99 bytes, got ${encoded.length}`);
    assert.ok(
      sanitized.endsWith(`-${uuid}.csv`),
      `expected trailing UUID+.csv to be preserved, got ${sanitized}`,
    );
    // Ensure valid UTF-8 without replacement chars
    assert.equal(sanitized.includes("\uFFFD"), false);

    const header = createTarHeader(rawName, 128, 1_700_000_000);
    const headerName = readNullTerminatedUtf8(header.subarray(0, 100));
    assert.equal(headerName, sanitized);
    assert.equal(header[99], 0);
  });

  it("falls back to uncompressed .tar and application/x-tar when CompressionStream is unavailable", async () => {
    const globalRef = globalThis as unknown as {
      CompressionStream?: typeof CompressionStream;
    };
    const originalCompressionStream = globalRef.CompressionStream;
    try {
      delete globalRef.CompressionStream;
      const payload = await packageTarArchive(
        [
          {
            name: "../tenant-demo.csv",
            content: new TextEncoder().encode("a,b\n1,2\n"),
          },
        ],
        "bills-export.tar.gz",
      );
      assert.equal(payload.compressed, false);
      assert.equal(payload.filename, "bills-export.tar");
      assert.equal(payload.mimeType, "application/x-tar");
      assert.equal(
        readNullTerminatedUtf8(payload.data.subarray(0, 100)),
        "tenant-demo.csv",
      );
    } finally {
      if (originalCompressionStream) {
        globalRef.CompressionStream = originalCompressionStream;
      }
    }
  });

  it("produces .tar.gz with application/gzip when CompressionStream is available", async () => {
    if (typeof globalThis.CompressionStream !== "function") {
      return;
    }
    const payload = await packageTarArchive(
      [
        {
          name: "sample.csv",
          content: new TextEncoder().encode("col\nval\n"),
        },
      ],
      "sample-archive.tar.gz",
    );
    assert.equal(payload.compressed, true);
    assert.equal(payload.filename, "sample-archive.tar.gz");
    assert.equal(payload.mimeType, "application/gzip");
    // Gzip magic header 0x1f 0x8b
    assert.equal(payload.data[0], 0x1f);
    assert.equal(payload.data[1], 0x8b);
  });
});
