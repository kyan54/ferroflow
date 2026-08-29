import type { HTMLAttributes } from "react";
import { cn } from "../../lib/utils";

export type BadgeVariant =
  | "default"
  | "secondary"
  | "destructive"
  | "success"
  | "warning"
  | "outline"
  // Protocol tags (Servers list) -- a small trimmed set of the reference
  // app's badge-hue palette, one per protocol.
  | "vless"
  | "vmess"
  | "trojan"
  | "shadowsocks"
  | "wireguard";

const VARIANT_CLASSES: Record<BadgeVariant, string> = {
  default: "border-transparent bg-flow-weak text-flow-hi",
  secondary: "border-transparent bg-surface-3 text-fg-dim",
  destructive: "border-transparent bg-err-weak text-err",
  success: "border-transparent bg-ok-weak text-ok",
  warning: "border-transparent bg-warn-weak text-warn",
  outline: "bg-transparent border-hair text-fg-faint",
  vless: "border-transparent bg-badge-blue/15 text-badge-blue",
  vmess: "border-transparent bg-badge-purple/15 text-badge-purple",
  trojan: "border-transparent bg-badge-orange/15 text-badge-orange",
  shadowsocks: "border-transparent bg-badge-teal/15 text-badge-teal",
  wireguard: "border-transparent bg-badge-indigo/15 text-badge-indigo",
};

export interface BadgeProps extends HTMLAttributes<HTMLSpanElement> {
  variant?: BadgeVariant;
}

export function Badge({ className, variant = "default", ...props }: BadgeProps) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1 whitespace-nowrap rounded-md border px-2 py-0.5 text-[11px] font-semibold uppercase tracking-wide",
        VARIANT_CLASSES[variant],
        className,
      )}
      {...props}
    />
  );
}
