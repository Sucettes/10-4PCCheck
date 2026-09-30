//! Export HTML autonome : un seul fichier, CSS et JS en ligne, aucune ressource externe.
//!
//! Sécurité : tout texte venant du rapport (donc de la machine analysée : nom de modèle,
//! sorties d'outils...) passe par [`escape_html`]. Aucune donnée n'est placée dans le `<script>`,
//! ce qui évite le piège de `</script>` dans un JSON en ligne. Une politique CSP interdit en plus
//! tout chargement réseau et tout script externe.

use std::fmt::Write as _;

use crate::format::{date_fr, slug};
use crate::model::{report_hash, Level, Report, Section, Table};

/// Échappe un texte pour le corps HTML et les valeurs d'attribut entre guillemets
/// (`&`, `<`, `>`, `"`, `'`). `&` est traité en premier par construction (un seul passage).
pub fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + s.len() / 8);
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Rapport complet en un fichier HTML autonome.
pub fn to_html(report: &Report) -> String {
    let hash = report_hash(report);
    let e = escape_html;
    let mut h = String::with_capacity(32 * 1024);

    h.push_str("<!doctype html>\n<html lang=\"fr\">\n<head>\n<meta charset=\"utf-8\">\n");
    h.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    // Aucune ressource réseau : seul le CSS et le JS en ligne de ce fichier sont permis.
    h.push_str(
        "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; \
         style-src 'unsafe-inline'; script-src 'unsafe-inline'; img-src data:\">\n",
    );
    let _ = writeln!(h, "<title>{} · PCCheck</title>", e(&report.title));
    let _ = writeln!(
        h,
        "<style>{CSS}</style>\n</head>\n<body>\n<main class=\"page\">"
    );

    // En-tête : titre, sujet, date.
    let _ = writeln!(
        h,
        "<header class=\"head\"><p class=\"kicker\">PCCheck · Rapport · {kind}</p>\
         <h1>{title}</h1><p class=\"muted\">{name} · {date}</p></header>",
        kind = e(report.subject.kind.label()),
        title = e(&report.title),
        name = e(&report.subject.name),
        date = e(&date_fr(&report.generated_at)),
    );

    verdict_banner(&mut h, report);
    subject_card(&mut h, report);

    h.push_str(
        "<div class=\"search\"><label for=\"q\">Rechercher dans le rapport</label>\
         <input id=\"q\" type=\"search\" placeholder=\"Ex. température, 197, batterie\" \
         autocomplete=\"off\"><span id=\"q-count\" class=\"muted\" aria-live=\"polite\"></span></div>\n",
    );

    for (i, section) in report.sections.iter().enumerate() {
        section_card(&mut h, section, i);
    }

    checklist_card(&mut h, report);
    raw_card(&mut h, report);
    footer(&mut h, report, &hash);

    let _ = writeln!(h, "</main>\n<script>{JS}</script>\n</body>\n</html>");
    h
}

fn badge(level: Level) -> String {
    format!(
        "<span class=\"badge lvl-{}\">{}</span>",
        level.key(),
        escape_html(level.label())
    )
}

fn verdict_banner(h: &mut String, report: &Report) {
    let v = &report.verdict;
    let icon = match v.level {
        Level::Ok => "✓",
        Level::Warn => "!",
        Level::Bad => "✕",
        Level::Info | Level::Neutral => "–",
    };
    let _ = writeln!(
        h,
        "<section class=\"verdict lvl-{key}\" aria-label=\"Verdict\">\
         <div class=\"verdict-icon\" aria-hidden=\"true\">{icon}</div>\
         <div class=\"verdict-text\"><h2>{label}</h2><p>{summary}</p></div>\
         <dl class=\"counters\">\
         <div class=\"c-ok\"><dt>OK</dt><dd>{ok}</dd></div>\
         <div class=\"c-warn\"><dt>À surveiller</dt><dd>{warn}</dd></div>\
         <div class=\"c-bad\"><dt>Critiques</dt><dd>{bad}</dd></div>\
         </dl></section>",
        key = v.level.key(),
        label = escape_html(v.label()),
        summary = escape_html(&v.summary),
        ok = v.ok,
        warn = v.warn,
        bad = v.bad,
    );
}

