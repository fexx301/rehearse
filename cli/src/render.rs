//! Render a saved report.json as one self-contained HTML page.
//!
//! No scripts, no web fonts, no external requests: the page opens offline and
//! can be attached to a pull request or an audit trail as a single file.
use serde_json::Value;
use std::fmt::Write;

// Design: Hallmark modern-minimal, Workbench structure, Cobalt palette (system fonts only).
const CSS: &str = r#":root {
  --color-paper: oklch(98.5% 0.004 250);
  --color-paper-2: oklch(96.2% 0.007 252);
  --color-ink: oklch(23% 0.02 258);
  --color-ink-2: oklch(36% 0.018 257);
  --color-muted: oklch(53% 0.014 257);
  --color-rule: oklch(90.5% 0.008 255);
  --color-rule-2: oklch(84% 0.01 255);
  --color-accent: oklch(57% 0.2 256);
  --color-danger: oklch(56% 0.2 27);
  --color-danger-soft: oklch(96% 0.022 27);
  --color-ok: oklch(52% 0.12 155);
  --color-warn: oklch(52% 0.13 70);
  --color-warn-soft: oklch(96% 0.035 80);
  --color-band: oklch(20.5% 0.02 262);
  --color-band-2: oklch(25.5% 0.022 262);
  --color-band-rule: oklch(33% 0.022 262);
  --color-band-ink: oklch(97% 0.006 255);
  --color-band-muted: oklch(72% 0.018 258);
  --color-band-accent: oklch(70% 0.17 256);
  --color-band-danger: oklch(70% 0.17 27);
  --color-band-ok: oklch(76% 0.13 155);
  --font-display: "SF Pro Display", "Segoe UI Variable Display", system-ui, sans-serif;
  --font-body: system-ui, -apple-system, "Segoe UI", Roboto, sans-serif;
  --font-mono: ui-monospace, "SF Mono", Menlo, Consolas, "Liberation Mono", monospace;
  --text-xs: 0.75rem; --text-sm: 0.875rem; --text-base: 1rem; --text-lg: 1.125rem;
  --text-xl: 1.5rem; --text-2xl: 2.25rem;
  --text-display: clamp(2.1rem, 2.6vw + 1rem, 3.6rem);
  --space-2xs: 0.25rem; --space-xs: 0.5rem; --space-sm: 0.75rem; --space-md: 1rem;
  --space-lg: 1.5rem; --space-xl: 2.25rem; --space-2xl: 3.5rem; --space-3xl: 5rem;
  --radius-sm: 6px; --radius-md: 10px;
}
@media (prefers-color-scheme: dark) {
  :root {
    --color-paper: oklch(17.5% 0.014 262);
    --color-paper-2: oklch(21.5% 0.016 262);
    --color-ink: oklch(96% 0.006 255);
    --color-ink-2: oklch(84% 0.01 255);
    --color-muted: oklch(67% 0.014 257);
    --color-rule: oklch(28% 0.016 260);
    --color-rule-2: oklch(35% 0.018 260);
    --color-accent: oklch(70% 0.16 256);
    --color-danger: oklch(71% 0.16 27);
    --color-danger-soft: oklch(26% 0.05 27);
    --color-ok: oklch(75% 0.12 155);
    --color-warn: oklch(78% 0.13 75);
    --color-warn-soft: oklch(27% 0.05 70);
    --color-band: oklch(11% 0.014 262);
    --color-band-2: oklch(19% 0.02 262);
    --color-band-rule: oklch(27% 0.02 262);
  }
}
* { box-sizing: border-box; }
html, body { overflow-x: clip; }
body { margin: 0; background: var(--color-paper); color: var(--color-ink-2);
  font: 400 var(--text-base)/1.55 var(--font-body); -webkit-font-smoothing: antialiased; }
.wrap { max-width: 76rem; margin: 0 auto; padding: 0 var(--space-xl); }
code, .mono { font-family: var(--font-mono); }
h1, h2, h3 { font-family: var(--font-display); font-style: normal; margin: 0; color: var(--color-ink); }

/* ---------- Hero band: the verdict and the figure ---------- */
.band { background: var(--color-band); color: var(--color-band-muted); border-bottom: 1px solid var(--color-band-rule);
}
.band .bar { display: flex; flex-wrap: wrap; align-items: baseline; gap: var(--space-xs) var(--space-xl);
  padding-block: var(--space-lg); border-bottom: 1px solid var(--color-band-rule);
  font: 500 var(--text-sm)/1.4 var(--font-mono); }
.wordmark { font: 700 var(--text-lg)/1 var(--font-display); color: var(--color-band-ink); letter-spacing: -0.02em; }
.wordmark span { color: var(--color-band-accent); }
.bar b { color: var(--color-band-ink); font-weight: 500; }
.hero.solo { grid-template-columns: minmax(0, 1fr); }
.hero.solo .lede, .hero.solo .tally { max-width: 52rem; }
.hero { display: grid; grid-template-columns: minmax(0, 0.95fr) minmax(0, 1.05fr); gap: var(--space-2xl);
  padding-block: var(--space-2xl) var(--space-2xl); align-items: start; }
.hero h1 { color: var(--color-band-ink); font-size: var(--text-display); font-weight: 600; line-height: 1.02;
  letter-spacing: -0.035em; overflow-wrap: anywhere; min-width: 0; }
.hero h1 .who { color: var(--color-band-danger); white-space: nowrap; }
.hero h1 .nw { white-space: nowrap; }
.hero .lede { margin: var(--space-lg) 0 0; font-size: var(--text-lg); line-height: 1.5; max-width: 46ch; }
.tally { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); margin: var(--space-xl) 0 0;
  border-top: 1px solid var(--color-band-rule); }
.tally div { padding: var(--space-md) var(--space-md) 0 0; min-width: 0; }
.tally dt { font: 500 var(--text-sm)/1.3 var(--font-mono); color: var(--color-band-ink); white-space: nowrap;
  overflow: hidden; text-overflow: ellipsis; }
