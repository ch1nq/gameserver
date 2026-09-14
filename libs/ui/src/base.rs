use maud::{DOCTYPE, Markup, PreEscaped, Render, html};

pub struct Base<'a> {
    pub title: &'a str,
    pub content: Markup,
}

/// Mockup token palette (`mockup/Landing.dc.html`). Single source of truth
/// for light/dark: `html[data-theme]` flips the vars, Tailwind classes
/// reference them via `var(--…)` so no `dark:` variants are needed.
const THEME_CSS: &str = r#"
  :root{
    --bg:#f1f1ef; --surface:#ffffff; --surface-2:#ffffff; --avatar:#dcdcd8; --row-hover:#f7f7f5;
    --ink:#16181c; --mid:#3d4046; --muted:#565a61; --faint:#8b8f97;
    --line:#d3d3cf; --line-soft:#e6e6e3; --hover:#e4e4e1; --fill:#e9e9e6;
    --accent:#3f5bd6; --accent-ink:#ffffff; --accent-hover:#2f45a8;
    --green:#146b45; --green-line:#a9d6bf; --pink:#c01f52; --pink-line:#e3adbc; --pink-soft:#b9808f;
    --yellow:#ffd84a; --brand:#e0338a; --brand-ink:#ffffff;
    --invert-bg:#16181c; --invert-ink:#f1f1ef; --term-bg:#16181c;
  }
  html[data-theme="dark"]{
    --bg:#101114; --surface:#191b1f; --surface-2:#16181b; --avatar:#2c2f36; --row-hover:#1e2024;
    --ink:#eef0f2; --mid:#c2c6cc; --muted:#9ba1a9; --faint:#7c8290;
    --line:#2e3138; --line-soft:#2a2d34; --hover:#24262c; --fill:#22242a;
    --accent:#7289ff; --accent-ink:#101114; --accent-hover:#b9c4ff;
    --green:#4cc38a; --green-line:#2f5d49; --pink:#ff7e9d; --pink-line:#5e2f3d; --pink-soft:#8a6070;
    --yellow:#ffd84a; --brand:#ff59a3; --brand-ink:#101114;
    --invert-bg:#eef0f2; --invert-ink:#101114; --term-bg:#0a0b10;
  }
  html, body { margin:0; padding:0; background:var(--bg); }
  * { box-sizing:border-box; }
  /* Layered so Tailwind utilities (e.g. text-[var(--muted)] on nav links)
     win over the element default: unlayered styles beat layered ones. */
  @layer base {
    a { color:var(--accent); text-decoration:none; }
    a:hover { color:var(--accent-hover); }
  }
  ::selection { background:#ffd84a; color:#16181c; }
  [data-icon-sun]{display:none}
  html[data-theme="dark"] [data-icon-sun]{display:block}
  html[data-theme="dark"] [data-icon-moon]{display:none}
"#;

/// Runs before paint so the persisted theme applies without a flash.
/// Mirrors the mockup `readTheme/applyTheme` logic.
const THEME_INIT_JS: &str = r#"
(function(){
  var theme = "light";
  try {
    if (localStorage.getItem("achtung-theme") === "dark") theme = "dark";
  } catch (e) {}
  document.documentElement.setAttribute("data-theme", theme);
  // The toggle button lives in <body>, parsed after this <head> script,
  // so sync its label once the DOM is ready.
  function sync(){ syncThemeButton(theme === "dark"); }
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", sync);
  } else { sync(); }
})();
function syncThemeButton(dark){
  var btn = document.querySelector("[data-theme-toggle]");
  if (!btn) return;
  btn.setAttribute("aria-label", dark ? "Switch to light theme" : "Switch to dark theme");
  btn.setAttribute("title", dark ? "Switch to light theme" : "Switch to dark theme");
}
function toggleAchtungTheme(){
  var el = document.documentElement;
  var next = el.getAttribute("data-theme") === "dark" ? "light" : "dark";
  el.setAttribute("data-theme", next);
  try { localStorage.setItem("achtung-theme", next); } catch (e) {}
  syncThemeButton(next === "dark");
}
"#;

impl<'a> Render for Base<'a> {
    fn render(&self) -> Markup {
        html! {
            (DOCTYPE)
            html {
                head {
                    meta charset="utf-8";
                    meta name="viewport" content="width=device-width, initial-scale=1";
                    title { (self.title) }
                    link href="https://cdn.jsdelivr.net/npm/flowbite@4.0.1/dist/flowbite.min.css" rel="stylesheet";
                    script src="https://cdn.jsdelivr.net/npm/@tailwindcss/browser@4" {}
                    link rel="preconnect" href="https://fonts.googleapis.com" {}
                    link rel="preconnect" href="https://fonts.gstatic.com" crossorigin {}
                    link href="https://fonts.googleapis.com/css2?family=Geologica:wght,CRSV@100..900,0&display=swap" rel="stylesheet" {}
                    style { (PreEscaped(THEME_CSS)) }
                    script { (PreEscaped(THEME_INIT_JS)) }
                }
                body style="background:var(--bg);color:var(--ink);font-family:Geologica,system-ui,sans-serif;" {
                    (self.content)
                    script src="https://cdn.jsdelivr.net/npm/flowbite@4.0.1/dist/flowbite.min.js" {};
                }
            }
        }
    }
}
