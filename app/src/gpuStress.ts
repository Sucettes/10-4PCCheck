/**
 * Test de charge graphique en WebGL2, fait dans la vue web : un shader de calcul lourd est rendu
 * en boucle dans un canevas hors écran pendant `seconds` secondes.
 *
 * - Débit : rendus complets par seconde. Chaque lot de rendus est suivi d'une barrière
 *   (`fenceSync`) ; on compte les lots dont la barrière est franchie, donc le travail réellement
 *   fini par la carte, pas les commandes envoyées. Trois lots restent en attente en permanence pour
 *   que la carte ne chôme pas pendant que la page rend la main.
 * - Stabilité : toutes les 10 s, une image de contrôle (entrée fixe) est rendue et comparée octet
 *   par octet à celle du début. Un calcul déterministe qui change de résultat trahit une carte
 *   instable (surchauffe, mémoire vidéo, fréquence trop haute).
 * - Température : capteur NVIDIA lu par le moteur (`nvidia-smi`) toutes les 2 s, gardé seulement
 *   si WebGL rend bien sur cette carte.
 *
 * Le moteur vit au niveau du module : quitter l'écran ne l'arrête pas.
 */
import { invoke } from "@tauri-apps/api/core";

export interface GpuSample {
  t_s: number;
  passes_per_s: number;
  temperature_c: number | null;
}

export interface GpuTestInput {
  renderer: string | null;
  samples: GpuSample[];
  render_errors: number;
  checks: number;
  cancelled: boolean;
}

export interface GpuState {
  running: boolean;
  seconds: number;
  renderer: string | null;
  samples: GpuSample[];
  checks: number;
  renderErrors: number;
}

interface GpuSensor {
  name: string;
  temperature_c: number | null;
}

const SIZE = 1024;
const CHECK_SIZE = 256;
const CHECK_EVERY_S = 10;
const SENSOR_EVERY_MS = 2000;
/** Durée visée d'un lot : assez court pour que l'interface reste fluide. */
const BATCH_MS = 30;
/** Lots envoyés d'avance : la carte a toujours du travail pendant que la page attend. */
const IN_FLIGHT = 3;

const VERTEX = `#version 300 es
void main() {
  vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
  gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}`;

// Itération bornée (|p| reste sous ~6) : ni infini ni NaN, donc un résultat reproductible.
const FRAGMENT = `#version 300 es
precision highp float;
uniform float u_seed;
out vec4 color;
void main() {
  vec2 p = gl_FragCoord.xy / 256.0 + u_seed;
  vec3 c = vec3(0.0);
  for (int i = 0; i < 96; i++) {
    p = vec2(sin(p.x * 1.7 + p.y * 0.3), cos(p.y * 1.3 - p.x * 0.7)) * 2.0 + p.yx * 0.5;
    c += abs(vec3(sin(p.x), cos(p.y), sin(p.x + p.y)));
  }
  color = vec4(fract(c / 96.0 * 3.0), 1.0);
}`;

const IDLE: GpuState = { running: false, seconds: 0, renderer: null, samples: [], checks: 0, renderErrors: 0 };
let state: GpuState = IDLE;
const listeners = new Set<() => void>();
let stopRequested = false;

function set(patch: Partial<GpuState>) {
  state = { ...state, ...patch };
  listeners.forEach((l) => l());
}

export const gpuState = (): GpuState => state;
export function subscribeGpu(l: () => void): () => void {
  listeners.add(l);
  return () => listeners.delete(l);
}
/** Arrête le test : le résultat partiel est gardé et marqué « arrêté avant la fin ». */
export function stopGpuStress() {
  stopRequested = true;
}

function compile(gl: WebGL2RenderingContext, type: number, src: string): WebGLShader {
  const s = gl.createShader(type)!;
  gl.shaderSource(s, src);
  gl.compileShader(s);
  if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s) ?? "shader invalide");
  return s;
}

function setup(canvas: HTMLCanvasElement) {
  const gl = canvas.getContext("webgl2", {
    antialias: false,
    depth: false,
    preserveDrawingBuffer: false,
    powerPreference: "high-performance",
  });
  if (!gl) throw new Error("WebGL 2 indisponible : pilote graphique absent ou désactivé.");
  const program = gl.createProgram()!;
  gl.attachShader(program, compile(gl, gl.VERTEX_SHADER, VERTEX));
  gl.attachShader(program, compile(gl, gl.FRAGMENT_SHADER, FRAGMENT));
  gl.linkProgram(program);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(program) ?? "programme invalide");
  gl.useProgram(program);
  gl.bindVertexArray(gl.createVertexArray());
  const seed = gl.getUniformLocation(program, "u_seed");
  const info = gl.getExtension("WEBGL_debug_renderer_info");
  const renderer = info ? String(gl.getParameter(info.UNMASKED_RENDERER_WEBGL)) : null;
  return { gl, seed, renderer };
}