.tally dd { margin: var(--space-xs) 0 0; font-size: var(--text-sm); }
.tally dd strong { display: block; font: 600 var(--text-2xl)/1 var(--font-display); letter-spacing: -0.03em;
  color: var(--color-band-ink); margin-bottom: var(--space-2xs); font-variant-numeric: tabular-nums; }
.tally .bad strong { color: var(--color-band-danger); }
.tally .ok strong { color: var(--color-band-ok); }

.figure { margin: 0; background: var(--color-band-2); border: 1px solid var(--color-band-rule);
  border-radius: var(--radius-md); padding: var(--space-lg); }
.figure figcaption { margin-bottom: var(--space-lg); }
.figure figcaption strong { display: block; color: var(--color-band-ink); font: 600 var(--text-lg)/1.25 var(--font-display); letter-spacing: -0.01em; }
.figure figcaption .loss { display: block; margin-top: var(--space-2xs); font-size: var(--text-sm); }
.figure figcaption .loss b { color: var(--color-band-danger); font-weight: 600; }
.legend { display: flex; flex-wrap: wrap; gap: var(--space-xs) var(--space-lg); margin-top: var(--space-sm);
  font: 500 var(--text-xs)/1.6 var(--font-mono); color: var(--color-band-muted); }
.legend span::before { content: ""; display: inline-block; width: 0.9rem; height: 0.55rem; border-radius: 2px;
  margin-right: 0.45rem; vertical-align: 0.02em; background: var(--swatch); }
.holder { display: grid; grid-template-columns: 8.5rem minmax(0, 1fr); gap: var(--space-xs) var(--space-md); align-items: center;
  padding-block: var(--space-sm); border-top: 1px solid var(--color-band-rule); }
.holder:first-of-type { border-top: 0; padding-top: 0; }
.holder h3 { font: 500 var(--text-sm)/1.25 var(--font-body); color: var(--color-band-ink); }
.holder h3 code { display: block; color: var(--color-band-muted); font-size: var(--text-xs); font-weight: 400; margin-top: 0.15rem; }
.bars { display: grid; gap: 0.35rem; }
.track { display: grid; grid-template-columns: minmax(0, 1fr) 7.25rem; align-items: center; gap: var(--space-sm); }
.track .rail { height: 0.65rem; border-radius: 2px; background: oklch(100% 0 0 / 0.07); overflow: hidden; }
.track .fill { display: block; height: 100%; border-radius: 2px; background: var(--swatch); }
.track .val.same { color: var(--color-band-ok); font-weight: 500; }
.track .val { font: 500 var(--text-sm)/1 var(--font-mono); color: var(--color-band-ink); text-align: right;
  font-variant-numeric: tabular-nums; white-space: nowrap; }
.track.zero .rail { background: oklch(100% 0 0 / 0.05); box-shadow: inset 4px 0 0 var(--color-band-danger); }
.track.zero .val { color: var(--color-band-danger); font-weight: 700; }
.sr { position: absolute; width: 1px; height: 1px; overflow: hidden; clip: rect(0 0 0 0); white-space: nowrap; }
.claim { margin: var(--space-md) 0 0; font-size: var(--text-xs); color: var(--color-band-muted); }

/* ---------- Light sections ---------- */
section { padding-block: var(--space-2xl); }
section + section { border-top: 1px solid var(--color-rule); }
.head { display: grid; gap: var(--space-xs); margin-bottom: var(--space-xl); max-width: 64ch; }
.head h2 { font-size: var(--text-xl); font-weight: 600; letter-spacing: -0.02em; line-height: 1.15; }
.head p { margin: 0; color: var(--color-muted); }
.scroll { overflow-x: auto; border: 1px solid var(--color-rule-2); border-radius: var(--radius-md); background: var(--color-paper); }
table { width: 100%; border-collapse: collapse; font-size: var(--text-sm); table-layout: fixed; }
th, td { padding: var(--space-sm) var(--space-md); border-bottom: 1px solid var(--color-rule); text-align: left; vertical-align: middle; }
tbody tr:last-child > * { border-bottom: 0; }
thead th { background: var(--color-paper-2); font: 600 var(--text-sm)/1.3 var(--font-mono); color: var(--color-ink); }
.cnt-bad { color: var(--color-danger); font-weight: 600; } .cnt-ok { color: var(--color-ok); font-weight: 600; }
thead th small { display: block; font-weight: 400; font-size: var(--text-xs); color: var(--color-muted); }
col.idx { width: 3rem; } col.call { width: 34%; }
.idx { color: var(--color-muted); font: 500 var(--text-xs)/1 var(--font-mono); text-align: right; padding-right: 0; }
tbody th { font-weight: 500; color: var(--color-ink); }
tbody th small { display: block; margin-top: 0.15rem; font: 400 var(--text-xs)/1.4 var(--font-mono); color: var(--color-muted); overflow-wrap: anywhere; }
.num { text-align: right; font-family: var(--font-mono); font-variant-numeric: tabular-nums; overflow-wrap: anywhere; }
td.long { text-align: left; white-space: normal; font-size: var(--text-xs); line-height: 1.6; }
td.all-same { color: var(--color-muted); }
td.all-same summary { cursor: pointer; font: 500 var(--text-sm)/1.4 var(--font-body); color: var(--color-ok); }
td.all-same summary:focus-visible { outline: 2px solid var(--color-accent); outline-offset: 2px; }
td.all-same code { display: block; margin-top: var(--space-xs); font-size: var(--text-xs); line-height: 1.6; color: var(--color-ink-2); overflow-wrap: anywhere; }
td.diff { background: var(--color-danger-soft); color: var(--color-danger); font-weight: 700;
  box-shadow: inset 3px 0 0 var(--color-danger); }
td small.why { white-space: nowrap; }
td small.why, td s { display: block; margin-top: 0.15rem; font-weight: 400; font-size: 0.8125rem; color: var(--color-ink-2); }
td s { text-decoration-color: color-mix(in oklch, var(--color-danger) 55%, transparent); text-decoration-thickness: 1.5px; }
td.consequence { background: var(--color-warn-soft); color: var(--color-warn); font-weight: 700; box-shadow: inset 3px 0 0 var(--color-warn); }

