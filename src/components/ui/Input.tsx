import { forwardRef } from "react";
import type { InputHTMLAttributes, TextareaHTMLAttributes, SelectHTMLAttributes } from "react";
import { cn } from "../../lib/utils";

const FIELD_CLASSES =
  "w-full rounded-md border border-line bg-surface-2 px-3 py-1.5 text-sm text-fg placeholder:text-fg-faint " +
  "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:border-flow " +
  "disabled:cursor-not-allowed disabled:opacity-50";

export const Input = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement>>(
  ({ className, ...props }, ref) => (
    <input ref={ref} className={cn(FIELD_CLASSES, "h-9", className)} {...props} />
  ),
);
Input.displayName = "Input";

export const Textarea = forwardRef<HTMLTextAreaElement, TextareaHTMLAttributes<HTMLTextAreaElement>>(
  ({ className, ...props }, ref) => (
    <textarea ref={ref} className={cn(FIELD_CLASSES, "leading-relaxed", className)} {...props} />
  ),
);
Textarea.displayName = "Textarea";

export const Select = forwardRef<HTMLSelectElement, SelectHTMLAttributes<HTMLSelectElement>>(
  ({ className, children, ...props }, ref) => (
    <select ref={ref} className={cn(FIELD_CLASSES, "h-9", className)} {...props}>
      {children}
    </select>
  ),
);
Select.displayName = "Select";
