import React from "react";
import { useNavigate } from "react-router";
import { ShieldAlert } from "lucide-react";
import { Badge } from "@/components/ui";
import { useT } from "@/locales";
import {
  CERT_LIST_PATH,
  shouldShowCertExpiryBadge,
} from "@/utils/ops/cert-badge";

export interface CertExpiryBadgeProps {
  days?: number | null;
}

export function CertExpiryBadge({ days }: CertExpiryBadgeProps) {
  const { t } = useT();
  const navigate = useNavigate();

  if (!shouldShowCertExpiryBadge(days)) return null;

  const displayDays =
    typeof days === "number" && Number.isFinite(days)
      ? String(Math.floor(days))
      : "—";

  return (
    <button
      type="button"
      onClick={() => void navigate(CERT_LIST_PATH)}
      className="inline-flex items-center p-0 border-0 bg-transparent cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/45 rounded-full"
    >
      <Badge variant="warning" className="gap-1">
        <ShieldAlert className="h-3 w-3 shrink-0" />
        <span>{t("ops.overview.certExpiryBadge", { days: displayDays })}</span>
      </Badge>
    </button>
  );
}
