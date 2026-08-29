/** Joins class name fragments, dropping falsy values. A minimal stand-in for
 * `clsx` -- ferroflow's kit is small enough not to need the dependency. */
export function cn(...classes: Array<string | false | null | undefined>): string {
  return classes.filter(Boolean).join(" ");
}