fn subject_card(h: &mut String, report: &Report) {
    let s = &report.subject;
    if s.details.is_empty() {
        return;
    }
    let _ = write!(
        h,
        "<section class=\"card\"><h2>{}</h2><dl class=\"details\">",
        escape_html(s.kind.label())
    );
    for d in &s.details {
        let _ = write!(
            h,
            "<div><dt>{}</dt><dd>{}</dd></div>",
            escape_html(&d.label),
            escape_html(&d.value)
        );
    }
    h.push_str("</dl></section>\n");
}

fn section_card(h: &mut String, section: &Section, index: usize) {
    // L'identifiant vient des données : on n'en garde qu'une forme sûre pour l'attribut id.
    let _ = writeln!(
        h,
        "<section class=\"card\" id=\"s{index}-{id}\"><h2>{title}</h2>",
        id = slug(&section.id),
        title = escape_html(&section.title),
    );
    if !section.items.is_empty() {
        h.push_str("<ul class=\"items\">");
        for item in &section.items {
            let detail = item
                .detail
                .as_deref()
                .map(|d| format!("<small>{}</small>", escape_html(d)))
                .unwrap_or_default();
            let _ = write!(
                h,
                "<li class=\"filterable\"><div class=\"label\">{label}{detail}</div>\
                 <div class=\"value\">{value}</div>{badge}</li>",
                label = escape_html(&item.label),
                value = escape_html(&item.value),
                badge = badge(item.level),
            );
        }
        h.push_str("</ul>\n");
    }
    for table in &section.tables {
        table_html(h, table);
    }
    h.push_str("</section>\n");
}

fn table_html(h: &mut String, table: &Table) {
    if table.width() == 0 {
        return;
    }
    let _ = write!(
        h,
        "<div class=\"table-wrap\"><table class=\"data\"><caption>{}</caption><thead><tr>",
        escape_html(&table.title)
    );
    for i in 0..table.width() {
        let name = table.columns.get(i).map(String::as_str).unwrap_or("");
        let _ = write!(
            h,
            "<th scope=\"col\" aria-sort=\"none\"><button type=\"button\" class=\"sort\">{}</button></th>",
            escape_html(name)
        );
    }
    h.push_str("</tr></thead><tbody>");
    for row in table.normalized_rows() {
        h.push_str("<tr class=\"filterable\">");
        for cell in &row {
            let _ = write!(h, "<td>{}</td>", escape_html(cell));
        }
        h.push_str("</tr>");
    }
    h.push_str("</tbody></table></div>\n");
}

fn checklist_card(h: &mut String, report: &Report) {
    if report.checklist.is_empty() {
        return;
    }
    h.push_str("<section class=\"card\"><h2>Vérifications manuelles</h2><ul class=\"checklist\">");
    for entry in &report.checklist {
        let (class, mark, state) = if entry.checked {
            ("done", "☑", "Fait")
        } else {
            ("todo", "☐", "Non fait")
        };
        let note = entry
            .note
            .as_deref()
            .map(|n| format!("<small>{}</small>", escape_html(n)))
            .unwrap_or_default();
        let _ = write!(
            h,
            "<li class=\"filterable {class}\"><span class=\"mark\" aria-hidden=\"true\">{mark}</span>\
             <div class=\"label\">{label}{note}</div><span class=\"state\">{state}</span></li>",
            label = escape_html(&entry.label),
        );
    }
    h.push_str("</ul></section>\n");
}

fn raw_card(h: &mut String, report: &Report) {
    if report.raw.is_null() {
        return;
    }
    let pretty = serde_json::to_string_pretty(&report.raw).unwrap_or_default();
    let _ = writeln!(
        h,
        "<section class=\"card raw\"><details><summary>Données brutes complètes (JSON)</summary>\
         <pre>{}</pre></details></section>",
        escape_html(&pretty)
    );
}

