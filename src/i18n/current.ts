// Store-independent language state, used by `store.ts` action functions
// (toast messages) that aren't React components and can't call a hook.
// Deliberately has no dependency on `store.ts` -- `store.ts` imports from
// this module, and `index.ts`'s `useTranslation` hook imports from
// `store.ts`, so keeping this file free of a `store.ts` import avoids a
// runtime circular import between the two.

import type { Dictionary, Language } from "./dictionary";
import { en } from "./en";
import { zh } from "./zh";

export const dictionaries: Record<Language, Dictionary> = { en, zh };

/** `config.language` is a free-form `Option<String>` on the wire -- narrows
 * anything else (including `null`/`undefined`) to the default "en". */
export function normalizeLanguage(language: string | null | undefined): Language {
  return language === "zh" ? "zh" : "en";
}

export function getDictionary(language: Language): Dictionary {
  return dictionaries[language] ?? en;
}

let currentLanguage: Language = "en";

/** Called by `store.ts` whenever `language` state changes (on config load
 * and on `setLanguage`), so `getT()` below reflects it without `store.ts`
 * needing to import the React hook module. */
export function setCurrentLanguage(language: Language): void {
  currentLanguage = language;
}

/** Non-reactive dictionary lookup for use outside React (store.ts action
 * functions building toast messages). Components should use `useTranslation`
 * from `./index` instead, so they re-render on language change. */
export function getT(): Dictionary {
  return getDictionary(currentLanguage);
}
