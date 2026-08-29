import { cn } from "../../lib/utils";

/** Small inline spinner used for consistent busy-state treatment across
 * buttons and inline "loading…" placeholders (instead of plain disabled
 * buttons with no visual feedback). */
export function Spinner({ className }: { className?: string }) {
  return (
    <svg
      className={cn("animate-spin text-current", className)}
      viewBox="0 0 24 24"
      fill="none"
      aria-hidden="true"
    >
      <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
      <path
        className="opacity-75"
        fill="currentColor"
        d="M4 12a8 8 0 018-8v3a5 5 0 00-5 5H4z"
      />
    </svg>
  );
}