fn footer(h: &mut String, report: &Report, hash: &str) {
    let tools = if report.tools.is_empty() {
        "aucun".to_string()
    } else {
        report
            .tools
            .iter()
            .map(|t| format!("{} {}", escape_html(&t.name), escape_html(&t.version)))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let _ = writeln!(
        h,
        "<footer class=\"foot\">\
         <p>PCCheck {version} · schéma {schema} · généré le {date} ({iso})</p>\
         <p>Outils tiers : {tools}</p>\
         <p>Empreinte SHA-256 du fichier JSON du rapport : <code class=\"hash\">{hash}</code></p>\
         <p class=\"muted\">Pour vérifier qu'il n'a pas été modifié : <code>sha256sum</code> (Linux) \
         ou <code>certutil -hashfile &lt;fichier&gt;.json SHA256</code> (Windows) doit donner la même valeur.</p>\
         </footer>",
        version = escape_html(&report.tool_version),
        schema = report.schema_version,
        date = escape_html(&date_fr(&report.generated_at)),
        iso = escape_html(&report.generated_at.to_rfc3339()),
        hash = escape_html(hash),
    );
}

/// Palette et règles du plan (section 6), mode clair seulement.
const CSS: &str = r#"
:root{
  --bg:#f6f7f9;--card:#ffffff;--border:#e3e6eb;--border-strong:#cfd5de;
  --text:#1a1d23;--muted:#5d6574;--accent:#2459d6;--active-bg:#e8effc;--active-text:#1f4fb8;
  --ok-bg:#e3f5ea;--ok-text:#177a47;--ok-dot:#1f9d5b;
  --warn-bg:#fff3d6;--warn-text:#8a5a00;--warn-border:#f0d48a;
  --bad-bg:#fdeceb;--bad-text:#b42318;--bad-border:#f0b8b4;
  --sans:"IBM Plex Sans","Segoe UI",system-ui,sans-serif;
  --title:"Space Grotesk","IBM Plex Sans","Segoe UI",system-ui,sans-serif;
  --mono:"IBM Plex Mono",ui-monospace,Consolas,"DejaVu Sans Mono",monospace;
  color-scheme:light;
}
*{box-sizing:border-box}
html{-webkit-text-size-adjust:100%}
body{margin:0;background:var(--bg);color:var(--text);font:15px/1.5 var(--sans)}
.page{max-width:980px;margin:0 auto;padding:24px 16px 40px}
h1,h2{font-family:var(--title);line-height:1.2;margin:0}
h1{font-size:26px;font-weight:700}
h2{font-size:18px;font-weight:600;margin-bottom:12px}
.kicker{margin:0 0 4px;color:var(--muted);font-size:13px;text-transform:uppercase;letter-spacing:.04em}
.muted{color:var(--muted)}
.head{margin-bottom:16px}.head p{margin:4px 0 0}
.card,.verdict{background:var(--card);border:1px solid var(--border);border-radius:12px;padding:18px 20px;margin:0 0 16px}
.verdict{display:flex;gap:16px;align-items:center;flex-wrap:wrap}
.verdict-icon{flex:none;width:48px;height:48px;border-radius:50%;display:grid;place-items:center;font:700 24px/1 var(--title);color:#fff;background:var(--muted)}
.verdict-text{flex:1 1 260px;min-width:0}.verdict-text h2{margin:0 0 4px;font-size:22px}.verdict-text p{margin:0}
.verdict.lvl-ok{background:var(--ok-bg);border-color:var(--ok-dot)}.verdict.lvl-ok .verdict-icon{background:var(--ok-dot)}.verdict.lvl-ok h2{color:var(--ok-text)}
.verdict.lvl-warn{background:var(--warn-bg);border-color:var(--warn-border)}.verdict.lvl-warn .verdict-icon{background:var(--warn-text)}.verdict.lvl-warn h2{color:var(--warn-text)}
.verdict.lvl-bad{background:var(--bad-bg);border-color:var(--bad-border)}.verdict.lvl-bad .verdict-icon{background:var(--bad-text)}.verdict.lvl-bad h2{color:var(--bad-text)}
.counters{display:flex;gap:8px;margin:0}
.counters div{background:var(--card);border:1px solid var(--border);border-radius:10px;padding:6px 12px;text-align:center;min-width:84px}
.counters dt{font-size:12px;color:var(--muted)}.counters dd{margin:0;font:700 20px/1.2 var(--title)}
.c-ok dd{color:var(--ok-text)}.c-warn dd{color:var(--warn-text)}.c-bad dd{color:var(--bad-text)}
.details{display:grid;grid-template-columns:repeat(auto-fill,minmax(210px,1fr));gap:10px 20px;margin:0}
.details dt{font-size:12px;color:var(--muted)}.details dd{margin:0;overflow-wrap:anywhere}
.search{display:flex;flex-wrap:wrap;gap:8px 12px;align-items:center;margin:0 0 16px}
.search label{font-weight:600}
.search input{flex:1 1 240px;min-height:44px;padding:8px 12px;border:1px solid var(--border-strong);border-radius:10px;font:inherit;background:var(--card);color:var(--text)}
.search input:focus{outline:2px solid var(--accent);outline-offset:1px}
.items,.checklist{list-style:none;margin:0;padding:0}
.items li,.checklist li{display:flex;gap:12px;align-items:center;padding:10px 0;border-top:1px solid var(--border)}
.items li:first-child,.checklist li:first-child{border-top:0}
.label{flex:1 1 auto;min-width:0}.label small{display:block;color:var(--muted);font-size:13px}
.value{font-family:var(--mono);font-size:14px;text-align:right;overflow-wrap:anywhere;max-width:45%}
.badge{flex:none;display:inline-block;min-width:96px;text-align:center;padding:3px 10px;border-radius:999px;font-size:13px;font-weight:600;border:1px solid transparent}
.lvl-ok.badge{background:var(--ok-bg);color:var(--ok-text);border-color:var(--ok-dot)}
.lvl-warn.badge{background:var(--warn-bg);color:var(--warn-text);border-color:var(--warn-border)}
.lvl-bad.badge{background:var(--bad-bg);color:var(--bad-text);border-color:var(--bad-border)}
.lvl-info.badge{background:var(--active-bg);color:var(--active-text)}
.lvl-neutral.badge{background:var(--bg);color:var(--muted);border-color:var(--border)}
.checklist .mark{font-size:20px;line-height:1}.checklist .state{font-size:13px;font-weight:600}
.checklist .done .state{color:var(--ok-text)}.checklist .todo .state{color:var(--warn-text)}
.table-wrap{overflow-x:auto;margin-top:14px}
table.data{width:100%;border-collapse:collapse;font-size:14px}
table.data caption{text-align:left;font-weight:600;padding:0 0 6px}
table.data th{padding:0;border-bottom:1px solid var(--border-strong);text-align:left;white-space:nowrap}
table.data td{padding:6px 10px;border-bottom:1px solid var(--border);font-family:var(--mono);font-size:13px}
table.data tbody tr:nth-child(even){background:#fafbfc}
.sort{all:unset;box-sizing:border-box;display:block;width:100%;min-height:44px;padding:10px;cursor:pointer;font:600 13px var(--sans);color:var(--text)}
.sort:hover{background:var(--active-bg);color:var(--active-text)}
.sort:focus-visible{outline:2px solid var(--accent);outline-offset:-2px}
th[aria-sort="ascending"] .sort::after{content:" ▲"}th[aria-sort="descending"] .sort::after{content:" ▼"}
[hidden]{display:none!important}
.raw summary{cursor:pointer;min-height:44px;display:flex;align-items:center;font-weight:600}
.raw pre{margin:8px 0 0;max-height:480px;overflow:auto;background:var(--bg);border:1px solid var(--border);border-radius:10px;padding:12px;font:12px/1.45 var(--mono);white-space:pre-wrap;overflow-wrap:anywhere}
.foot{color:var(--muted);font-size:13px;border-top:1px solid var(--border);padding-top:12px}
.foot p{margin:4px 0}
code{font-family:var(--mono);font-size:12.5px}.hash{overflow-wrap:anywhere;color:var(--text)}
@media (max-width:600px){.items li{flex-wrap:wrap}.value{text-align:left;max-width:none}.counters{width:100%}.counters div{flex:1;min-width:0}}
@media print{
  @page{size:letter;margin:14mm}
  *{-webkit-print-color-adjust:exact;print-color-adjust:exact}
  body{background:#fff;font-size:11pt}
  .page{max-width:none;padding:0}
  .search,.raw{display:none!important}
  [hidden]{display:revert!important}
  .card,.verdict{border-radius:8px;break-inside:avoid-page}
  .card:has(table){break-inside:auto}
  .items li,table.data tr{break-inside:avoid}
  thead{display:table-header-group}
  .sort{min-height:0;padding:6px 10px}
  th .sort::after{content:""!important}
  .table-wrap{overflow:visible}
}
"#;

/// Tri des tableaux (clic sur l'en-tête) et recherche. JS sans dépendance.
const JS: &str = r#"
(function(){
  "use strict";
  function norm(s){return s.normalize("NFD").replace(/[̀-ͯ]/g,"").toLowerCase();}
  // Nombre si la cellule est un nombre (espaces de milliers et virgule décimale acceptés,
  // unité courte en suffixe), sinon texte.
  var NUM=/^[-+]?\d+(?:[.,]\d+)?(?:%|°C|h|[kKMGT]?o|ms|s)?$/;
  function key(cell){
    var t=cell.textContent.trim();
    var c=t.replace(/[\s  ]/g,"");
    return NUM.test(c)?{n:parseFloat(c.replace(",",".")),t:t}:{n:NaN,t:t};
  }
  function cmp(a,b){
    var an=!isNaN(a.n),bn=!isNaN(b.n);
    if(an&&bn)return a.n-b.n;
    if(an)return -1;
    if(bn)return 1;
    return a.t.localeCompare(b.t,"fr",{numeric:true,sensitivity:"base"});
  }
  document.querySelectorAll("table.data").forEach(function(table){
    var ths=table.querySelectorAll("thead th");
    ths.forEach(function(th,col){
      th.querySelector("button").addEventListener("click",function(){
        var asc=th.getAttribute("aria-sort")!=="ascending";
        ths.forEach(function(o){o.setAttribute("aria-sort","none");});
        th.setAttribute("aria-sort",asc?"ascending":"descending");
        var body=table.tBodies[0];
        var rows=Array.prototype.slice.call(body.rows).map(function(r){return {r:r,k:key(r.cells[col]||r)};});
        rows.sort(function(x,y){var d=cmp(x.k,y.k);return asc?d:-d;});
        rows.forEach(function(x){body.appendChild(x.r);});
      });
    });
  });
  var q=document.getElementById("q"),count=document.getElementById("q-count");
  var all=Array.prototype.slice.call(document.querySelectorAll(".filterable"));
  var texts=all.map(function(el){return norm(el.textContent);});
  q.addEventListener("input",function(){
    var terms=norm(q.value).split(/\s+/).filter(Boolean),shown=0;
    all.forEach(function(el,i){
      var ok=terms.every(function(t){return texts[i].indexOf(t)!==-1;});
      el.hidden=!ok;if(ok)shown++;
    });
    count.textContent=terms.length?shown+" résultat"+(shown>1?"s":""):"";
  });
})();
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_all_special_characters() {
        assert_eq!(
            escape_html("<script>alert(\"x\")</script>"),
            "&lt;script&gt;alert(&quot;x&quot;)&lt;/script&gt;"
        );
        assert_eq!(escape_html("A & B"), "A &amp; B");
        assert_eq!(escape_html("l'été"), "l&#39;été");
        // Pas de double échappement : une entité déjà présente est un texte comme un autre.
        assert_eq!(escape_html("&amp;"), "&amp;amp;");
        assert_eq!(escape_html(""), "");
        assert_eq!(escape_html("Ça va ✓"), "Ça va ✓");
    }
}
