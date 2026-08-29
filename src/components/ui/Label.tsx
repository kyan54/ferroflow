import type { LabelHTMLAttributes } from "react";
import { cn } from "../../lib/utils";

export function Label({ className, ...props }: LabelHTMLAttributes<HTMLLabelElement>) {
  return (
    <label
      className={cn("flex flex-col gap-1 text-sm font-medium text-fg-dim", className)}
      {...props}
    />
  );
}
