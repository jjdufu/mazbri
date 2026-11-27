import React from "react";
import {
  Button,
  Tooltip,
  type ButtonProps,
  type TooltipProps,
} from "./primitives";

export interface AuthorizationTooltipButtonProps extends ButtonProps {
  disabledReason?: React.ReactNode;
  tooltipSide?: TooltipProps["side"];
}

export function AuthorizationTooltipButton({
  disabled,
  disabledReason,
  tooltipSide = "top",
  ...buttonProps
}: AuthorizationTooltipButtonProps) {
  const showTooltip = Boolean(disabled && disabledReason);
  const btn = <Button disabled={disabled} {...buttonProps} />;

  if (!showTooltip) {
    return btn;
  }

  return (
    <Tooltip content={disabledReason} side={tooltipSide}>
      {btn}
    </Tooltip>
  );
}
