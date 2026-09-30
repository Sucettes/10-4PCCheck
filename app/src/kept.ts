import { useCallback, useState } from "react";

/** Valeurs gardées au niveau du module, par clé : elles survivent au démontage de l'écran. */
const kept = new Map<string, unknown>();

/**
 * Comme `useState`, mais la valeur est retrouvée quand l'écran est rouvert. Le module est écrit
 * AVANT React : une opération asynchrone qui se termine après le démontage (copie en cours)
 * met quand même à jour la valeur gardée, alors que React n'appellerait plus la fonction de
 * mise à jour d'un composant démonté.
 */
export function useKept<T>(key: string, init: () => T): [T, (update: T | ((prev: T) => T)) => void] {
  const [value, setValue] = useState<T>(() => {
    if (!kept.has(key)) kept.set(key, init());
    return kept.get(key) as T;
  });
  const set = useCallback(
    (update: T | ((prev: T) => T)) => {
      const prev = kept.get(key) as T;
      const next = typeof update === "function" ? (update as (p: T) => T)(prev) : update;
      kept.set(key, next);
      setValue(next);
    },
    [key],
  );
  return [value, set];
}
