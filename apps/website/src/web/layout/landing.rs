//! Landing page sections, mirroring `mockup/Landing.dc.html`.
//!
//! Renders live Weng-Lin ratings (`LeaderboardEntry`): Elo-scale mu ± sigma
//! plus win / match counts, best first. Bots under [`PROVISIONAL_MATCHES`]
//! games render a provisional marker. Bot names and `@author` come from
//! the same rows.

use achtung_core::matches::{LeaderboardEntry, PROVISIONAL_MATCHES};
use achtung_ui::avatar::GithubAvatar;
use achtung_ui::badge::Badge;
use achtung_ui::button::{AccentLink, Primary};
use achtung_ui::code::CodeBlock;
use achtung_ui::section::{ActionsRow, Lede, Note, Section, SectionHead};
use achtung_ui::table::{Cell, EmptyRow, HeaderCell, Row, Table};
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
                        (Primary { text: "Enter the competition", url: "/agents/new", icon: None })
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
    pub entries: &'a [LeaderboardEntry],
}

impl Render for LeaderboardSection<'_> {
    fn render(&self) -> Markup {
        html! {
            (Section {
                id: Some("board"),
                content: html! {
                    (SectionHead {
                        title: html! { "Leaderboard" },
                        sub: None,
                    })
                    (Table {
                        headers: vec![
                            HeaderCell::numeric("#"),
                            HeaderCell::plain("Bot"),
                            HeaderCell::plain("Author"),
                            HeaderCell::plain("Lang"),
                            HeaderCell::numeric("Rating"),
                            HeaderCell::numeric("Win"),
                            HeaderCell::numeric("Matches"),
                        ],
                        rows: html! {
                            @if self.entries.is_empty() {
                                (EmptyRow { colspan: 7, message: "No bots yet — upload the first one." })
                            }
                            @for (i, entry) in self.entries.iter().enumerate() {
                                @let rating_title = if entry.is_provisional() {
                                    format!("provisional — uncertainty ±{:.0}", entry.uncertainty)
                                } else {
                                    format!("uncertainty ±{:.0}", entry.uncertainty)
                                };
                                @let provisional_title = format!("fewer than {PROVISIONAL_MATCHES} matches");
                                (Row {
                                    content: html! {
                                        (Cell::numeric(html! { (i + 1) }))
                                        (Cell::primary(html! { (&*entry.agent.name) }))
                                        (Cell::plain(html! {
                                            span class="row-meta" {
                                                (GithubAvatar::small(&entry.username))
                                                span { "@" (&*entry.username) }
                                            }
                                        }))
                                        (Cell::plain(html! { "—" }))
                                        (Cell::numeric_primary(html! {
                                            span title=(rating_title) {
                                                (entry.formatted_rating())
                                            }
                                            @if entry.is_provisional() {
                                                span class="provisional" title=(provisional_title) {
                                                    "provisional"
                                                }
                                            }
                                        }))
                                        (Cell::numeric(html! {
                                            @if entry.matches_played > 0 {
                                                (format!("{:.0}%", 100.0 * entry.wins as f64 / entry.matches_played as f64))
                                            } @else {
                                                "—"
                                            }
                                        }))
                                        (Cell::numeric(html! { (entry.matches_played) }))
                                    }
                                })
                            }
                        },
                        extra_classes: None,
                    })
                    (Note {
                        content: html! { "New bots start at 1500 ± 500 and stay provisional for 20 matches." }
                    })
                }
            })
        }
    }
}

pub struct ExplainerSection;

impl Render for ExplainerSection {
    fn render(&self) -> Markup {
        html! {
            (Section {
                id: None,
                content: html! {
                    (SectionHead {
                        title: html! { "Every curve up there is a program" },
                        sub: None,
                    })
                    (Lede {
                        content: html! { "A bot is one function: it gets the board each tick and returns -1, 0 or 1 to steer. Upload one and it plays ranked rounds against everyone else's, with its Weng-Lin rating moving after each result." }
                    })
                    (ActionsRow {
                        content: html! {
                            (Primary { text: "Upload an example bot", url: "/agents/new", icon: None })
                            (AccentLink { text: "Read the docs", url: "#bot-file" })
                        }
                    })
                }
            })
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
            (Section {
                id: Some("bot-file"),
                content: html! {
                    (SectionHead {
                        title: html! { "A bot in one file" },
                        sub: None,
                    })
                    (Lede {
                        content: html! { "Bots run as containers, so the language is up to you — the SDK just speaks the match protocol for you. Package the program as an OCI image and push it to the registry on this site; the CLI wraps the build and push into one command, and each push becomes a new version you can roll back to." }
                    })
                    (Tabs {
                        group: "lang",
                        tabs: vec![
                            Tab { id: "python", label: "Python", content: code_panel(PYTHON_CODE, PYTHON_INSTALL) },
                            Tab { id: "rust", label: "Rust", content: code_panel(RUST_CODE, RUST_INSTALL) },
                            Tab { id: "javascript", label: "JavaScript", content: code_panel(JS_CODE, JS_INSTALL) },
                        ],
                    })
                    (Note {
                        content: html! {
                            "Missing your language? "
                            (AccentLink { text: "Open a PR", url: "https://github.com" })
                            "."
                        }
                    })
                }
            })
        }
    }
}
