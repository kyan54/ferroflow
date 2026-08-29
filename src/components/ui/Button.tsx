import { forwardRef } from "react";
import type { ButtonHTMLAttributes } from "react";
import { cn } from "../../lib/utils";
import { Spinner } from "./Spinner";

export type ButtonVariant = "default" | "outline" | "destructive" | "ghost" | "secondary";
export type ButtonSize = "default" | "sm" | "lg" | "icon";

const VARIANT_CLASSES: Record<ButtonVariant, string> = {
  default: "bg-flow text-white hover:bg-flow-hi",
  destructive: "bg-err-weak text-err border border-err/30 hover:bg-err/15 hover:border-err/50",
  outline: "border border-hair bg-transparent text-fg-dim hover:bg-surface-2 hover:border-line",
  secondary: "bg-surface-2 text-fg-dim hover:bg-surface-3",
  ghost: "text-fg-dim hover:bg-surface-2 hover:text-fg",
};

const SIZE_CLASSES: Record<ButtonSize, string> = {
  default: "h-[33px] px-4 text-[13px]",
  sm: "h-7 px-3 text-xs",
  lg: "h-[38px] px-6 text-sm",
  icon: "h-[33px] w-[33px] p-0",
};

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  /** Shows a spinner in place of the icon slot and disables the button. */
  busy?: boolean;
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant = "default", size = "default", busy = false, disabled, children, ...props }, ref) => {
    return (
      <button
        ref={ref}
        disabled={disabled || busy}
        className={cn(
          "inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-md border border-transparent font-medium transition-colors",
          "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background",
          "disabled:pointer-events-none disabled:opacity-50",
          VARIANT_CLASSES[variant],
          SIZE_CLASSES[size],
          className,
        )}
        {...props}
      >
        {busy && <Spinner className="h-3.5 w-3.5" />}
        {children}
      </button>
    );
  },
);
Button.displayName = "Button";
