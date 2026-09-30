// Tests interactifs de l'analyse complète (plan §5) : clavier, écran, webcam, micro, haut-parleurs,
// pavé tactile. Tout se passe dans la vue web ; l'utilisateur confirme ce qu'il voit ou entend.
import { useEffect, useRef, useState, type ReactNode } from "react";

export type TestStatus = "pass" | "fail" | "skipped";

export interface InteractiveResult {
  id: string;
  label: string;
  status: TestStatus;
  note: string | null;
}

/** Résultats gardés entre deux passages sur l'écran, repris dans le rapport. */
const results = new Map<string, InteractiveResult>();
const listeners = new Set<() => void>();

export function interactiveResults(): InteractiveResult[] {
  return [...results.values()];
}

function record(r: InteractiveResult) {
  results.set(r.id, r);
  listeners.forEach((l) => l());
}

/** Résultats des tests interactifs, avec rendu à chaque changement. */
export function useResults(): Map<string, InteractiveResult> {
  const [, force] = useState(0);
  useEffect(() => {
    const l = () => force((n) => n + 1);
    listeners.add(l);
    return () => void listeners.delete(l);
  }, []);
  return results;
}

const TESTS = [
  { id: "keyboard", label: "Clavier" },
  { id: "screen", label: "Pixels morts" },
  { id: "webcam", label: "Webcam" },
  { id: "mic", label: "Micro" },
  { id: "speakers", label: "Haut-parleurs G / D" },
  { id: "touchpad", label: "Pavé tactile" },
] as const;

type TestId = (typeof TESTS)[number]["id"];

/** Liste des tests avec leur état ; un clic ouvre le test en superposition. */
export function InteractiveTests() {
  const res = useResults();
  const [open, setOpen] = useState<TestId | null>(null);
  const close = () => setOpen(null);
  return (
    <aside className="panel tests-panel" aria-label="Tests interactifs">
      <h3>Tests interactifs</h3>
      <ul className="itests">
        {TESTS.map((t) => {
          const r = res.get(t.id);
          return (
            <li key={t.id}>
              <button type="button" className="itest-btn" onClick={() => setOpen(t.id)}>
                <span>{t.label}</span>
                <ResultTag r={r} />
              </button>
            </li>
          );
        })}
      </ul>
      <p className="muted small">Test RAM complet : au démarrage, sur la clé bootable (MemTest86+).</p>
      {open === "keyboard" && <KeyboardTest onClose={close} />}
      {open === "screen" && <ScreenTest onClose={close} />}
      {open === "webcam" && <WebcamTest onClose={close} />}
      {open === "mic" && <MicTest onClose={close} />}
      {open === "speakers" && <SpeakerTest onClose={close} />}
      {open === "touchpad" && <TouchpadTest onClose={close} />}
    </aside>
  );
}

function ResultTag({ r }: { r: InteractiveResult | undefined }) {
  if (!r) return <span className="muted small">À faire</span>;
  if (r.status === "pass") return <span className="status status-good">Réussi</span>;
  if (r.status === "skipped") return <span className="status status-neutral">Passé</span>;
  return <span className="status status-bad" title={r.note ?? undefined}>{r.note ?? "Échec"}</span>;
}

function Overlay({ title, children, onClose }: { title: string; children: ReactNode; onClose: () => void }) {
  return (
    <div className="overlay" role="dialog" aria-modal="true" aria-label={title}>
      <div className="overlay-box">
        <div className="panel-head">
          <h3>{title}</h3>
          <button type="button" className="btn" onClick={onClose}>
            Fermer
          </button>
        </div>
        {children}
      </div>
    </div>
  );
}

function Verdict({
  id,
  label,
  question,
  onClose,
  failNote,
}: {
  id: TestId;
  label: string;
  question: string;
  onClose: () => void;
  failNote: string;
}) {
  const done = (status: TestStatus, note: string | null) => {
    record({ id, label, status, note });
    onClose();
  };
  return (
    <div className="itest-verdict">
      <span>{question}</span>
      <div className="header-actions">
        <button type="button" className="btn btn-primary" onClick={() => done("pass", null)}>
          Oui, ça fonctionne
        </button>
        <button type="button" className="btn" onClick={() => done("fail", failNote)}>
          Non
        </button>
        <button type="button" className="btn" onClick={() => done("skipped", null)}>
          Passer
        </button>
      </div>
    </div>
  );
}

// ---------- Clavier ----------

