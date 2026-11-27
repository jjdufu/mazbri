import { useNavigate } from "react-router";
import { FileQuestion, Home, LogOut, ShieldAlert } from "lucide-react";
import { Button, Card } from "@/components/ui";
import { resolveDefaultHomePath } from "@/components/layout/menuConfig";
import { useT } from "@/locales";
import { useAuthStore } from "@/stores/auth";

interface ExceptionViewProps {
  code: "403" | "404";
  messageKey: "exceptionPage.403" | "exceptionPage.404";
}

function ExceptionView({ code, messageKey }: ExceptionViewProps) {
  const { t } = useT();
  const navigate = useNavigate();
  const isLogin = useAuthStore((s) => s.isLogin);
  const role = useAuthStore((s) => s.info.role);
  const logOut = useAuthStore((s) => s.logOut);

  const handleGoHome = () => {
    if (!isLogin) {
      navigate("/auth/login", { replace: true });
      return;
    }
    navigate(resolveDefaultHomePath(role), { replace: true });
  };

  const handleSwitchAccount = async () => {
    await logOut();
    navigate("/auth/login", { replace: true });
  };

  const Icon = code === "403" ? ShieldAlert : FileQuestion;

  return (
    <div
      data-testid={`exception-page-${code}`}
      className="min-h-screen w-full flex items-center justify-center bg-canvas-bg p-6"
    >
      <Card className="w-full max-w-md p-8 text-center space-y-5">
        <div className="mx-auto inline-flex h-12 w-12 items-center justify-center rounded-full bg-xblue-soft text-xblue">
          <Icon className="h-6 w-6" />
        </div>
        <div className="space-y-1.5">
          <div className="text-3xl font-bold font-mono-tech text-foreground">
            {code}
          </div>
          <p className="text-sm text-muted-foreground">{t(messageKey)}</p>
        </div>
        <div className="flex items-center justify-center gap-2 pt-1">
          <Button
            type="button"
            variant="primary"
            onClick={handleGoHome}
            data-testid="exception-go-home"
          >
            <Home className="h-3.5 w-3.5" />
            {t("exceptionPage.gohome")}
          </Button>
          <Button
            type="button"
            variant="outline"
            onClick={() => void handleSwitchAccount()}
            data-testid="exception-switch-account"
          >
            <LogOut className="h-3.5 w-3.5" />
            {t("exceptionPage.switchAccount")}
          </Button>
        </div>
      </Card>
    </div>
  );
}

export function ForbiddenPage() {
  return <ExceptionView code="403" messageKey="exceptionPage.403" />;
}

export function NotFoundPage() {
  return <ExceptionView code="404" messageKey="exceptionPage.404" />;
}
