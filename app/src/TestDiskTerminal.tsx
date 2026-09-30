import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { errorMessage } from "./load";

/**
 * Session TestDisk gardée au niveau du module : l'émulateur est ouvert une fois dans un élément
 * « hôte » qu'on rattache à l'écran à chaque affichage. Changer d'écran ne tue donc pas TestDisk.
 */
interface Session {
  term: Terminal;
  fit: FitAddon;
  host: HTMLDivElement;
  id: number | null;
  exited: boolean;
}
let session: Session | null = null;
const listeners = new Set<() => void>();
const notify = () => listeners.forEach((l) => l());

// Les sorties peuvent arriver avant que l'identifiant de session soit connu : on les garde.
const pending: { id: number; data: string }[] = [];
const pendingExit = new Map<number, number | null>();
let subscribed: Promise<unknown> | null = null;

function markExited(code: number | null): void {
  if (!session) return;
  session.exited = true;
      session.term.write(`

[90m[TestDisk terminé${code !== null ? `, code ${code}` : ""}][0m

`);
  notify();
}

/** Abonnement aux évènements du terminal, fait une seule fois, au premier lancement. */
function subscribe(): Promise<unknown> {
  subscribed ??= Promise.all([
    listen<{ id: number; data: string }>("terminal-output", (e) => {
      if (session?.id === e.payload.id) session.term.write(e.payload.data);
      else pending.push(e.payload);
    }),
    listen<{ id: number; code: number | null }>("terminal-exit", (e) => {
      if (session?.id === e.payload.id) markExited(e.payload.code);
      // Outil terminé avant que l'identifiant soit connu (DLL manquante...) : fin gardée aussi.
      else pendingExit.set(e.payload.id, e.payload.code);
    }),
  ]);
  return subscribed;
}

async function start(): Promise<void> {
  await subscribe();
  const host = document.createElement("div");
  host.className = "term-host";
  const term = new Terminal({
    fontFamily: '"IBM Plex Mono", Consolas, monospace',
    fontSize: 13,
    cursorBlink: true,
    // TestDisk dessine ses menus en couleurs sur fond sombre, comme une console classique.
    theme: { background: "#14161b", foreground: "#e6e9ee", cursor: "#e6e9ee" },
    scrollback: 0,
  });
  const fit = new FitAddon();
  term.loadAddon(fit);
  session = { term, fit, host, id: null, exited: false };
  notify();
}

async function launch(container: HTMLDivElement): Promise<void> {
  if (!session) return;
  const { term, fit } = session;
  term.open(session.host);
  container.appendChild(session.host);
  fit.fit();
  const id = await invoke<number>("terminal_open", { tool: "testdisk", cols: term.cols, rows: term.rows });
  session.id = id;
  for (const p of pending.splice(0)) if (p.id === id) term.write(p.data);
  if (pendingExit.has(id)) {
    markExited(pendingExit.get(id) ?? null);
    pendingExit.delete(id);
  }
  // Clavier → TestDisk. xterm.js répond aussi seul aux questions de ConPTY (position du curseur).
  term.onData((d) => void invoke("terminal_write", { id, data: d }).catch(() => {}));
  term.onResize(({ cols, rows }) => void invoke("terminal_resize", { id, cols, rows }).catch(() => {}));
  term.focus();
  notify();
}

function close(): void {
  if (!session) return;
  if (session.id !== null && !session.exited) void invoke("terminal_close", { id: session.id });
  session.term.dispose();
  session.host.remove();
  session = null;
  notify();
}

/** Onglet « TestDisk » de l'écran Récupération. */
export function TestDiskTerminal({ available }: { available: boolean }) {
  const container = useRef<HTMLDivElement>(null);
  const [, force] = useState(0);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const l = () => force((n) => n + 1);
    listeners.add(l);
    return () => void listeners.delete(l);
  }, []);

  // Rattache l'émulateur existant (retour sur l'écran) et suit la taille du cadre.
  useEffect(() => {
    const el = container.current;
    if (!el || !session || session.id === null) return;
    el.appendChild(session.host);
    session.fit.fit();
    const ro = new ResizeObserver(() => session?.fit.fit());
    ro.observe(el);
    return () => {
      ro.disconnect();
      session?.host.remove();
    };
  });

  const open = async () => {
    setError(null);
    // Relance : l'ancien émulateur est libéré d'abord (sinon deux terminaux s'empilent).
    if (session) close();
    try {
      await start();
      if (container.current) await launch(container.current);
    } catch (e) {
      close();
      setError(errorMessage(e));
    }
  };

  // Lancement en cours compris (identifiant pas encore reçu) : pas de second clic possible.
  const running = session !== null && !session.exited;

  return (
    <section className="panel" aria-label="TestDisk">
      <div className="panel-head">
        <p className="muted small help-text">
          TestDisk se pilote au clavier : flèches pour choisir, Entrée pour valider, <kbd>q</kbd> pour revenir.
          Fichiers supprimés : choisis le disque, puis la partition, <strong>Advanced</strong> →{" "}
          <strong>Undelete</strong>, sélectionne avec <kbd>:</kbd> et copie avec <kbd>C</kbd>. Partition perdue :{" "}
          <strong>Analyse</strong> → <strong>Quick Search</strong>. Le journal testdisk.log est écrit dans recup/ sur la
          clé.
        </p>
        <div className="header-actions">
          {!running && (
            <button type="button" className="btn btn-primary" onClick={() => void open()} disabled={!available}>
              {session?.exited ? "Relancer TestDisk" : "Lancer TestDisk"}
            </button>
          )}
          {session && (
            <button type="button" className="btn" onClick={close}>
              Fermer
            </button>
          )}
          <button
            type="button"
            className="btn"
            disabled={!available}
            onClick={() => void invoke("open_console_window", { tool: "testdisk" }).catch((e: unknown) => setError(errorMessage(e)))}
          >
            Fenêtre séparée
          </button>
        </div>
      </div>
      {!available && <p className="text-bad small">TestDisk absent de la clé : lance tools/fetch-tools-windows.ps1.</p>}
      {error && <p className="text-bad small">{error}</p>}
      <div ref={container} className={session ? "term-frame" : "term-frame empty"} onClick={() => session?.term.focus()}>
        {!session && <span className="muted small">TestDisk n'est pas lancé.</span>}
      </div>
    </section>
  );
}
