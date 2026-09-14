//! Landing page sections, mirroring `mockup/Landing.dc.html`.
//!
//! Only renders data the backend actually has: bot names, `@author`
//! (via [`AgentWithAuthor`]) and the live spectator stream. Elo, lang,
//! win-rate and match counts render as em-dashes until ranking lands.

use achtung_core::agents::manager::AgentWithAuthor;
use achtung_ui::avatar::Avatar;
use achtung_ui::badge::Badge;
use achtung_ui::code::CodeBlock;
use achtung_ui::tabs::{Tab, Tabs};
use maud::{Markup, Render, html};

pub struct HeroLive;

impl Render for HeroLive {
    fn render(&self) -> Markup {
        html! {
            section class="hero" {
                div class="hero-media" {
                    canvas id="achtung-canvas" width="1000" height="1000" class="hero-canvas" {}
                }
                div class="hero-side" {
                    div class="live-row" {
                        (Badge { label: "Live" })
                        span id="spectator-round" class="round-label" {
                            "Round 1 · ranked"
                        }
                    }
                    div class="stack" {
                        span class="eyebrow" {
                            "Playing now"
                        }
                        ul id="spectator-legend" class="legend" {}
                    }
                    div class="cta-col" {
                        p class="cta-title" {
                            "Think you can beat these bots?"
                        }
                        a href="/agents/new" class="btn" {
                            "Enter the competition"
                        }
                    }
                }
                // Scripts last: init_spectator grabs the ids above at call
                // time, so they must already be parsed.
                script src="/static/spectator.js" {};
                script { "init_spectator('achtung-canvas');" };
            }
        }
    }
}

pub struct LeaderboardSection<'a> {
    pub entries: &'a [AgentWithAuthor],
}

impl Render for LeaderboardSection<'_> {
    fn render(&self) -> Markup {
        html! {
            section id="board" class="section" {
                div class="section-head" {
                    h2 class="section-title" {
                        "Leaderboard"
                    }
                    span class="section-sub" {
                        (self.entries.len()) " bots ranked"
                    }
                }
                div class="tbl-wrap" {
                    table class="tbl" {
                        thead {
                            tr {
                                th class="num" { "#" }
                                th { "Bot" }
                                th { "Author" }
                                th { "Lang" }
                                th class="num" { "Elo" }
                                th class="num" { "Win" }
                                th class="num" { "Matches" }
                            }
                        }
                        tbody {
                            @if self.entries.is_empty() {
                                tr {
                                    td colspan="7" class="center" {
                                        "No bots yet — upload the first one."
                                    }
                                }
                            }
                            @for (i, entry) in self.entries.iter().enumerate() {
                                tr {
                                    td class="num" {
                                        (i + 1)
                                    }
                                    td class="primary" {
                                        (&*entry.agent.name)
                                    }
                                    td {
                                        span class="row-meta" {
                                            (github_avatar(&entry.username))
                                            span { "@" (&*entry.username) }
                                        }
                                    }
                                    (placeholder_cell())
                                    (placeholder_cell_right_bold())
                                    (placeholder_cell_right())
                                    (placeholder_cell_right())
                                }
                            }
                        }
                    }
                }
                p class="note" {
                    "New bots start at 1200 and stay provisional for 20 matches."
                }
            }
        }
    }
}

/// Left-aligned em-dash cell (Lang).
fn placeholder_cell() -> Markup {
    html! {
        td { "—" }
    }
}

/// Right-aligned em-dash cell (Win, Matches).
fn placeholder_cell_right() -> Markup {
    html! {
        td class="num" { "—" }
    }
}

/// Right-aligned bold em-dash cell (Elo keeps the column's emphasis).
fn placeholder_cell_right_bold() -> Markup {
    html! {
        td class="num primary" { "—" }
    }
}

/// GitHub-specific avatar: builds the `github.com/{}.png` URL and the
/// initial fallback, then renders the generic [`Avatar`].
fn github_avatar(username: &str) -> Markup {
    let initial = username
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string());
    let src = format!("https://github.com/{username}.png?size=40");
    html! {
        (Avatar { src: Some(&src), fallback: &initial })
    }
}

pub struct ExplainerSection;

impl Render for ExplainerSection {
    fn render(&self) -> Markup {
        html! {
            section class="section" {
                h2 class="section-title" {
                    "Every curve up there is a program"
                }
                p class="lede" {
                    "A bot is one function: it gets the board each tick and returns -1, 0 or 1 to steer. Upload one and it plays ranked rounds against everyone else's, with its Elo moving after each result."
                }
                div class="actions-row" {
                    a href="/agents/new" class="btn" {
                        "Upload an example bot"
                    }
                    a href="#bot-file" class="link-accent" {
                        "Read the docs"
                    }
                }
            }
        }
    }
}

const PYTHON_CODE: &str = "from achtung import Bot, run\n\nclass Hugger(Bot):\n    def step(self, view):\n        # turn away when something is close ahead\n        if view.distance_ahead() < 20:\n            return 1 if view.distance_right() > view.distance_left() else -1\n        return 0\n\nrun(Hugger())";
const PYTHON_INSTALL: &str = "$ pip install achtung-cli\n$ achtung init --python        # writes a Dockerfile\n$ achtung push                 # builds the image, pushes to registry.achtung.bot";

const RUST_CODE: &str = "use achtung::{run, View};\n\nfn step(view: &View) -> i8 {\n    // turn away when something is close ahead\n    if view.distance_ahead() < 20.0 {\n        if view.distance_right() > view.distance_left() { 1 } else { -1 }\n    } else {\n        0\n    }\n}\n\nfn main() {\n    run(step);\n}";
const RUST_INSTALL: &str = "$ cargo install achtung-cli\n$ achtung init --rust          # writes a Dockerfile\n$ achtung push                 # builds the image, pushes to registry.achtung.bot";

const JS_CODE: &str = "import { run } from \"achtung\";\n\nrun((view) => {\n  // turn away when something is close ahead\n  if (view.distanceAhead() < 20) {\n    return view.distanceRight() > view.distanceLeft() ? 1 : -1;\n  }\n  return 0;\n});";
const JS_INSTALL: &str = "$ npm i -g achtung-cli\n$ achtung init --node          # writes a Dockerfile\n$ achtung push                 # builds the image, pushes to registry.achtung.bot";

pub struct BotFileSection;

fn code_panel(code: &str, install: &str) -> Markup {
    html! {
        (CodeBlock { code })
        div class="install-note" {
            span {
                "Install the CLI once, then build and push the image:"
            }
            (CodeBlock { code: install })
        }
    }
}

impl Render for BotFileSection {
    fn render(&self) -> Markup {
        html! {
            section id="bot-file" class="section" {
                h2 class="section-title" {
                    "A bot in one file"
                }
                p class="lede" {
                    "Bots run as containers, so the language is up to you — the SDK just speaks the match protocol for you. Package the program as an OCI image and push it to the registry on this site; the CLI wraps the build and push into one command, and each push becomes a new version you can roll back to."
                }
                (Tabs {
                    group: "lang",
                    tabs: vec![
                        Tab { id: "python", label: "Python", content: code_panel(PYTHON_CODE, PYTHON_INSTALL) },
                        Tab { id: "rust", label: "Rust", content: code_panel(RUST_CODE, RUST_INSTALL) },
                        Tab { id: "javascript", label: "JavaScript", content: code_panel(JS_CODE, JS_INSTALL) },
                    ],
                })
                p class="note" {
                    "Missing your language? "
                    a href="https://github.com" class="link-accent" {
                        "Open a PR"
                    }
                    "."
                }
            }
        }
    }
}