/** Disposition ISO sans pavé numérique, par code physique (indépendant de la langue du clavier). */
const ROWS: string[][] = [
  ["Escape", "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12", "PrintScreen", "ScrollLock", "Pause"],
  ["Backquote", "Digit1", "Digit2", "Digit3", "Digit4", "Digit5", "Digit6", "Digit7", "Digit8", "Digit9", "Digit0", "Minus", "Equal", "Backspace", "Insert", "Home", "PageUp"],
  ["Tab", "KeyQ", "KeyW", "KeyE", "KeyR", "KeyT", "KeyY", "KeyU", "KeyI", "KeyO", "KeyP", "BracketLeft", "BracketRight", "Enter", "Delete", "End", "PageDown"],
  ["CapsLock", "KeyA", "KeyS", "KeyD", "KeyF", "KeyG", "KeyH", "KeyJ", "KeyK", "KeyL", "Semicolon", "Quote", "Backslash"],
  ["ShiftLeft", "IntlBackslash", "KeyZ", "KeyX", "KeyC", "KeyV", "KeyB", "KeyN", "KeyM", "Comma", "Period", "Slash", "ShiftRight", "ArrowUp"],
  ["ControlLeft", "MetaLeft", "AltLeft", "Space", "AltRight", "ContextMenu", "ControlRight", "ArrowLeft", "ArrowDown", "ArrowRight"],
];

const NAMES: Record<string, string> = {
  Escape: "Échap", Backquote: "`", Minus: "-", Equal: "=", Backspace: "⌫", Insert: "Inser", Home: "Début",
  PageUp: "Pg↑", Tab: "Tab", BracketLeft: "[", BracketRight: "]", Enter: "Entrée", Delete: "Suppr", End: "Fin",
  PageDown: "Pg↓", CapsLock: "Maj verr", Semicolon: ";", Quote: "'", Backslash: "\\", ShiftLeft: "Maj",
  IntlBackslash: "<", Comma: ",", Period: ".", Slash: "/", ShiftRight: "Maj", ArrowUp: "↑", ControlLeft: "Ctrl",
  MetaLeft: "Win", AltLeft: "Alt", Space: "Espace", AltRight: "Alt Gr", ContextMenu: "Menu", ControlRight: "Ctrl",
  ArrowLeft: "←", ArrowDown: "↓", ArrowRight: "→", PrintScreen: "Impr", ScrollLock: "Arrêt défil", Pause: "Pause",
};

const WIDE: Record<string, number> = {
  Backspace: 2, Tab: 1.5, Enter: 1.5, CapsLock: 1.8, ShiftLeft: 1.3, ShiftRight: 2.2, Space: 6, ControlLeft: 1.3,
  ControlRight: 1.3, AltLeft: 1.2, AltRight: 1.2,
};

type LayoutMap = { get(code: string): string | undefined };

function KeyboardTest({ onClose }: { onClose: () => void }) {
  const [pressed, setPressed] = useState<Set<string>>(new Set());
  const [down, setDown] = useState<Set<string>>(new Set());
  const [layout, setLayout] = useState<LayoutMap | null>(null);

  useEffect(() => {
    // Étiquettes réelles du clavier branché (Chromium/WebView2) ; sinon étiquettes génériques.
    const kb = (navigator as unknown as { keyboard?: { getLayoutMap(): Promise<LayoutMap> } }).keyboard;
    kb?.getLayoutMap()
      .then(setLayout)
      .catch(() => {});
    const onDown = (e: KeyboardEvent) => {
      // Tout est intercepté : F5 rechargerait la page, Tab déplacerait le focus, Alt ouvrirait un menu.
      e.preventDefault();
      setPressed((p) => new Set(p).add(e.code));
      setDown((d) => new Set(d).add(e.code));
    };
    const onUp = (e: KeyboardEvent) => {
      e.preventDefault();
      // Impr écran n'envoie souvent que le relâchement sous Windows.
      setPressed((p) => new Set(p).add(e.code));
      setDown((d) => {
        const n = new Set(d);
        n.delete(e.code);
        return n;
      });
    };
    window.addEventListener("keydown", onDown, true);
    window.addEventListener("keyup", onUp, true);
    return () => {
      window.removeEventListener("keydown", onDown, true);
      window.removeEventListener("keyup", onUp, true);
    };
  }, []);

  const label = (code: string) => NAMES[code] ?? layout?.get(code)?.toUpperCase() ?? code.replace(/^(Key|Digit)/, "");
  const all = ROWS.flat();
  const missing = all.filter((c) => !pressed.has(c));

  const finish = () => {
    record({
      id: "keyboard",
      label: "Clavier",
      status: missing.length === 0 ? "pass" : "fail",
      note:
        missing.length === 0
          ? null
          : missing.length <= 6
            ? `${missing.map(label).join(", ")} sans réponse`
            : `${missing.length} touches sans réponse (dont ${missing.slice(0, 4).map(label).join(", ")}…)`,
    });
    onClose();
  };

  return (
    <Overlay title="Test du clavier" onClose={onClose}>
      <p className="muted small">
        Appuie sur chaque touche. Les touches testées passent au vert. Certaines touches (Fn, touches multimédia) ne
        remontent aucun évènement : c'est normal. Si le portable n'a pas une touche, ignore-la.
      </p>
      <div className="kb" aria-label="Clavier">
        {ROWS.map((row, i) => (
          <div className="kb-row" key={i}>
            {row.map((code) => (
              <span
                key={code}
                className={`kb-key${pressed.has(code) ? " kb-ok" : ""}${down.has(code) ? " kb-down" : ""}`}
                style={{ flexGrow: WIDE[code] ?? 1 }}
              >
                {label(code)}
              </span>
            ))}
          </div>
        ))}
      </div>
      <div className="itest-verdict">
        <span>
          {pressed.size} touche(s) testée(s) sur {all.length}.{" "}
          {missing.length > 0 && missing.length <= 8 && `Reste : ${missing.map(label).join(", ")}.`}
        </span>
        <button type="button" className="btn btn-primary" onClick={finish}>
          Terminer
        </button>
      </div>
    </Overlay>
  );
}

