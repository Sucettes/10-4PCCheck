import { useCallback, useEffect, useState, useSyncExternalStore } from "react";
import { invoke } from "@tauri-apps/api/core";
import { formatBytes } from "./format";
import { gpuState, runGpuStress, stopGpuStress, subscribeGpu, type GpuState } from "./gpuStress";
import { interactiveResults, InteractiveTests, useResults } from "./InteractiveTests";
import { cancelJob, isOk, useJob, waitForJob } from "./jobs";
import { errorMessage } from "./load";
import { machineName } from "./moreTypes";
import type {
  ChecklistEntry,
  Level,
  MachineInventory,
  RamProgress,
  Report,
  ReportItem,
  StressResult,
  StressSample,
} from "./moreTypes";
import { StressChart } from "./StressChart";
import { ReportButton } from "./ReportButton";
import { VerdictBanner } from "./Verdict";

/** Vérifications à faire devant le vendeur, enregistrées dans le rapport. */
const CHECKLIST = [
  "Session et compte Microsoft du vendeur retirés (réinitialisation faite)",
  "Aucun mot de passe BIOS / UEFI",
  "Numéro de série identique à l'étiquette sous l'appareil",
  "Le chargeur fourni charge la batterie",
  "Chaque port USB reconnaît la clé",
  "Charnières, coque et écran sans fissure",
  "Le Wi-Fi se connecte à un réseau",
];

const CPU_SECONDS = 300;
const GPU_SECONDS = 120;

type Step = "idle" | "inventory" | "disks" | "ram" | "cpu" | "gpu" | "done";
const STEP_LABELS: Record<Step, string> = {
  idle: "",
  inventory: "Inventaire du matériel",
  disks: "Lecture des disques",
  ram: "Test de la mémoire (partiel)",
  cpu: "Test de charge du processeur",
  gpu: "Test de charge de la carte graphique",
  done: "Analyse terminée",
};

const LEVEL_PILL: Record<Level, { label: string; cls: string }> = {
  ok: { label: "Bon", cls: "pill-good" },
  info: { label: "Info", cls: "pill-neutral" },
  warn: { label: "À surveiller", cls: "pill-warn" },
  bad: { label: "Critique", cls: "pill-bad" },
  neutral: { label: "—", cls: "pill-neutral" },
};
// « Info » ne décrit pas un état : il passe sous « Bon » pour la pastille de la carte.
const RANK: Record<Level, number> = { neutral: 0, info: 1, ok: 2, warn: 3, bad: 4 };

/** État gardé entre deux passages sur l'écran. */
let savedChecks: Record<string, boolean> = {};
let savedStep: Step = "idle";
/** Échantillons du test de charge en cours, gardés si l'écran est quitté puis rouvert. */
let liveSamples: StressSample[] = [];

function checklistEntries(checks: Record<string, boolean>): ChecklistEntry[] {
  return CHECKLIST.map((label) => ({ label, checked: !!checks[label], note: null }));
}

function interactivePayload() {
  return interactiveResults().map((r) => ({ label: r.label, status: r.status, note: r.note }));
}

