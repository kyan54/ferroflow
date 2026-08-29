import { cn } from "../../lib/utils";

export interface SegmentedOption<T extends string> {
  value: T;
  label: string;
  title?: string;
  disabled?: boolean;
}

export interface SegmentedControlProps<T extends string> {
  options: SegmentedOption<T>[];
  value: T;
  onChange: (value: T) => void;
  disabled?: boolean;
  className?: string;
  "aria-label"?: string;
}

/** Horizontal N-way segmented control -- a tab group styled as a single
 * bordered row of equal-width buttons, the active one filled with the flow
 * (teal) color. Ported from the FlowZ Electron reference app's own
 * `SegmentedControl` (same Conduit design tokens), used on the Dashboard for
 * the takeover-mode and routing-strategy pickers in place of a `<select>`. */
export function SegmentedControl<T extends string>({
  options,
  value,
  onChange,
  disabled,
  className,
  ...aria
}: SegmentedControlProps<T>) {
  return (
    <div
      role="radiogroup"
      aria-label={aria["aria-label"]}
      className={cn(
        "inline-flex w-full gap-[3px] rounded-lg border border-line bg-surface-2 p-[3px]",
        className,
      )}
    >
      {options.map((opt) => {
        const active = opt.value === value;
        return (
          <button
            key={opt.value}
            type="button"
            role="radio"
            aria-checked={active}
            title={opt.title}
            disabled={disabled || opt.disabled}
            onClick={() => !active && onChange(opt.value)}
            className={cn(
              "flex-1 whitespace-nowrap rounded-md px-2 py-1.5 text-xs font-semibold text-fg-dim transition-colors",
              "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
              "disabled:pointer-events-none disabled:opacity-50",
              active ? "bg-surface text-flow-hi shadow-sm" : "hover:text-fg",
            )}
          >
            {opt.label}
          </button>
        );
      })}
    </div>
  );
}