.pairs { border: 1px solid var(--color-rule-2); border-radius: var(--radius-md); overflow: hidden; background: var(--color-paper); }
.pairs .row { display: grid; grid-template-columns: minmax(0, 1fr) 7.5rem minmax(0, 1fr); align-items: center;
  border-top: 1px solid var(--color-rule); font: 500 var(--text-sm)/1.4 var(--font-mono); }
.pairs .row > * { padding: var(--space-sm) var(--space-lg); overflow-wrap: anywhere; }
.pairs .row.top { border-top: 0; background: var(--color-paper-2); font: 600 var(--text-sm)/1.3 var(--font-display); color: var(--color-ink); }
.pairs .row.top span:last-child { color: var(--color-danger); }
.pairs .stored { color: var(--color-ink); }
.pairs .stored em, .pairs .looked em { display: block; font-style: normal; font: 400 var(--text-xs)/1.4 var(--font-body); color: var(--color-muted); }
.pairs .looked { color: var(--color-danger); background: var(--color-danger-soft); box-shadow: inset 3px 0 0 var(--color-danger); height: 100%; display: flex; flex-direction: column; justify-content: center; }
.pairs .looked em { color: var(--color-danger); }
.pairs .link { text-align: center; color: var(--color-danger); font: 600 var(--text-xs)/1.2 var(--font-body); white-space: nowrap; }
.after { margin-top: var(--space-xl); }
.after h3 { font: 600 var(--text-base)/1.3 var(--font-display); margin-bottom: var(--space-2xs); }
.after p { margin: 0 0 var(--space-sm); font-size: var(--text-sm); color: var(--color-muted); max-width: 70ch; }

.cover { margin: 0; font-size: var(--text-lg); color: var(--color-ink-2); max-width: 60ch; line-height: 1.8; }
.cover b { font: 600 1.25em/1 var(--font-display); color: var(--color-ink); letter-spacing: -0.02em; }
.cover b.ok { color: var(--color-ok); } .cover b.bad { color: var(--color-danger); }
details { margin-top: var(--space-lg); }
details summary { cursor: pointer; color: var(--color-ink-2); font-weight: 500; font-size: var(--text-sm); }
details summary:focus-visible { outline: 2px solid var(--color-accent); outline-offset: 3px; border-radius: 2px; }
details ul { font: 400 var(--text-xs)/1.8 var(--font-mono); overflow-wrap: anywhere; padding-left: var(--space-lg); color: var(--color-muted); }

footer { background: var(--color-paper-2); border-top: 1px solid var(--color-rule); padding-block: var(--space-xl) var(--space-2xl);
  font-size: var(--text-sm); color: var(--color-muted); }
footer dl { display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: var(--space-xs) var(--space-xl); margin: 0 0 var(--space-lg); }
footer dd { margin: 0; font: 400 var(--text-xs)/1.6 var(--font-mono); color: var(--color-ink-2); overflow-wrap: anywhere; }
footer p { margin: 0; max-width: 80ch; }

@media (max-width: 900px) {
  .hero { grid-template-columns: minmax(0, 1fr); gap: var(--space-xl); }
}
@media (max-width: 600px) {
  .wrap { padding: 0 var(--space-md); }
  .figure { padding: var(--space-md); }
  .holder { grid-template-columns: minmax(0, 1fr); }
  .legend { display: grid; gap: 0.15rem; }
  .track { grid-template-columns: minmax(0, 1fr) 6.75rem; }
  .tally dd strong { font-size: var(--text-xl); }
  .tally { grid-template-columns: minmax(0, 1fr); }
  .tally div { display: grid; grid-template-columns: minmax(0, 1fr) auto; align-items: baseline; gap: var(--space-md);
    padding: var(--space-sm) 0; border-bottom: 1px solid var(--color-band-rule); }
  .tally dt { hyphens: none; }
  .tally dd { margin: 0; text-align: right; display: flex; align-items: baseline; gap: var(--space-xs); }
  .tally dd strong { display: inline; margin: 0; }
  /* Workflow and storage tables become one card per row; every value keeps its version label. */
  .stack, .stack tbody, .stack tr, .stack th, .stack td { display: block; }
  .stack colgroup, .stack thead { display: none; }
  .stack tr { border-bottom: 1px solid var(--color-rule); padding: var(--space-sm) var(--space-md); }
  .stack tbody tr:last-child { border-bottom: 0; }
  .stack th, .stack td { border: 0; padding: 0.3rem 0; }
  .stack .idx { display: none; }
  .stack td { display: flex; justify-content: space-between; gap: var(--space-md); text-align: right; }
  .stack td::before { content: attr(data-v); font: 500 var(--text-xs)/1.6 var(--font-mono); color: var(--color-muted); text-align: left; flex: none; }
  .stack td.diff, .stack td.consequence { padding: 0.35rem var(--space-sm); margin: 0.2rem calc(-1 * var(--space-sm)); border-radius: 4px; flex-wrap: wrap; }
  .stack td.diff::before { color: var(--color-danger); }
  .stack td s, .stack td small.why { flex-basis: 100%; }
  .pairs { border: 0; background: none; display: grid; gap: var(--space-xs); }
  .pairs .row { grid-template-columns: minmax(0, 1fr); border: 1px solid var(--color-rule-2); border-radius: var(--radius-sm); overflow: hidden; background: var(--color-paper); }
  .pairs .row.top { display: none; }
  .pairs .link { text-align: left; padding-block: 0; }
  .pairs .link .arr { display: inline-block; transform: rotate(90deg); }
  footer dl { grid-template-columns: minmax(0, 1fr); gap: 0; }
  footer dd { margin-bottom: var(--space-sm); }
}
@media print { .band { background: none; color: black; } .scroll { overflow: visible; } }
"#;

