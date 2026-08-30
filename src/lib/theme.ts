// Manual light/dark theme toggle, layered on top of the "follow the OS"
// behavior this app already had (src/index.css's `prefers-color-scheme`
// media query). "system" (the default) leaves that media query in sole
// control; "light"/"dark" set a `data-theme` attribute on `<html>` that
// src/index.css's `:root[data-theme="dark"]` block (and the media query's
// `:not([data-theme="light"])` guard) key off of. Store-independent, same
// reasoning as `i18n/current.ts`: applying a theme is a DOM side effect,
// not React state, so it doesn't need a hook.

export type Theme = "system" | "light" | "dark";

/** `config.theme` is a free-form `Option<String>` on the wire -- narrows
 * anything else (including `null`/`undefined`) to the default "system". */
export function normalizeTheme(theme: string | null | undefined): Theme {
  return theme === "light" || theme === "dark" ? theme : "system";
}

/** Sets (or clears, for "system") the `data-theme` attribute driving
 * src/index.css's theme blocks. Call on config load and on every
 * `setTheme`. */
export function applyTheme(theme: Theme): void {
  const root = document.documentElement;
  if (theme === "system") {
    root.removeAttribute("data-theme");
  } else {
    root.setAttribute("data-theme", theme);
  }
}
