/** Joins class name fragments, dropping falsy values. A minimal stand-in for
 * `clsx` -- ferroflow's kit is small enough not to need the dependency. */
export function cn(...classes: Array<string | false | null | undefined>): string {
  return classes.filter(Boolean).join(" ");
}

/** Generates a client-side id for a freshly created object (new server,
 * duplicated server, ...) before it's ever sent to the backend -- prefers
 * `crypto.randomUUID()`, falling back to a timestamp+random string on
 * runtimes where it's unavailable. Shared by `ServerForm` (new servers) and
 * `store.ts`'s `duplicateServer` (cloned servers) so both use the same
 * convention instead of two hand-rolled copies. */
export function newId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `srv-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

/** Human-readable byte count (e.g. "12.4 MB"). Shared by ConnectionsView's
 * active/history tables and the Dashboard's bottom status bar -- kept here
 * so both use identical formatting instead of two hand-rolled copies. */
export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = n / 1024;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  return `${value.toFixed(1)} ${units[unitIndex]}`;
}
