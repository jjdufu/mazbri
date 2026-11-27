import { type FormEvent, useEffect, useState } from "react";
import { Lock, Unlock } from "lucide-react";
import {
  Button,
  Card,
  FormField,
  Modal,
  PasswordInput,
  cn,
} from "@/components/ui";
import { LanguageEnum } from "@/enums/appEnum";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";
import { useLockScreenStore } from "@/stores/lockscreen";
import { useMfaStepUpStore } from "@/stores/mfa-step-up";
import { showErrorToast } from "@/utils/ui/message-guard";

export function LockScreen() {
  const { t, locale } = useT();
  const username = useAuthStore(
    (s) => s.info.username || s.info.userName || "User",
  );
  const logOut = useAuthStore((s) => s.logOut);
  const isLocked = useLockScreenStore((s) => s.isLocked);
  const lockDialogOpen = useLockScreenStore((s) => s.lockDialogOpen);
  const closeLockDialog = useLockScreenStore((s) => s.closeLockDialog);
  const lock = useLockScreenStore((s) => s.lock);
  const unlock = useLockScreenStore((s) => s.unlock);

  const [lockPassword, setLockPassword] = useState("");
  const [lockError, setLockError] = useState<string | undefined>(undefined);
  const [locking, setLocking] = useState(false);

  const [unlockPassword, setUnlockPassword] = useState("");
  const [unlockError, setUnlockError] = useState<string | undefined>(undefined);
  const [unlocking, setUnlocking] = useState(false);
  const [shaking, setShaking] = useState(false);

  useEffect(() => {
    if (!lockDialogOpen) {
      setLockPassword("");
      setLockError(undefined);
    }
  }, [lockDialogOpen]);

  useEffect(() => {
    if (typeof document === "undefined") return;
    if (!isLocked) {
      setUnlockPassword("");
      setUnlockError(undefined);
      return;
    }
    const prevOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = prevOverflow;
    };
  }, [isLocked]);

  const handleLockSubmit = async (event?: FormEvent) => {
    event?.preventDefault();
    if (!lockPassword.trim()) {
      setLockError(t("lockScreen.lock.inputPlaceholder"));
      return;
    }
    setLocking(true);
    setLockError(undefined);
    try {
      await lock(lockPassword);
      setLockPassword("");
      useMfaStepUpStore
        .getState()
        .lock()
        .catch(() => undefined);
    } catch {
      showErrorToast(t("lockScreen.initFailed"));
    } finally {
      setLocking(false);
    }
  };

  const triggerShake = () => {
    setShaking(false);
    requestAnimationFrame(() => {
      setShaking(true);
      setTimeout(() => setShaking(false), 500);
    });
  };

  const handleUnlockSubmit = async (event?: FormEvent) => {
    event?.preventDefault();
    if (!unlockPassword.trim()) {
      setUnlockError(t("lockScreen.unlock.inputPlaceholder"));
      return;
    }
    setUnlocking(true);
    setUnlockError(undefined);
    try {
      const matched = await unlock(unlockPassword);
      if (matched) {
        setUnlockPassword("");
        return;
      }
      setUnlockPassword("");
      setUnlockError(t("lockScreen.pwdError"));
      showErrorToast(t("lockScreen.pwdError"));
      triggerShake();
    } finally {
      setUnlocking(false);
    }
  };

  return (
    <>
      <Modal
        open={lockDialogOpen && !isLocked}
        onClose={closeLockDialog}
        size="sm"
        testId="lock-screen-modal"
        title={t("lockScreen.lock.btnText")}
        footer={
          <>
            <Button
              type="button"
              variant="outline"
              onClick={closeLockDialog}
              disabled={locking}
            >
              {t("common.cancel")}
            </Button>
            <Button
              type="button"
              variant="primary"
              loading={locking}
              onClick={() => void handleLockSubmit()}
              data-testid="lock-screen-confirm-btn"
            >
              {t("lockScreen.lock.btnText")}
            </Button>
          </>
        }
      >
        <form
          onSubmit={(e) => void handleLockSubmit(e)}
          className="space-y-4"
        >
          <div className="flex items-center gap-2.5">
            <span className="inline-flex h-8 w-8 items-center justify-center rounded-full bg-xblue-soft text-xblue font-semibold text-xs shrink-0">
              {username.slice(0, 1).toUpperCase()}
            </span>
            <span className="text-sm font-medium text-foreground truncate">
              {username}
            </span>
          </div>
          <input
            type="text"
            name="username"
            value={username}
            autoComplete="username"
            readOnly
            tabIndex={-1}
            aria-hidden="true"
            className="hidden"
          />
          <FormField
            label={locale === LanguageEnum.ZH ? "锁屏密码" : "Lock password"}
            htmlFor="lock-screen-password-input"
            error={lockError}
            required
          >
            <PasswordInput
              id="lock-screen-password-input"
              name="lock_password"
              value={lockPassword}
              autoFocus
              autoComplete="new-password"
              placeholder={t("lockScreen.lock.inputPlaceholder")}
              data-testid="lock-screen-password-input"
              onChange={(e) => {
                setLockPassword(e.target.value);
                if (lockError) setLockError(undefined);
              }}
            />
          </FormField>
        </form>
      </Modal>

      {isLocked && (
        <div
          data-testid="lock-screen-overlay"
          role="dialog"
          aria-modal="true"
          className="fixed inset-0 z-[70] flex items-center justify-center bg-canvas-bg/95 backdrop-blur-md p-4"
        >
          <Card
            className={cn(
              "w-full max-w-sm p-6 space-y-5 shadow-xl",
              shaking && "mazbri-shake",
            )}
          >
            <div className="flex flex-col items-center gap-2 text-center">
              <div className="inline-flex h-11 w-11 items-center justify-center rounded-full bg-xblue-soft text-xblue">
                <Lock className="h-5 w-5" />
              </div>
              <div className="text-base font-semibold text-foreground">
                {username}
              </div>
            </div>

            <form
              onSubmit={(e) => void handleUnlockSubmit(e)}
              className="space-y-4"
            >
              <input
                type="text"
                name="username"
                value={username}
                autoComplete="username"
                readOnly
                tabIndex={-1}
                aria-hidden="true"
                className="hidden"
              />
              <FormField
                label={
                  locale === LanguageEnum.ZH ? "解锁密码" : "Unlock password"
                }
                htmlFor="lock-screen-unlock-input"
                error={unlockError}
              >
                <PasswordInput
                  id="lock-screen-unlock-input"
                  name="unlock_password"
                  value={unlockPassword}
                  autoFocus
                  autoComplete="new-password"
                  placeholder={t("lockScreen.unlock.inputPlaceholder")}
                  data-testid="lock-screen-unlock-input"
                  onChange={(e) => {
                    setUnlockPassword(e.target.value);
                    if (unlockError) setUnlockError(undefined);
                  }}
                />
              </FormField>

              <Button
                type="submit"
                variant="primary"
                loading={unlocking}
                data-testid="lock-screen-unlock-btn"
                className="w-full"
              >
                <Unlock className="h-3.5 w-3.5" />
                {t("lockScreen.unlock.btnText")}
              </Button>

              <div className="text-center">
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  data-testid="lock-screen-logout-btn"
                  onClick={() => void logOut()}
                >
                  {t("lockScreen.unlock.backBtnText")}
                </Button>
              </div>
            </form>
          </Card>
        </div>
      )}
    </>
  );
}
