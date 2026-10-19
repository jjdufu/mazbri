import type { ISODateString } from "./common";

export interface LicenseStatus {
  status: string;
  active: boolean;
  issuanceAllowed: boolean;
  decryptionAllowed: boolean;
  sequence: number | null;
  keyVersion: string | null;
  customerId: string | null;
  installationId: string | null;
  notBefore: ISODateString | null;
  expiresAt: ISODateString | null;
  graceUntil: ISODateString | null;
  secondsToExpiry: number | null;
  secondsToGraceEnd: number | null;
}