/** Écran « Analyse complète » (maquette FullScan.dc.html). */
export function FullScanPage({ onRefreshDisks }: { onRefreshDisks: () => Promise<void> }) {
  const [inventory, setInventory] = useState<MachineInventory | null>(null);
  const [preview, setPreview] = useState<Report | null>(null);
  const [step, setStep] = useState<Step>(savedStep);
  const [error, setError] = useState<string | null>(null);
  const [checks, setChecks] = useState<Record<string, boolean>>(savedChecks);
  const interactive = useResults();
  const [cpu] = useJob<StressSample, StressResult>("cpu");
  const [samples, setSamples] = useState<StressSample[]>(liveSamples);

  // Un échantillon par seconde pendant le test ; à la fin, la série complète du résultat fait foi.
  useEffect(() => {
    const s = cpu.progress;
    if (!s || !cpu.running) return;
    const last = liveSamples[liveSamples.length - 1];
    if (last && s.elapsed_ms <= last.elapsed_ms) {
      if (s.elapsed_ms < last.elapsed_ms) liveSamples = [];
      else return;
    }
    liveSamples = [...liveSamples, s];
    setSamples(liveSamples);
  }, [cpu.progress, cpu.running]);
  useEffect(() => {
    if (!cpu.running && isOk(cpu.result) && cpu.result.ok.samples.length > 0) {
      liveSamples = cpu.result.ok.samples;
      setSamples(liveSamples);
    }
  }, [cpu.running, cpu.result]);
  const [ram] = useJob<RamProgress, unknown>("ram");
  const gpu = useSyncExternalStore(subscribeGpu, gpuState);
  const [gpuNote, setGpuNote] = useState<string | null>(null);

  const refreshPreview = useCallback(() => {
    invoke<Report>("preview_machine_report", { interactive: interactivePayload(), checklist: checklistEntries(checks) })
      .then(setPreview)
      .catch(() => setPreview(null));
  }, [checks]);

  useEffect(() => {
    invoke<MachineInventory>("machine_inventory", { refresh: false })
      .then((inv) => {
        setInventory(inv);
        refreshPreview();
      })
      .catch((e: unknown) => setError(errorMessage(e)));
  }, [refreshPreview]);

  // Les tests interactifs changent le verdict : aperçu recalculé.
  useEffect(() => {
    if (inventory) refreshPreview();
  }, [interactive.size, inventory, refreshPreview]);

  const go = (s: Step) => {
    savedStep = s;
    setStep(s);
  };

  const run = async () => {
    setError(null);
    try {
      go("inventory");
      setInventory(await invoke<MachineInventory>("machine_inventory", { refresh: true }));
      go("disks");
      await onRefreshDisks();
      go("ram");
      await invoke("start_ram_test");
      await waitForJob("ram");
      go("cpu");
      liveSamples = [];
      setSamples([]);
      await invoke("start_cpu_stress", { seconds: CPU_SECONDS });
      await waitForJob("cpu");
      go("gpu");
      setGpuNote(null);
      try {
        const input = await runGpuStress(GPU_SECONDS);
        await invoke("record_gpu_test", { input });
      } catch (e) {
        // Sans WebGL (pilote absent, bureau à distance), le reste de l'analyse reste valable.
        setGpuNote(`Test graphique impossible : ${errorMessage(e)}`);
      }
      go("done");
    } catch (e) {
      setError(errorMessage(e));
      go("idle");
    }
    refreshPreview();
  };

  const running = step !== "idle" && step !== "done";
  const name = inventory ? machineName(inventory) : "";
  const toggle = (label: string) =>
    setChecks((c) => {
      const next = { ...c, [label]: !c[label] };
      savedChecks = next;
      return next;
    });

  return (
    <>
      <header className="page-header">
        <div>
          <div className="eyebrow">Analyse complète{step === "done" ? " · terminée" : ""}</div>
          <h1>{name || "Cet ordinateur"}</h1>
          {inventory && (
            <p className="muted">
              {[inventory.cpu?.name, inventory.memory?.total_bytes ? formatBytes(inventory.memory.total_bytes) : null, inventory.os?.name]
                .filter(Boolean)
                .join(" · ")}
            </p>
          )}
        </div>
        <div className="header-actions">
          <button type="button" className={preview ? "btn" : "btn btn-primary"} onClick={() => void run()} disabled={running}>
            {step === "done" ? "Relancer l'analyse" : "Lancer l'analyse"}
            <span className="btn-sub">~9 min</span>
          </button>
          {inventory && (
            <ReportButton
              command="save_machine_report"
              args={{ interactive: interactivePayload(), checklist: checklistEntries(checks) }}
            />
          )}
        </div>
      </header>

      {error && (
        <div className="banner banner-bad" role="alert">
          {error}
        </div>
      )}
      {!inventory && !error && <p className="muted">Lecture du matériel… (environ 10 secondes)</p>}

      {gpuNote && <div className="banner banner-warn">{gpuNote}</div>}
      {running && <StepProgress step={step} cpu={cpu.progress} ram={ram.progress} gpu={gpu} />}
      {samples.length > 1 && (
        <section className="panel" aria-label="Processeur sous charge">
          <h3>Processeur sous charge</h3>
          <StressChart samples={samples} seconds={CPU_SECONDS} />
        </section>
      )}
      {gpu.samples.length > 1 && (
        <section className="panel" aria-label="Carte graphique sous charge">
          <h3>Carte graphique sous charge</h3>
          {gpu.renderer && <p className="muted small">{gpu.renderer}</p>}
          <StressChart
            samples={gpu.samples.map((g) => ({
              elapsed_ms: g.t_s * 1000,
              iterations_per_sec: g.passes_per_s,
              max_celsius: g.temperature_c,
            }))}
            seconds={GPU_SECONDS}
            rateLabel="Débit de rendu"
          />
        </section>
      )}

      {preview && (
        <VerdictBanner
          level={preview.verdict.level}
          summary={preview.verdict.summary}
          counts={{ ok: preview.verdict.ok, warn: preview.verdict.warn, bad: preview.verdict.bad }}
        />
      )}

      <div className="fullscan">
        <section className="section-cards" aria-label="Composants">
          {preview?.sections.map((s) => (
            <SectionCard key={s.id} title={s.title} items={s.items} />
          ))}
        </section>
        <div className="fullscan-side">
          <InteractiveTests />
          <section className="panel" aria-label="Vérifications devant le vendeur">
            <h3>Devant le vendeur</h3>
            <ul className="manual-checks single">
              {CHECKLIST.map((label) => (
                <li key={label}>
                  <label>
                    <input type="checkbox" checked={!!checks[label]} onChange={() => toggle(label)} />
                    <span className="manual-label">{label}</span>
                  </label>
                </li>
              ))}
            </ul>
          </section>
        </div>
      </div>
    </>
  );
}