// ---------- Écran ----------

const COLORS = ["#000000", "#ffffff", "#ff0000", "#00ff00", "#0000ff", "#808080"];

function ScreenTest({ onClose }: { onClose: () => void }) {
  const [i, setI] = useState<number | null>(null);
  const ref = useRef<HTMLDivElement>(null);

  const start = () => {
    setI(0);
    void ref.current?.requestFullscreen?.().catch(() => {});
  };
  const next = () => {
    if (i === null) return;
    if (i + 1 < COLORS.length) setI(i + 1);
    else {
      setI(null);
      if (document.fullscreenElement) void document.exitFullscreen();
    }
  };

  return (
    <Overlay title="Pixels morts" onClose={onClose}>
      <p className="muted small">
        L'écran passe en plein écran avec 6 couleurs unies. Regarde de près : un point toujours noir, toujours allumé ou
        d'une autre couleur est un pixel mort. Clique ou appuie sur Espace pour passer à la couleur suivante.
      </p>
      <button type="button" className="btn btn-primary" onClick={start}>
        Lancer le test
      </button>
      <div
        ref={ref}
        className={i === null ? "screen-test hidden" : "screen-test"}
        style={{ background: i === null ? undefined : COLORS[i] }}
        onClick={next}
        onKeyDown={(e) => e.key === " " && next()}
        tabIndex={-1}
      />
      <Verdict id="screen" label="Pixels morts" question="Écran sans pixel mort ni tache ?" failNote="Pixels morts ou taches" onClose={onClose} />
    </Overlay>
  );
}

// ---------- Webcam et micro ----------

function WebcamTest({ onClose }: { onClose: () => void }) {
  const video = useRef<HTMLVideoElement>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let stream: MediaStream | null = null;
    navigator.mediaDevices
      ?.getUserMedia({ video: true })
      .then((s) => {
        stream = s;
        if (video.current) video.current.srcObject = s;
      })
      .catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)));
    return () => stream?.getTracks().forEach((t) => t.stop());
  }, []);
  return (
    <Overlay title="Webcam" onClose={onClose}>
      {error ? (
        <p className="text-bad small">
          Caméra inaccessible ({error}). Vérifie avec l'application Caméra de Windows, puis réponds ci-dessous.
        </p>
      ) : (
        <video ref={video} autoPlay playsInline muted className="webcam" />
      )}
      <Verdict id="webcam" label="Webcam" question="Image nette et sans défaut ?" failNote="Webcam défectueuse" onClose={onClose} />
    </Overlay>
  );
}