/// Styles for the authorization and event sections. Emitted only in reports that have them,
/// so reports without them keep their exact bytes.
const EXTRA_CSS: &str = r#"<style>
table.changes td { font: 500 var(--text-xs)/1.6 var(--font-mono); overflow-wrap: anywhere; vertical-align: top; }
table.changes td ul { margin: 0; padding: 0; list-style: none; display: grid; gap: var(--space-xs); }
table.changes td .none { color: var(--color-muted); font-family: var(--font-body); }
table.changes td.diff { font-weight: 600; }
@media (max-width: 600px) {
  .stack.changes td { display: block; text-align: left; }
  .stack.changes td::before { display: block; margin-bottom: 0.2rem; }
}
</style>"#;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Shorten 56-character G…/C… strkeys to `GA5U…RAL3`, keeping everything else.
fn short_keys(s: &str) -> String {
    let mut out = String::new();
    let mut token = String::new();
    let flush = |token: &mut String, out: &mut String| {
        let is_key = token.len() == 56
            && (token.starts_with('G') || token.starts_with('C'))
            && token.chars().all(|c| c.is_ascii_uppercase() || ('2'..='7').contains(&c));
        if is_key {
            out.push_str(&token[..4]);
            out.push('…');
            out.push_str(&token[52..]);
        } else {
            out.push_str(token);
        }
        token.clear();
    };
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            token.push(c);
        } else {
            flush(&mut token, &mut out);
            out.push(c);
        }
    }
    flush(&mut token, &mut out);
    out
}

struct Units {
    decimals: u32,
    symbol: String,
    calls: Vec<String>,
}

impl Units {
    fn from(report: &Value) -> Option<Self> {
        let d = &report["manifest"]["display"];
        Some(Units {
            decimals: d["decimals"].as_u64()? as u32,
            symbol: d["symbol"].as_str()?.to_string(),
            calls: d["amount_calls"].as_array()?.iter().filter_map(|v| v.as_str().map(String::from)).collect(),
        })
    }

    fn amount(&self, raw: &str) -> Option<String> {
        let v: i128 = raw.parse().ok()?;
        let scale = 10i128.pow(self.decimals);
        let (neg, v) = (v < 0, v.abs());
        let whole = (v / scale).to_string();
        let mut grouped = String::new();
        for (i, c) in whole.chars().enumerate() {
            if i > 0 && (whole.len() - i) % 3 == 0 {
                grouped.push(',');
            }
            grouped.push(c);
        }
        let mut frac = format!("{:0width$}", v % scale, width = self.decimals as usize);
        while frac.len() > 2 && frac.ends_with('0') {
            frac.pop();
        }
        Some(format!("{}{grouped}.{frac} {}", if neg { "−" } else { "" }, self.symbol))
    }
}

fn shown(units: &Option<Units>, call: &str, raw: &str) -> String {
    if let Some(u) = units {
        if u.calls.iter().any(|c| c == call) {
            if let Some(a) = u.amount(raw) {
                return a;
            }
        }
    }
    match raw {
        "()" => "ok".into(),
        r => r.strip_prefix("error: ").map(|e| format!("failed: {e}")).unwrap_or_else(|| r.to_string()),
    }
}

fn state_value(units: &Option<Units>, key: &str, v: &Value) -> String {
    match v.as_str() {
        None => "absent".into(),
        Some(raw) => match units {
            Some(u) if !key.starts_with("instance ") => u.amount(raw).unwrap_or_else(|| raw.to_string()),
            _ => raw.to_string(),
        },
    }
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap_or("")
}

fn arr(v: &Value) -> &[Value] {
    v.as_array().map(Vec::as_slice).unwrap_or(&[])
}

fn group(n: u64) -> String {
    let t = n.to_string();
    let mut g = String::new();
    for (i, c) in t.chars().enumerate() {
        if i > 0 && (t.len() - i) % 3 == 0 {
            g.push(',');
        }
        g.push(c);
    }
    g
}

fn hash8(v: &Value) -> String {
    s(v, "wasm_sha256").chars().take(8).collect()
}

/// The storage key part of a readable ledger key ("C… persistent Balance(G…)" -> "Balance(G…)").
fn key_tail(k: &str) -> &str {
    k.rsplit(' ').next().unwrap_or(k)
}

/// The argument inside a key like "Balance(GA5U…)", used to pair renamed keys.
fn key_arg(k: &str) -> &str {
    let t = key_tail(k);
    t.split_once('(').map(|(_, r)| r.trim_end_matches(')')).unwrap_or(t)
}

/// A result cell's main line and optional second line. Failures split the
/// readable error name from its code.
fn cell(units: &Option<Units>, call: &str, raw: &str) -> (String, Option<String>) {
    if let Some(e) = raw.strip_prefix("error: ") {
        return match e.split_once(" (") {
            Some((name, code)) => (name.to_string(), Some(code.trim_end_matches(')').to_string() + ")")),
            None => ("call failed".into(), Some(e.to_string())),
        };
    }
    (shown(units, call, raw), None)
}