function StepProgress({
  step,
  cpu,
  ram,
  gpu,
}: {
  step: Step;
  cpu: StressSample | null;
  ram: RamProgress | null;
  gpu: GpuState;
}) {
  const steps: Step[] = ["inventory", "disks", "ram", "cpu", "gpu"];
  let pct: number | null = null;
  let detail = "";
  if (step === "cpu" && cpu) {
    pct = (cpu.elapsed_ms / 1000 / CPU_SECONDS) * 100;
    detail = `${Math.round(cpu.elapsed_ms / 1000)} s sur ${CPU_SECONDS}${cpu.max_celsius !== null ? ` · ${Math.round(cpu.max_celsius)} °C` : ""}`;
  } else if (step === "ram" && ram) {
    pct = ram.bytes_total > 0 ? (ram.bytes_done / ram.bytes_total) * 100 : 0;
    detail = `Passe ${ram.pass} sur ${ram.total_passes} · ${ram.errors} erreur(s)`;
  } else if (step === "gpu") {
    const last = gpu.samples[gpu.samples.length - 1];
    pct = last ? (last.t_s / GPU_SECONDS) * 100 : null;
    detail = last
      ? `${Math.round(last.t_s)} s sur ${GPU_SECONDS}${last.temperature_c !== null ? ` · ${Math.round(last.temperature_c)} °C` : ""} · ${gpu.renderErrors} erreur(s) de rendu`
      : "";
  }
  const job = step === "cpu" ? "cpu" : step === "ram" ? "ram" : null;
  return (
    <section className="panel" role="status" aria-label="Progression de l'analyse">
      <ol className="steps">
        {steps.map((s, i) => (
          <li key={s} className={s === step ? "step-current" : steps.indexOf(step) > i ? "step-done" : ""}>
            {STEP_LABELS[s]}
          </li>
        ))}
      </ol>
      <div className="test-progress-head">
        <span>{STEP_LABELS[step]}…</span>
        <span className="mono">{detail}</span>
      </div>
      <div className="bar" aria-hidden="true">
        <div className={pct === null ? "bar-fill bar-indeterminate" : "bar-fill"} style={pct === null ? undefined : { width: `${Math.min(pct, 100)}%` }} />
      </div>
      {(job || step === "gpu") && (
        <button type="button" className="btn" onClick={() => (job ? void cancelJob(job) : stopGpuStress())}>
          Passer ce test
        </button>
      )}
    </section>
  );
}

/** Carte d'une section du rapport : état le plus grave et les mesures les plus parlantes. */
function SectionCard({ title, items }: { title: string; items: ReportItem[] }) {
  const worst = items.reduce<Level>((w, i) => (RANK[i.level] > RANK[w] ? i.level : w), "neutral");
  const shown = [...items].sort((a, b) => RANK[b.level] - RANK[a.level]).slice(0, 5);
  const pill = LEVEL_PILL[worst];
  return (
    <article className="panel section-card">
      <div className="card-head">
        <h3>{title}</h3>
        {worst !== "neutral" && worst !== "info" && <span className={`pill ${pill.cls}`}>{pill.label}</span>}
      </div>
      <dl className="rows">
        {shown.map((i) => (
          <div className="row" key={i.label + i.value} title={i.detail ?? undefined}>
            <dt>{i.label}</dt>
            <dd className={`lvl-${i.level}`}>{i.value || LEVEL_PILL[i.level].label}</dd>
          </div>
        ))}
      </dl>
      {items.length > shown.length && <p className="muted small">et {items.length - shown.length} autre(s) mesure(s) dans le rapport</p>}
    </article>
  );
}
