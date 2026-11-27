export class MfaStepUpCancelledError extends Error {
  readonly cancelled = true;

  constructor() {
    super("MFA step-up cancelled");
    this.name = "MfaStepUpCancelledError";
  }
}

export function isMfaStepUpCancelled(error: unknown): boolean {
  return error instanceof MfaStepUpCancelledError;
}
