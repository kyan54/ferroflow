import { cn } from "../../lib/utils";

export interface ToggleProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
  label?: string;
  className?: string;
  "aria-label"?: string;
}

/** Styled switch replacing bare `<input type="checkbox">` usages, matching the
 * reference app's switch. Plain button + span, no Radix dependency needed for
 * something this small. */
export function Toggle({ checked, onChange, disabled, label, className, ...aria }: ToggleProps) {
  const button = (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={aria["aria-label"] ?? label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={cn(
        "inline-flex h-5 w-9 shrink-0 cursor-pointer items-center rounded-full transition-colors",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background",
        "disabled:cursor-not-allowed disabled:opacity-50",
        checked ? "bg-flow" : "bg-hair",
        className,
      )}
    >
      <span
        className={cn(
          "block h-4 w-4 rounded-full bg-white shadow transition-transform",
          checked ? "translate-x-4" : "translate-x-0.5",
        )}
      />
    </button>
  );

  if (!label) return button;

  return (
    <label className="flex items-center justify-between gap-3 text-sm text-fg">
      <span>{label}</span>
      {button}
    </label>
  );
}