function MicTest({ onClose }: { onClose: () => void }) {
  const [level, setLevel] = useState(0);
  const [peak, setPeak] = useState(0);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let stream: MediaStream | null = null;
    let ctx: AudioContext | null = null;
    let frame = 0;
    navigator.mediaDevices
      ?.getUserMedia({ audio: true })
      .then((s) => {
        stream = s;
        ctx = new AudioContext();
        const analyser = ctx.createAnalyser();
        analyser.fftSize = 1024;
        ctx.createMediaStreamSource(s).connect(analyser);
        const data = new Float32Array(analyser.fftSize);
        const tick = () => {
          analyser.getFloatTimeDomainData(data);
          // Niveau RMS ramené sur 0..1 (échelle ×4 pour une voix normale).
          const rms = Math.sqrt(data.reduce((a, v) => a + v * v, 0) / data.length);
          const l = Math.min(1, rms * 4);
          setLevel(l);
          setPeak((p) => Math.max(p, l));
          frame = requestAnimationFrame(tick);
        };
        tick();
      })
      .catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)));
    return () => {
      cancelAnimationFrame(frame);
      stream?.getTracks().forEach((t) => t.stop());
      void ctx?.close();
    };
  }, []);
  return (
    <Overlay title="Micro" onClose={onClose}>
      {error ? (
        <p className="text-bad small">Micro inaccessible ({error}).</p>
      ) : (
        <>
          <p className="muted small">Parle normalement ou claque des doigts près de l'ordinateur : la barre doit bouger.</p>
          <div className="bar bar-tall" aria-hidden="true">
            <div className="bar-fill" style={{ width: `${level * 100}%`, transition: "none" }} />
          </div>
          <p className="small">Niveau maximal atteint : {Math.round(peak * 100)} %</p>
        </>
      )}
      <Verdict id="mic" label="Micro" question="La barre réagit à ta voix ?" failNote="Micro muet" onClose={onClose} />
    </Overlay>
  );
}

// ---------- Haut-parleurs ----------

function beep(pan: -1 | 1) {
  const ctx = new AudioContext();
  const osc = ctx.createOscillator();
  const gain = ctx.createGain();
  const panner = ctx.createStereoPanner();
  osc.frequency.value = 660;
  gain.gain.value = 0.2;
  panner.pan.value = pan;
  osc.connect(gain).connect(panner).connect(ctx.destination);
  osc.start();
  osc.stop(ctx.currentTime + 1);
  osc.onended = () => void ctx.close();
}

function SpeakerTest({ onClose }: { onClose: () => void }) {
  const [left, setLeft] = useState<boolean | null>(null);
  const [right, setRight] = useState<boolean | null>(null);
  const finish = () => {
    const bad = [left === false && "gauche", right === false && "droit"].filter(Boolean);
    record({
      id: "speakers",
      label: "Haut-parleurs G / D",
      status: bad.length === 0 ? "pass" : "fail",
      note: bad.length === 0 ? null : `Haut-parleur ${bad.join(" et ")} muet`,
    });
    onClose();
  };
  const Side = ({ name, pan, value, set }: { name: string; pan: -1 | 1; value: boolean | null; set: (v: boolean) => void }) => (
    <div className="speaker-side">
      <button type="button" className="btn" onClick={() => beep(pan)}>
        Jouer à {name}
      </button>
      <label>
        <input type="checkbox" checked={value === true} onChange={(e) => set(e.target.checked)} /> Entendu à {name}
      </label>
    </div>
  );
  return (
    <Overlay title="Haut-parleurs" onClose={onClose}>
      <p className="muted small">Monte le volume. Chaque bouton joue un son d'une seconde d'un seul côté.</p>
      <div className="speakers">
        <Side name="gauche" pan={-1} value={left} set={setLeft} />
        <Side name="droite" pan={1} value={right} set={setRight} />
      </div>
      <div className="itest-verdict">
        <span>Coche chaque côté entendu, puis termine.</span>
        <button
          type="button"
          className="btn btn-primary"
          onClick={() => {
            if (left === null) setLeft(false);
            if (right === null) setRight(false);
            finish();
          }}
        >
          Terminer
        </button>
      </div>
    </Overlay>
  );
}

// ---------- Pavé tactile ----------

function TouchpadTest({ onClose }: { onClose: () => void }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const last = useRef<{ x: number; y: number } | null>(null);
  const draw = (e: React.PointerEvent<HTMLCanvasElement>) => {
    const c = canvas.current;
    const ctx = c?.getContext("2d");
    if (!c || !ctx) return;
    const r = c.getBoundingClientRect();
    const p = { x: ((e.clientX - r.left) / r.width) * c.width, y: ((e.clientY - r.top) / r.height) * c.height };
    if (last.current && e.buttons === 1) {
      ctx.strokeStyle = "#2459d6";
      ctx.lineWidth = 3;
      ctx.beginPath();
      ctx.moveTo(last.current.x, last.current.y);
      ctx.lineTo(p.x, p.y);
      ctx.stroke();
    }
    last.current = p;
  };
  return (
    <Overlay title="Pavé tactile" onClose={onClose}>
      <p className="muted small">
        Dessine dans le cadre en gardant le clic enfoncé, jusque dans les coins. Essaie aussi le clic droit et le défilement à deux doigts.
      </p>
      <canvas ref={canvas} width={800} height={300} className="touch-canvas" onPointerMove={draw} onPointerDown={draw} />
      <Verdict id="touchpad" label="Pavé tactile" question="Le trait suit bien le doigt partout ?" failNote="Pavé tactile défaillant" onClose={onClose} />
    </Overlay>
  );
}
