import { useAppStore } from "../store";
import { getDictionary } from "./current";

export type { Dictionary, Language } from "./dictionary";
export { normalizeLanguage, getDictionary, getT, setCurrentLanguage } from "./current";

/** Reactive dictionary lookup for React components -- re-renders when
 * `language` changes in the store. `store.ts` action functions (toasts) use
 * the non-reactive `getT()` from `./current` instead, since they can't call
 * a hook. */
export function useTranslation() {
  const language = useAppStore((s) => s.language);
  return { t: getDictionary(language), language };
}