/** Image de contrôle : entrée fixe, résumée par un hachage FNV-1a des pixels. */
function checkImage(gl: WebGL2RenderingContext, seed: WebGLUniformLocation | null): number {
  gl.viewport(0, 0, CHECK_SIZE, CHECK_SIZE);
  gl.uniform1f(seed, 0.125);
  gl.drawArrays(gl.TRIANGLES, 0, 3);
  const px = new Uint8Array(CHECK_SIZE * CHECK_SIZE * 4);
  gl.readPixels(0, 0, CHECK_SIZE, CHECK_SIZE, gl.RGBA, gl.UNSIGNED_BYTE, px);
  let h = 0x811c9dc5;
  for (const b of px) h = Math.imul(h ^ b, 0x01000193);
  gl.viewport(0, 0, SIZE, SIZE);
  return h >>> 0;
}

const yieldToUi = () => new Promise<void>((r) => setTimeout(r, 0));

/**
 * Lance le test et renvoie les mesures brutes. Le temps ne compte que pendant que la fenêtre est
 * visible : une fenêtre réduite est ralentie par le système et fausserait la courbe.
 */
export async function runGpuStress(seconds: number): Promise<GpuTestInput> {
  const canvas = document.createElement("canvas");
  canvas.width = SIZE;
  canvas.height = SIZE;
  let lost = false;
  canvas.addEventListener("webglcontextlost", (e) => {
    e.preventDefault();
    lost = true;
  });
  const { gl, seed, renderer } = setup(canvas);
  // Température NVIDIA seulement si WebGL rend sur une carte NVIDIA (sinon, portable à deux
  // cartes : la température ne serait pas celle de la carte testée).
  const useSensor = renderer === null || /nvidia/i.test(renderer);
  let temperature: number | null = null;
  const readSensor = () => {
    if (!useSensor) return;
    invoke<GpuSensor[]>("gpu_sensors_now")
      .then((s) => {
        const match = s.find((g) => renderer?.includes(g.name)) ?? s[0];
        temperature = match?.temperature_c ?? null;
      })
      .catch(() => (temperature = null));
  };
  const sensorTimer = setInterval(readSensor, SENSOR_EVERY_MS);
  readSensor();

  stopRequested = false;
  set({ ...IDLE, running: true, seconds, renderer });
  const samples: GpuSample[] = [];
  let checks = 0;
  let renderErrors = 0;
  const inFlight: { sync: WebGLSync; passes: number }[] = [];
  try {
    gl.viewport(0, 0, SIZE, SIZE);
    const reference = checkImage(gl, seed);
    let batch = 1;
    let active = 0; // ms de test écoulées, fenêtre visible
    let windowMs = 0;
    let windowPasses = 0;
    let nextCheck = CHECK_EVERY_S;
    let frame = 0;
    let last = performance.now();
    let lastDone = last;
    while (active < seconds * 1000 && !stopRequested && !lost) {
      if (document.hidden) {
        await new Promise((r) => setTimeout(r, 250));
        last = lastDone = performance.now();
        continue;
      }
      // File toujours pleine : la carte calcule pendant que la page rend la main à l'interface.
      while (inFlight.length < IN_FLIGHT) {
        for (let i = 0; i < batch; i++) {
          gl.uniform1f(seed, (frame++ % 1000) * 0.001);
          gl.drawArrays(gl.TRIANGLES, 0, 3);
        }
        inFlight.push({ sync: gl.fenceSync(gl.SYNC_GPU_COMMANDS_COMPLETE, 0)!, passes: batch });
        gl.flush();
      }
      await yieldToUi();
      const now = performance.now();
      active += now - last;
      windowMs += now - last;
      last = now;
      // L'état d'une barrière ne change qu'entre deux tâches : d'où l'attente juste avant.
      let completed = 0;
      while (inFlight.length > 0 && gl.getSyncParameter(inFlight[0]!.sync, gl.SYNC_STATUS) === gl.SIGNALED) {
        const done = inFlight.shift()!;
        gl.deleteSync(done.sync);
        completed += done.passes;
      }
      if (completed > 0) {
        windowPasses += completed;
        // Débit calculé sur tout ce qui a fini depuis le dernier constat (deux lots peuvent finir
        // ensemble), puis lot suivant dimensionné pour durer environ BATCH_MS.
        const rate = completed / Math.max(now - lastDone, 1);
        batch = Math.max(1, Math.min(4096, Math.round(rate * BATCH_MS)));
        lastDone = now;
      }

      if (windowMs >= 1000) {
        samples.push({
          t_s: Math.round(active / 100) / 10,
          passes_per_s: windowPasses / (windowMs / 1000),
          temperature_c: temperature,
        });
        windowMs = 0;
        windowPasses = 0;
        if (active / 1000 >= nextCheck) {
          nextCheck += CHECK_EVERY_S;
          checks++;
          if (checkImage(gl, seed) !== reference) renderErrors++;
        }
        set({ samples: [...samples], checks, renderErrors });
      }
      await yieldToUi();
    }
    // Perte du contexte en pleine charge : le pilote a réinitialisé la carte (plantage).
    if (lost) {
      checks++;
      renderErrors++;
    }
  } finally {
    clearInterval(sensorTimer);
    inFlight.forEach((f) => gl.deleteSync(f.sync));
    gl.getExtension("WEBGL_lose_context")?.loseContext();
    set({ running: false, samples: [...samples], checks, renderErrors });
  }
  return { renderer, samples, render_errors: renderErrors, checks, cancelled: stopRequested };
}