/// Short version label for tight spots: "baseline (deployed)" -> "deployed".
fn sentence(l: &str) -> String {
    let mut c = l.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

fn short_label(l: &str) -> &str {
    l.split_once('(').map(|(_, r)| r.trim_end_matches(')')).unwrap_or(l)
}

pub fn render(report: &Value) -> String {
    let units = Units::from(report);
    let cap = &report["capture"];
    let base = &report["baseline"];
    // Diverged candidates first: the evidence sits next to the baseline everywhere.
    let mut sorted: Vec<&Value> = arr(&report["candidates"]).iter().collect();
    sorted.sort_by_key(|c| s(c, "status") != "diverged");
    let cands: &[&Value] = &sorted;
    let steps = arr(&base["steps"]);
    let n_steps = steps.len();
    let ledger_fmt = group(cap["ledger"].as_u64().unwrap_or(0));
    let diverged: Vec<&Value> = cands.iter().copied().filter(|c| s(c, "status") == "diverged").collect();
    let base_short = short_label(s(base, "label"));
    let has_figure = units.as_ref().is_some_and(|u| steps.first().is_some_and(|st| u.calls.iter().any(|c| c == s(st, "call"))));

    let via_upgrade = cands.iter().any(|c| c["installed_via_upgrade"].is_object());
    let failed_upgrades: Vec<&Value> = cands.iter().copied().filter(|c| s(c, "status") == "upgrade failed").collect();
    let has_auth_or_events = via_upgrade || cands.iter().any(|c| !arr(&c["auth_differences"]).is_empty() || !arr(&c["event_differences"]).is_empty());
    let mut h = String::new();
    let _ = write!(h, r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="color-scheme" content="light dark">
<title>Rehearse report for {contract_short}</title>
<style>{CSS}</style>{EXTRA}</head><body>
<div class="band"><div class="wrap">
<header class="bar"><span class="wordmark">rehearse<span>.</span></span>
<span>contract <b title="{contract}">{contract_short}</b></span><span>{network} ledger <b>{ledger_fmt}</b></span><span>protocol <b>{protocol}</b></span></header>
<div class="hero{solo}"><div>
"#,
        contract = esc(s(cap, "contract")),
        contract_short = esc(&short_keys(s(cap, "contract"))),
        network = esc(s(cap, "network")),
        protocol = cap["protocol"],
        solo = if has_figure { "" } else { " solo" },
        EXTRA = if has_auth_or_events { EXTRA_CSS } else { "" },
    );

    match diverged.as_slice() {
        [] if !failed_upgrades.is_empty() => {
            let names: Vec<String> = failed_upgrades.iter().map(|c| esc(s(c, "label"))).collect();
            let _ = write!(h, r#"<h1>The upgrade to <span class="who">{}</span> failed.</h1>"#, names.join(", "));
        }
        [] => h.push_str(r#"<h1>No candidate changes what this workflow sees.</h1>"#),
        [one] => {
            let (n, a, e) = (arr(&one["step_differences"]).len(), arr(&one["auth_differences"]).len(), arr(&one["event_differences"]).len());
            // Only the count stays on one line; the lead-in may wrap.
            let (lead, count) = if n > 0 || (a == 0 && e == 0) {
                (String::new(), format!("{n} of {n_steps} results"))
            } else if a > 0 {
                ("who must authorize ".to_string(), format!("{a} of {n_steps} calls"))
            } else {
                ("the events of ".to_string(), format!("{e} of {n_steps} calls"))
            };
            let _ = write!(h, r#"<h1>Upgrading to <span class="who">{}</span> changes {}<span class="nw">{}.</span></h1>"#,
                esc(s(one, "label")), esc(&lead), esc(&count));
        }
        many => {
            let _ = write!(h, r#"<h1><span class="who">{} of {}</span> candidates change what this workflow sees.</h1>"#, many.len(), cands.len());
        }
    }
    let _ = write!(h, r#"<p class="lede">Rehearse replayed the same {n_steps}-step workflow against the deployed contract and each candidate. Every run started from identical {} state, captured at ledger {ledger_fmt}.{}</p>
<dl class="tally">"#, esc(s(cap, "network")),
        if via_upgrade { " Each candidate went in through the contract's own upgrade function, not a direct code swap." } else { "" });
    let fails = arr(&base["expectation_failures"]).len();
    let _ = write!(h, r#"<div class="{}"><dt>{}</dt><dd><strong>{}/{n_steps}</strong>expectations met</dd></div>"#,
        if fails == 0 { "ok" } else { "bad" }, esc(base_short), n_steps - fails.min(n_steps));
    for c in cands {
        let n = arr(&c["step_differences"]).len();
        let unverified = s(c, "status").starts_with("unverified");
        let upgrade_failed = s(c, "status") == "upgrade failed";
        let (a, e) = (arr(&c["auth_differences"]).len(), arr(&c["event_differences"]).len());
        let class = if n > 0 || a > 0 || e > 0 || unverified || upgrade_failed { "bad" } else { "ok" };
        let (shown, word) = if upgrade_failed {
            (0, "upgrade failed; results are from the deployed code".to_string())
        } else if unverified {
            (n, "unverified reads".to_string())
        } else if n == 0 && a > 0 {
            (a, format!("of {n_steps} calls changed who must authorize"))
        } else if n == 0 && e > 0 {
            (e, format!("of {n_steps} calls changed their events"))
        } else {
            (n, format!("of {n_steps} results changed"))
        };
        let _ = write!(h, r#"<div class="{class}"><dt>{}</dt><dd><strong>{shown}</strong>{word}</dd></div>"#, esc(s(c, "label")));
    }
    h.push_str("</dl>");
    if !has_figure {
        let _ = write!(h, r#"<p class="claim">{}</p>"#, esc(s(report, "claim")));
    }
    h.push_str("</div>\n");

    // Figure: holder balances as every version reads them, before any write.
    let amount_steps: Vec<(usize, &Value)> = steps
        .iter()
        .enumerate()
        .take_while(|(_, st)| units.as_ref().is_some_and(|u| u.calls.iter().any(|c| c == s(st, "call"))))
        .collect();
    if let (Some(u), false) = (&units, amount_steps.is_empty()) {
        let raw = |v: &str| v.parse::<i128>().ok();
        let max = amount_steps.iter().filter_map(|(_, st)| raw(s(st, "result"))).max().unwrap_or(1).max(1);
        let versions: Vec<(&str, &Value, &str)> = std::iter::once((base_short, base, "var(--color-band-accent)"))
            .chain(cands.iter().map(|c| {
                let sw = if s(c, "status") == "diverged" { "var(--color-band-danger)" } else { "var(--color-band-ok)" };
                (s(c, "label"), *c, sw)
            }))
            .collect();
        h.push_str(r#"<figure class="figure" aria-label="Holder balances as each version reads them"><figcaption><strong>Holder balances, as each version reads them</strong>"#);
        if let Some(f) = diverged.first() {
            let (mut lost, mut holders) = (0i128, 0);
            for (i, st) in &amount_steps {
                let b = raw(s(st, "result")).unwrap_or(0);
                if b > 0 && raw(s(&arr(&f["steps"])[*i], "result")) == Some(0) {
                    lost += b;
                    holders += 1;
                }
            }
            if holders > 0 {
                let _ = write!(h, r#"<span class="loss">Under {}, <b>{} across {} holder{}</b> reads as zero.</span>"#,
                    esc(s(f, "label")), esc(&u.amount(&lost.to_string()).unwrap_or_default()), holders, if holders == 1 { "" } else { "s" });
            }
        }
        h.push_str(r#"<span class="legend">"#);
        for (label, v, sw) in &versions {
            let _ = (v, ());
            let _ = write!(h, r#"<span style="--swatch: {sw}">{}</span>"#, esc(label));
        }
        h.push_str("</span></figcaption>\n");
        for (i, st) in &amount_steps {
            let arg = arr(&st["args"]).first().and_then(|a| a.as_str()).map(short_keys).unwrap_or_default();
            let name = s(st, "label").strip_suffix(" balance").map(|n| { let mut c = n.chars(); c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default() }).unwrap_or_else(|| s(st, "label").to_string());
            let _ = write!(h, r#"<div class="holder"><h3>{}<code>{}</code></h3><div class="bars">"#, esc(&name), esc(&arg));
            let b = s(st, "result");
            for (label, v, sw) in &versions {
                let r = if std::ptr::eq(*v, base) { b } else { s(&arr(&v["steps"])[*i], "result") };
                let x = raw(r).unwrap_or(0);
                let zero = x == 0 && raw(b).unwrap_or(0) > 0;
                let pct = x.max(0) as f64 / max as f64 * 100.0;
                let same = !std::ptr::eq(*v, base) && r == b;
                let shown_val = if same { "= deployed".to_string() } else { u.amount(r).unwrap_or_else(|| r.to_string()) };
                let _ = write!(h, r#"<div class="track{}" style="--swatch: {sw}"><span class="rail"><span class="fill" style="width: {:.1}%"></span></span><span class="val{}"><span class="sr">{}: </span>{}</span></div>"#,
                    if zero { " zero" } else { "" }, pct, if same { " same" } else { "" }, esc(label), esc(&shown_val));
            }
            h.push_str("</div></div>\n");
        }
        let _ = write!(h, r#"<p class="claim">{}</p></figure>"#, esc(s(report, "claim")));
    }
    h.push_str("</div></div></div>\n<main class=\"wrap\">\n");

    // Workflow table (cards on narrow screens).
    let _ = write!(h, r#"<section aria-labelledby="steps"><div class="head"><h2 id="steps">The workflow, call by call</h2>
<p>Red cells differ from the deployed contract; the struck value is what it returned.</p></div>
<div class="scroll"><table class="stack"><colgroup><col class="idx"><col class="call">"#);
    for _ in 0..=cands.len() {
        h.push_str("<col>");
    }
    let _ = write!(h, r#"</colgroup><thead><tr><th class="idx" scope="col">#</th><th scope="col">Call</th><th scope="col" class="num">{}<small>{}</small></th>"#,
        esc(base_short), esc(&hash8(base)));
    for c in cands {
        let n = arr(&c["step_differences"]).len();
        let _ = write!(h, r#"<th scope="col" class="num">{}<small>{}, <span class="{}">{n} changed</span></small></th>"#,
            esc(s(c, "label")), esc(&hash8(c)), if n > 0 { "cnt-bad" } else { "cnt-ok" });
    }
    h.push_str("</tr></thead><tbody>\n");
    for (i, b) in steps.iter().enumerate() {
        let call = s(b, "call");
        let args: Vec<String> = arr(&b["args"]).iter().filter_map(|a| a.as_str()).map(short_keys).collect();
        let (bm, bs) = cell(&units, call, s(b, "result"));
        let all_same = cands.iter().all(|c| s(&arr(&c["steps"])[i], "result") == s(b, "result"));
        if all_same && bm.chars().count() > 48 {
            let _ = write!(h, r#"<tr><td class="idx">{}</td><th scope="row">{}<small>{}({})</small></th><td class="all-same" colspan="{}" data-v="all versions"><details><summary>Identical in all {} versions</summary><code>{}</code></details></td></tr>
"#, i + 1, esc(&sentence(s(b, "label"))), esc(call), esc(&args.join(", ")), cands.len() + 1, cands.len() + 1, esc(&short_keys(&bm)));
            continue;
        }
        let long = |m: &str| if m.chars().count() > 48 { " long" } else { "" };
        let _ = write!(h, r#"<tr><td class="idx">{}</td><th scope="row">{}<small>{}({})</small></th><td class="num{}" data-v="{}">{}{}</td>"#,
            i + 1, esc(&sentence(s(b, "label"))), esc(call), esc(&args.join(", ")), long(&bm), esc(base_short), esc(&short_keys(&bm)),
            bs.map(|x| format!(r#"<small class="why">{}</small>"#, esc(&x))).unwrap_or_default());
        for c in cands {
            let r = s(&arr(&c["steps"])[i], "result");
            let (m, sub) = cell(&units, call, r);
            if r == s(b, "result") {
                let _ = write!(h, r#"<td class="num{}" data-v="{}">{}</td>"#, long(&m), esc(s(c, "label")), esc(&short_keys(&m)));
            } else {
                let second = match sub {
                    Some(x) => format!(r#"<small class="why">{}</small>"#, esc(&x)),
                    None => format!("<s>{}</s>", esc(&bm)),
                };
                let _ = write!(h, r#"<td class="num diff{}" data-v="{}">{}{second}</td>"#, long(&m), esc(s(c, "label")), esc(&short_keys(&m)));
            }
        }
        h.push_str("</tr>\n");
    }
    h.push_str("</tbody></table></div></section>\n");

    // Diagnosis: each stored key opposite the key the candidate reads instead.
    let stored: Vec<&str> = arr(&cap["keys_captured"])
        .iter()
        .filter_map(|k| k.as_str())
        .filter(|k| k.contains(" persistent ") && !k.ends_with(" instance"))
        .collect();
    for c in diverged.iter().filter(|c| !arr(&c["state_differences"]).is_empty() || !arr(&c["reads_absent_on_chain"]).is_empty()) {
        let label = s(c, "label");
        let mut absent: Vec<&str> = arr(&c["reads_absent_on_chain"]).iter().filter_map(|k| k.as_str()).collect();
        absent.sort_by_key(|k| key_arg(k).to_string());
        let mut pairs: Vec<(Option<&str>, Option<&str>)> = Vec::new();
        let mut used = vec![false; stored.len()];
        for a in &absent {
            let m = stored.iter().enumerate().find(|(j, st)| !used[*j] && key_arg(st) == key_arg(a));
            match m {
                Some((j, st)) => {
                    used[j] = true;
                    pairs.push((Some(*st), Some(*a)));
                }
                None => pairs.push((None, Some(*a))),
            }
        }
        for (j, st) in stored.iter().enumerate() {
            if !used[j] {
                pairs.push((Some(*st), None));
            }
        }
        let renamed = pairs.iter().filter(|(a, b)| a.is_some() && b.is_some()).count();
        let lede = if renamed > 0 {
            format!("The data is still there; {label} looks under keys that are empty at ledger {ledger_fmt}.")
        } else {
            format!("{label} reads storage keys that hold nothing on {}. Checked at ledger {ledger_fmt}.", s(cap, "network"))
        };
        let _ = write!(h, r#"<section aria-labelledby="why-{id}"><div class="head"><h2 id="why-{id}">Why {l} diverges</h2><p>{}</p></div>
<div class="pairs"><div class="row top"><span>Where the data is</span><span></span><span>Where {l} looks</span></div>"#,
            esc(&lede), id = esc(label), l = esc(label));
        for (st, ab) in &pairs {
            let left = st.map(|k| format!(r#"<span class="stored" title="{}">{}<em>stored on-chain</em></span>"#, esc(k), esc(&short_keys(key_tail(k))))).unwrap_or_else(|| "<span></span>".into());
            let link = if st.is_some() && ab.is_some() { r#"<span class="link">renamed <span class="arr">→</span></span>"# } else { "<span></span>" };
            let right = ab.map(|k| format!(r#"<span class="looked" title="{}">{}<em>absent on-chain</em></span>"#, esc(k), esc(&short_keys(key_tail(k))))).unwrap_or_else(|| "<span></span>".into());
            let _ = write!(h, r#"<div class="row">{left}{link}{right}</div>"#);
        }
        h.push_str("</div>\n");

        let diffs = arr(&c["state_differences"]);
        if !diffs.is_empty() {
            let failed = arr(&c["steps"]).iter().enumerate().find(|(_, st)| st["failed"].as_bool() == Some(true));
            let note = match failed {
                Some((i, _)) => format!("Step {} failed under {label}, so its writes never happened.", i + 1),
                None => format!("Entries that end with different values under {label}."),
            };
            let _ = write!(h, r#"<div class="after"><h3>Storage after the workflow</h3><p>{}</p><div class="scroll"><table class="stack"><thead><tr><th scope="col">Key</th><th scope="col" class="num">{}</th><th scope="col" class="num">{}</th></tr></thead><tbody>"#,
                esc(&note), esc(base_short), esc(label));
            for d in diffs {
                let key = s(d, "key");
                let _ = write!(h, r#"<tr><th scope="row" class="mono" title="{}">{}</th><td class="num" data-v="{}">{}</td><td class="num consequence" data-v="{}">{}</td></tr>"#,
                    esc(key), esc(&short_keys(key_tail(key))), esc(base_short), esc(&state_value(&units, key, &d["baseline"])), esc(label), esc(&state_value(&units, key, &d["candidate"])));
            }
            h.push_str("</tbody></table></div></div>\n");
        }
        h.push_str("</section>\n");
    }

    // How each candidate was installed, when the manifest asks for the upgrade path.
    if via_upgrade {
        let _ = write!(h, r#"<section aria-labelledby="installed"><div class="head"><h2 id="installed">How each candidate was installed</h2><p>Through the deployed contract's own upgrade function, with the candidate's Wasm hash, then any migration calls, before the workflow ran.</p></div>
<div class="scroll"><table class="stack changes"><colgroup><col class="call"><col><col></colgroup><thead><tr><th scope="col">Candidate</th><th scope="col">Call and result</th><th scope="col">Required authorization</th></tr></thead><tbody>
"#);
        for c in cands.iter().filter(|c| c["installed_via_upgrade"].is_object()) {
            let iv = &c["installed_via_upgrade"];
            let calls = std::iter::once(&iv["upgrade"]).chain(arr(&iv["migrate"]).iter());
            let mut what = Vec::new();
            let mut who = Vec::new();
            for st in calls {
                let args: Vec<String> = arr(&st["args"]).iter().filter_map(|a| a.as_str()).map(short_keys).collect();
                let res = if st["failed"].as_bool() == Some(true) { s(st, "result").trim_start_matches("error: ").to_string() } else { "ok".to_string() };
                what.push(format!("<li>{}({}): {}</li>", esc(s(st, "call")), esc(&args.join(", ")), esc(&res)));
                for a in arr(&st["auths"]).iter().filter_map(|x| x.as_str()) {
                    who.push(format!("<li>{}</li>", esc(&short_keys(a))));
                }
            }
            let ok = iv["candidate_code_installed"].as_bool() == Some(true);
            let installed = if ok { "candidate code running".to_string() } else { "candidate code not installed".to_string() };
            let _ = write!(h, r#"<tr><th scope="row">{}<small>{}</small></th><td{} data-v="call">{}<span class="none">{}</span></td><td data-v="authorization">{}</td></tr>
"#, esc(s(c, "label")), esc(&hash8(c)), if ok { "" } else { r#" class="diff""# }, format!("<ul>{}</ul>", what.join("")), esc(&installed),
                if who.is_empty() { r#"<span class="none">None required</span>"#.to_string() } else { format!("<ul>{}</ul>", who.join("")) });
        }
        h.push_str("</tbody></table></div></section>\n");
    }

    // Authorization and event changes, compared on calls where both versions succeeded.
    for (key, title, lede, empty) in [
        ("auth_differences", "Who must authorize", "Authorizations each call required, recorded while signatures are mocked. Compared where both versions succeeded.", "No authorization required"),
        ("event_differences", "Events emitted", "Contract events each call emitted. Compared where both versions succeeded.", "No events"),
    ] {
        for c in cands.iter().filter(|c| !arr(&c[key]).is_empty()) {
            let label = s(c, "label");
            let _ = write!(h, r#"<section aria-labelledby="{key}-{id}"><div class="head"><h2 id="{key}-{id}">{title}: {l}</h2><p>{lede}</p></div>
<div class="scroll"><table class="stack changes"><colgroup><col class="call"><col><col></colgroup><thead><tr><th scope="col">Call</th><th scope="col">{b}</th><th scope="col">{l}</th></tr></thead><tbody>
"#, id = esc(label), l = esc(label), b = esc(base_short));
            let list = |v: &Value| -> String {
                let items: Vec<String> = arr(v).iter().filter_map(|x| x.as_str()).map(|x| format!("<li>{}</li>", esc(&short_keys(x)))).collect();
                if items.is_empty() { format!(r#"<span class="none">{empty}</span>"#) } else { format!("<ul>{}</ul>", items.join("")) }
            };
            for d in arr(&c[key]) {
                let _ = write!(h, r#"<tr><th scope="row">{}<small>step {}</small></th><td data-v="{}">{}</td><td class="diff" data-v="{}">{}</td></tr>
"#, esc(&sentence(s(d, "label"))), d["step"], esc(base_short), list(&d["baseline"]), esc(label), list(&d["candidate"]));
            }
            h.push_str("</tbody></table></div></section>\n");
        }
    }

    // Coverage.
    let captured = arr(&cap["keys_captured"]);
    let absent_all = arr(&cap["keys_verified_absent"]);
    let outside: usize = cands.iter().map(|c| arr(&c["reads_outside_capture"]).len()).sum::<usize>()
        + arr(&base["reads_outside_capture"]).len();
    let _ = write!(h, r#"<section aria-labelledby="coverage"><div class="head"><h2 id="coverage">What the capture covers</h2>
<p>Every key any version read was captured or confirmed absent at the same ledger.</p></div>
<p class="cover"><b>{}</b> ledger entries captured, <b>{}</b> keys verified absent,<br><b class="{}">{}</b> reads outside the capture, and <b class="ok">0</b> network calls during replay.</p>
<details><summary>Show every captured and absent key</summary><ul>"#,
        captured.len(), absent_all.len(), if outside == 0 { "ok" } else { "bad" }, outside);
    for k in captured {
        let _ = write!(h, "<li>{}</li>", esc(&short_keys(k.as_str().unwrap_or(""))));
    }
    for k in absent_all {
        let _ = write!(h, "<li>{}: absent</li>", esc(&short_keys(k.as_str().unwrap_or(""))));
    }
    h.push_str("</ul></details></section>\n</main>\n");

    let rt = &report["runtime"];
    let _ = write!(h, r#"<footer><div class="wrap"><dl>
<dt>Snapshot SHA-256</dt><dd>{}</dd>
<dt>{} Wasm</dt><dd>{}</dd>
"#, esc(s(cap, "snapshot_sha256")), esc(base_short), esc(s(base, "wasm_sha256")));
    for c in cands {
        let _ = write!(h, "<dt>{} Wasm</dt><dd>{}</dd>\n", esc(s(c, "label")), esc(s(c, "wasm_sha256")));
    }
    let _ = write!(h, r#"<dt>Ledger close time</dt><dd>{} (unix)</dd>
<dt>Runtime</dt><dd>soroban-sdk {}, soroban-env-host {}</dd>
<dt>Authorization</dt><dd>{}</dd>
</dl><p>Generated by {} from report.json. {}</p></div></footer>
</body></html>
"#,
        cap["ledger_close_time"],
        esc(s(rt, "soroban_sdk")),
        esc(s(rt, "soroban_env_host")),
        if rt["auth_mocked"].as_bool() == Some(true) { "mocked: every require_auth is satisfied; signatures are not checked" } else { "enforced" },
        esc(s(report, "tool")),
        esc(&report["manifest"]["description"].as_str().map(|d| format!("Workflow: {d}")).unwrap_or_default()),
    );
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rhd() -> Option<Units> {
        Some(Units { decimals: 7, symbol: "RHD".into(), calls: vec!["balance".into()] })
    }

    #[test]
    fn only_full_strkeys_are_shortened() {
        assert_eq!(
            short_keys("Balance(GA5U3PK2JZAO6O443KOYFDELNZMC6IYAM3HIDMLRS7N5RT2RSERSRAL3)"),
            "Balance(GA5U…RAL3)"
        );
        assert_eq!(short_keys("CCFM7DDZ3DFKKLDJ335J47WDEVZJALLIVNVDJZYXNUB5T75YWZK6XIRI"), "CCFM…XIRI");
        assert_eq!(short_keys("BalanceOf(short) 12400000000"), "BalanceOf(short) 12400000000");
    }

    #[test]
    fn amounts_use_the_manifest_units() {
        let u = rhd().unwrap();
        assert_eq!(u.amount("12400000000").unwrap(), "1,240.00 RHD");
        assert_eq!(u.amount("752500000").unwrap(), "75.25 RHD");
        assert_eq!(u.amount("1").unwrap(), "0.0000001 RHD");
        assert_eq!(u.amount("-30000000000").unwrap(), "−3,000.00 RHD");
        assert!(u.amount("not a number").is_none());
    }

    #[test]
    fn results_are_shown_in_reader_terms() {
        assert_eq!(cell(&rhd(), "balance", "12400000000").0, "1,240.00 RHD");
        assert_eq!(cell(&rhd(), "decimals", "7").0, "7");
        assert_eq!(cell(&rhd(), "transfer", "()").0, "ok");
        let (main, sub) = cell(&rhd(), "transfer", "error: InsufficientBalance (Error(Contract, #2))");
        assert_eq!(main, "InsufficientBalance");
        assert_eq!(sub.as_deref(), Some("Error(Contract, #2)"));
    }
}
