// Gabarit du rapport PDF de PCCheck.
//
// Toutes les données viennent du fichier virtuel /data.json, fourni par pdf.rs. Une chaîne lue
// par json() est une valeur : l'afficher produit du texte littéral, jamais du balisage évalué.
// Ne jamais construire ce gabarit par concaténation de texte du rapport.

#let d = json("/data.json")
#let r = d.report

// Palette du plan, section 6 (mode clair).
#let ink = rgb("#1a1d23")
#let muted = rgb("#5d6574")
#let line-color = rgb("#e3e6eb")
#let soft-bg = rgb("#f6f7f9")
#let pal = (
  ok: (bg: rgb("#e3f5ea"), fg: rgb("#177a47"), bd: rgb("#1f9d5b")),
  warn: (bg: rgb("#fff3d6"), fg: rgb("#8a5a00"), bd: rgb("#f0d48a")),
  bad: (bg: rgb("#fdeceb"), fg: rgb("#b42318"), bd: rgb("#f0b8b4")),
  info: (bg: rgb("#e8effc"), fg: rgb("#1f4fb8"), bd: rgb("#c9d7f4")),
  neutral: (bg: rgb("#f6f7f9"), fg: rgb("#5d6574"), bd: rgb("#e3e6eb")),
)
#let colors(level) = pal.at(level, default: pal.neutral)
#let mono(s) = text(font: "DejaVu Sans Mono", size: 0.92em, s)
#let small(s) = text(size: 7.5pt, fill: muted, s)

// État : libellé texte + couleur, jamais la couleur seule.
#let badge(level) = {
  let c = colors(level)
  box(
    fill: c.bg,
    stroke: 0.6pt + c.bd,
    radius: 8pt,
    inset: (x: 6pt, y: 2.5pt),
    text(size: 7.5pt, weight: "bold", fill: c.fg, d.level_labels.at(level, default: "—")),
  )
}

#let section-title(t) = block(above: 16pt, below: 7pt, sticky: true, text(size: 12pt, weight: "bold", t))

#let tools-text = if r.tools.len() == 0 { "aucun" } else {
  r.tools.map(t => t.name + " " + t.version).join(", ")
}

#set document(title: r.title, author: "PCCheck")
#set text(font: "DejaVu Sans", size: 9pt, fill: ink, lang: "fr")
#set par(leading: 0.55em)
#set page(
  paper: "us-letter",
  margin: (x: 1.7cm, top: 1.6cm, bottom: 2.4cm),
  footer: context {
    set text(size: 6.8pt, fill: muted)
    line(length: 100%, stroke: 0.5pt + line-color)
    grid(
      columns: (1fr, auto),
      column-gutter: 12pt,
      stack(
        spacing: 3pt,
        "PCCheck " + r.tool_version + " · schéma " + str(r.schema_version) + " · " + d.date,
        "Outils tiers : " + tools-text,
        [SHA-256 du fichier JSON : #mono(d.hash)],
      ),
      align(right, "Page " + str(here().page()) + " sur " + str(counter(page).final().first())),
    )
  },
)

// ---------- En-tête ----------
#text(size: 7.5pt, fill: muted, tracking: 0.04em, upper("PCCheck · Rapport · " + d.subject_kind))
#v(1pt)
#text(size: 17pt, weight: "bold", r.title)
#v(0pt)
#text(fill: muted, r.subject.name + " · " + d.date)
#v(6pt)

#let vc = colors(r.verdict.level)
#let icon = (ok: "✓", warn: "!", bad: "✕").at(r.verdict.level, default: "–")
#let counter-box(label, n, c) = box(
  fill: white,
  stroke: 0.6pt + line-color,
  radius: 6pt,
  inset: (x: 8pt, y: 4pt),
  align(center, stack(
    spacing: 3pt,
    text(size: 7pt, fill: muted, label),
    text(size: 13pt, weight: "bold", fill: c, str(n)),
  )),
)
#block(width: 100%, fill: vc.bg, stroke: 0.8pt + vc.bd, radius: 8pt, inset: 12pt, breakable: false)[
  #grid(
    columns: (auto, 1fr, auto),
    column-gutter: 12pt,
    align: horizon,
    circle(radius: 13pt, fill: vc.fg, stroke: none, align(center + horizon, text(
      fill: white,
      weight: "bold",
      size: 14pt,
      icon,
    ))),
    stack(
      spacing: 5pt,
      text(size: 15pt, weight: "bold", fill: vc.fg, d.verdict_label),
      text(r.verdict.summary),
    ),
    grid(
      columns: 3,
      column-gutter: 5pt,
      counter-box("OK", r.verdict.ok, pal.ok.fg),
      counter-box("À surveiller", r.verdict.warn, pal.warn.fg),
      counter-box("Critiques", r.verdict.bad, pal.bad.fg),
    ),
  )
]

// ---------- Sujet ----------
#if r.subject.details.len() > 0 {
  section-title(d.subject_kind)
  grid(
    columns: (1fr, 1fr, 1fr),
    column-gutter: 12pt,
    row-gutter: 8pt,
    ..r.subject.details.map(x => stack(spacing: 2.5pt, small(x.label), text(x.value))),
  )
}

// ---------- Sections ----------
#let head-cell(s) = text(size: 7.5pt, weight: "bold", fill: muted, s)
#for (i, s) in r.sections.enumerate() {
  section-title(s.title)
  if s.items.len() > 0 {
    table(
      columns: (1.3fr, 1fr, auto),
      inset: (x: 6pt, y: 5pt),
      align: (left + horizon, right + horizon, center + horizon),
      stroke: (x, y) => if y > 0 { (top: 0.5pt + line-color) },
      table.header(head-cell("Élément"), head-cell("Valeur"), head-cell("État")),
      ..s.items
        .map(it => (
          {
            it.label
            if it.detail != none {
              linebreak()
              small(it.detail)
            }
          },
          mono(it.value),
          badge(it.level),
        ))
        .flatten(),
    )
  }
  for t in d.tables.at(i) {
    block(above: 11pt, below: 5pt, sticky: true, text(size: 8.5pt, weight: "bold", t.title))
    table(
      columns: (auto,) * t.columns.len(),
      inset: (x: 5pt, y: 3.5pt),
      stroke: (x, y) => (bottom: 0.4pt + line-color),
      fill: (x, y) => if y == 0 { soft-bg } else if calc.even(y) { rgb("#fafbfc") },
      table.header(..t.columns.map(head-cell)),
      ..t.rows.flatten().map(c => text(font: "DejaVu Sans Mono", size: 7pt, c)),
    )
  }
}

// ---------- Vérifications manuelles ----------
#if r.checklist.len() > 0 {
  section-title("Vérifications manuelles")
  table(
    columns: (auto, 1fr, auto),
    inset: (x: 6pt, y: 5pt),
    align: (center + horizon, left + horizon, right + horizon),
    stroke: (x, y) => if y > 0 { (top: 0.5pt + line-color) },
    ..r
      .checklist
      .map(c => (
        text(size: 11pt, if c.checked { "☑" } else { "☐" }),
        {
          c.label
          if c.note != none {
            linebreak()
            small(c.note)
          }
        },
        if c.checked { text(weight: "bold", fill: pal.ok.fg, "Fait") } else {
          text(weight: "bold", fill: pal.warn.fg, "Non fait")
        },
      ))
      .flatten(),
  )
}

#v(12pt)
#small("Données brutes complètes : fichier JSON et rapport HTML du même nom. L'empreinte SHA-256 en pied de page permet de vérifier que le JSON n'a pas été modifié.")
