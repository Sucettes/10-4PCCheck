// Suivi des tâches longues du moteur (app/src-tauri/src/jobs.rs) : état initial lu une fois,
// puis mises à jour par les évènements `job-progress` et `job-done`.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";

export type JobResult<R> = { ok: R } | { error: unknown };

export interface JobState<P, R> {
  running: boolean;
  progress: P | null;
  result: JobResult<R> | null;
}

interface RawState {
  running: boolean;
  progress: unknown;
  result: unknown;
}

const empty = { running: false, progress: null, result: null };

export const cancelJob = (id: string): Promise<void> => invoke<void>("cancel_job", { id });

/**
 * État d'une tâche. `id` null : aucune tâche suivie. Le composant peut être démonté et remonté
 * pendant la tâche (changement d'écran) : il retrouve l'état gardé côté Rust.
 */
export function useJob<P, R>(id: string | null): [JobState<P, R>, (s: JobState<P, R>) => void] {
  const [state, setState] = useState<JobState<P, R>>(empty);

  useEffect(() => {
    if (id === null) return;
    let alive = true;
    // Abonnement avant la lecture de l'état : aucun évènement perdu entre les deux.
    const unlisten = Promise.all([
      listen<{ id: string; data: P }>("job-progress", (e) => {
        if (alive && e.payload.id === id) setState((s) => ({ ...s, running: true, progress: e.payload.data }));
      }),
      listen<{ id: string; result: JobResult<R> }>("job-done", (e) => {
        if (alive && e.payload.id === id) setState((s) => ({ ...s, running: false, result: e.payload.result }));
      }),
    ]);
    invoke<RawState>("job_state", { id })
      .then((s) => {
        if (alive) setState({ running: s.running, progress: s.progress as P | null, result: s.result as JobResult<R> | null });
      })
      .catch(() => {});
    return () => {
      alive = false;
      void unlisten.then((fns) => fns.forEach((f) => f()));
    };
  }, [id]);

  return [state, setState];
}

export function isOk<R>(r: JobResult<R> | null): r is { ok: R } {
  return r !== null && "ok" in r;
}
