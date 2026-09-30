// Captures d'écran du wiki (docs/wiki/images/), prises en mode démo : données fictives, aucune
// information d'une vraie machine. Lancé par tools/dev/wiki-screenshots.sh dans un conteneur
// Playwright, sur l'interface déjà construite (app/dist).
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { extname, join, normalize } from "node:path";
import { chromium } from "playwright";

const DIST = "/dist";
const OUT = "/out";
const TYPES = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript",
  ".css": "text/css",
  ".json": "application/json",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".woff": "font/woff",
  ".woff2": "font/woff2",
};

// Petit serveur statique : l'interface construite est une page unique (routage par ancre).
const server = createServer(async (req, res) => {
  const path = normalize(decodeURIComponent(new URL(req.url, "http://x").pathname)).replace(/^([/\\])+/, "");
  const file = join(DIST, path || "index.html");
  try {
    const body = await readFile(file);
    res.writeHead(200, { "content-type": TYPES[extname(file)] ?? "application/octet-stream" });
    res.end(body);
  } catch {
    res.writeHead(404).end();
  }
});
await new Promise((r) => server.listen(4173, r));

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1 });
const settle = () => page.waitForTimeout(900);

/** Capture d'un écran ; `prepare` clique au besoin avant la capture. */
async function shot(name, hash, { full = false, prepare } = {}) {
  await page.goto(`http://localhost:4173/#${hash}`);
  await page.waitForSelector("main");
  await settle();
  if (prepare) {
    await prepare();
    await settle();
  }
  if (full) {
    // L'application défile dans <main>, pas dans la page : on agrandit la fenêtre à la hauteur
    // du contenu pour tout capturer.
    const height = await page.evaluate(() => Math.max(900, document.querySelector("main").scrollHeight + 32));
    await page.setViewportSize({ width: 1440, height });
    await settle();
  }
  await page.screenshot({ path: `${OUT}/${name}.png` });
  await page.setViewportSize({ width: 1440, height: 900 });
  console.log(`capture : ${name}.png`);
}

await shot("accueil", "accueil");
await shot("analyse-complete", "analyse", { full: true });
await shot("disque-ssd", "disques", { full: true });
await shot("disque-dur-defaillant", "disques", {
  full: true,
  prepare: () => page.getByRole("tab", { name: /WD10EZEX/ }).click(),
});
await shot("telephone", "telephone", {
  full: true,
  prepare: () => page.getByRole("button", { name: "Analyser" }).click(),
});
await shot("recuperation", "recuperation");
await shot("rapports", "rapports");

await browser.close();
server.close();
