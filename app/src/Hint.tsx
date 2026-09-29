import { useId, useState, type ReactNode, type SyntheticEvent } from "react";
import type { HintText } from "./hints";

/** Largeur de la bulle, identique à `.hint-bubble` dans styles.css. */
const BUBBLE_WIDTH = 320;
const EDGE_MARGIN = 16;

interface Props {
  hint: HintText;
  children: ReactNode;
}

/**
 * Info-bulle affichée au survol et au focus clavier (Tab). Le texte est relié au déclencheur
 * par aria-describedby : un lecteur d'écran le lit même quand la bulle est masquée.
 * À l'ouverture, la bulle s'aligne à droite du déclencheur si elle dépasserait la fenêtre.
 * Échap retire le focus pour la fermer.
 */
export function Hint({ hint, children }: Props) {
  const id = useId();
  const [align, setAlign] = useState<"start" | "end">("start");

  const place = (e: SyntheticEvent<HTMLElement>) => {
    const left = e.currentTarget.getBoundingClientRect().left;
    const fits = left + BUBBLE_WIDTH + EDGE_MARGIN <= document.documentElement.clientWidth;
    setAlign(fits ? "start" : "end");
  };

  return (
    <span
      className="hint"
      tabIndex={0}
      aria-describedby={id}
      onMouseEnter={place}
      onFocus={place}
      onKeyDown={(e) => {
        if (e.key === "Escape") e.currentTarget.blur();
      }}
    >
      {children}
      <span role="tooltip" id={id} className={`hint-bubble hint-${align}`}>
        <span className="hint-box">
          <strong>{hint.title}</strong>
          {hint.body.map((p) => (
            <span key={p} className="hint-p">
              {p}
            </span>
          ))}
          {hint.here && <span className="hint-here">{hint.here}</span>}
        </span>
      </span>
    </span>
  );
}
