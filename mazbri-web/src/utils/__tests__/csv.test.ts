import assert from "node:assert/strict";
import { describe, it } from "node:test";
import {
  generateCsv,
  parseCsv,
  sanitizeCsvFormulaCell,
  serializeRawCsv,
} from "../csv";

describe("csv security and parsing helpers", () => {
  it("prefixes formula-injection payloads while preserving pure decimal negatives", () => {
    assert.equal(sanitizeCsvFormulaCell("=SUM(A1:A2)"), "'=SUM(A1:A2)");
    assert.equal(sanitizeCsvFormulaCell("+cmd|' /C calc'!A0"), "'+cmd|' /C calc'!A0");
    assert.equal(sanitizeCsvFormulaCell("@SUM(1+1)"), "'@SUM(1+1)");
    assert.equal(sanitizeCsvFormulaCell("\t=1+1"), "'\t=1+1");
    assert.equal(sanitizeCsvFormulaCell("\r=1+1"), "'\r=1+1");
    assert.equal(sanitizeCsvFormulaCell("-2+3"), "'-2+3");
    assert.equal(sanitizeCsvFormulaCell("-cmd"), "'-cmd");

    // Pure decimal negative numbers must NOT be prefixed
    assert.equal(sanitizeCsvFormulaCell("-42"), "-42");
    assert.equal(sanitizeCsvFormulaCell("-0.0035"), "-0.0035");
    assert.equal(sanitizeCsvFormulaCell("normal-text"), "normal-text");
  });

  it("escapes headers, array items, quotes, and formula cells in generateCsv", () => {
    const csv = generateCsv(
      [
        {
          formula: '=HYPERLINK("http://evil.example","open")',
          delta: "-12.50",
          negExpr: "-1+2",
          tags: ['alpha "1"', "=EVIL()"],
        },
      ],
      [
        { key: "formula", label: 'Formula "Col", 1' },
        { key: "delta", label: "Amount" },
        { key: "negExpr", label: "=BadHeader" },
        { key: "tags", label: "Tags" },
      ],
      { withBOM: false },
    );

    const lines = csv.trimEnd().split("\n");
    assert.equal(lines[0], '"Formula ""Col"", 1",Amount,\'=BadHeader,Tags');
    assert.equal(
      lines[1],
      '"\'=HYPERLINK(""http://evil.example"",""open"")",-12.50,\'-1+2,"alpha ""1""',
    );
    assert.equal(lines[2], '\'=EVIL()"');
  });

  it("sanitizes formula cells and escapes quotes in serializeRawCsv", () => {
    const raw = serializeRawCsv([
      ["Header", '=CMD("x")', -99.25, "-SUM(A1)"],
    ]);
    assert.equal(
      raw,
      '"Header","\'=CMD(""x"")","-99.25","\'-SUM(A1)"',
    );
  });

  it("maps CSV headers to target object keys in parseCsv (including BOM and case differences)", () => {
    interface WhitelistRow extends Record<string, unknown> {
      ipAddress: string;
      cidrMask: string;
      note: string;
    }

    const content = [
      "\uFEFFIP_Address,CIDR,Description",
      '10.0.0.1,24,"Core ""gateway"", prod"',
      "192.168.1.10,32,Edge node",
    ].join("\r\n");

    const parsed = parseCsv<WhitelistRow>(content, {
      ip_address: "ipAddress",
      cidr: "cidrMask",
      description: "note",
    });

    assert.deepEqual(parsed, [
      {
        ipAddress: "10.0.0.1",
        cidrMask: "24",
        note: 'Core "gateway", prod',
      },
      {
        ipAddress: "192.168.1.10",
        cidrMask: "32",
        note: "Edge node",
      },
    ]);
  });
});
