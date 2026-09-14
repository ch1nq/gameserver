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
            section class="flex flex-wrap gap-[22px] items-start" {
                div class="flex-[2_1_460px] min-w-0 w-full max-w-[760px] aspect-square relative bg-[#0A0B10] border border-[var(--line)] rounded overflow-hidden" {
                    canvas id="achtung-canvas" width="1000" height="1000" class="absolute inset-0 w-full h-full block" {}
                }
                div class="flex-1 min-w-[280px] flex flex-col gap-[34px]" {
                    div class="flex items-center gap-2.5 flex-wrap" {
                        (Badge { label: "Live" })
                        span id="spectator-round" class="text-[13px] text-[var(--mid)] font-semibold tabular-nums" {
                            "Round 1 · ranked"
                        }
                    }
                    div class="flex flex-col gap-3" {
                        span class="text-[11px] font-bold tracking-[0.1em] uppercase text-[var(--muted)]" {
                            "Playing now"
                        }
                        ul id="spectator-legend" class="flex flex-col gap-[5px]" {}
                    }
                    div class="flex flex-col gap-3.5" {
                        p class="font-semibold text-[15px] tracking-[-0.015em] text-[var(--ink)]" {
                            "Think you can beat these bots?"
                        }
                        a href="/agents/new" class="self-start font-semibold text-sm text-[var(--accent-ink)] bg-[var(--accent)] hover:bg-[var(--accent-hover)] px-4 py-2.5 rounded-[3px]" style="color:var(--accent-ink);" {
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
            section id="board" class="border-t border-[var(--line)] pt-[26px] flex flex-col gap-3.5" {
                div class="flex items-baseline gap-3 flex-wrap" {
                    h2 class="m-0 font-[Geologica] font-semibold text-[19px] tracking-[-0.025em] text-[var(--ink)]" {
                        "Leaderboard"
                    }
                    span class="text-sm text-[var(--muted)]" {
                        (self.entries.len()) " bots ranked"
                    }
                }
                div class="bg-[var(--surface)] border border-[var(--line)] rounded overflow-hidden overflow-x-auto" {
                    table class="w-full text-[13px] min-w-[740px] border-collapse" {
                        thead {
                            tr class="text-[11px] font-bold tracking-[0.1em] uppercase text-[var(--muted)] border-b border-[var(--line)]" {
                                th class="text-right font-bold px-2.5 py-[11px] pl-[18px] w-[50px]" { "#" }
                                th class="text-left font-bold px-3.5 py-[11px]" { "Bot" }
                                th class="text-left font-bold px-3.5 py-[11px]" { "Author" }
                                th class="text-left font-bold px-3.5 py-[11px]" { "Lang" }
                                th class="text-right font-bold px-3.5 py-[11px]" { "Elo" }
                                th class="text-right font-bold px-3.5 py-[11px]" { "Win" }
                                th class="text-right font-bold pl-3.5 pr-[18px] py-[11px]" { "Matches" }
                            }
                        }
                        tbody {
                            @if self.entries.is_empty() {
                                tr class="border-t border-[var(--line-soft)]" {
                                    td colspan="7" class="px-4 py-4 text-center text-[var(--muted)]" {
                                        "No bots yet — upload the first one."
                                    }
                                }
                            }
                            @for (i, entry) in self.entries.iter().enumerate() {
                                tr class="border-t border-[var(--line-soft)] hover:bg-[var(--row-hover)]" {
                                    td class="text-right px-2.5 py-[13px] pl-[18px] text-[var(--muted)] tabular-nums" {
                                        (i + 1)
                                    }
                                    td class="px-3.5 py-[13px]" {
                                        span class="text-[var(--ink)] font-semibold" {
                                            (&*entry.agent.name)
                                        }
                                    }
                                    td class="px-3.5 py-[13px] text-[var(--mid)]" {
                                        span class="flex items-center gap-2" {
                                            (github_avatar(&entry.username))
                                            span { "@" (&*entry.username) }
                                        }
                                    }
                                    (placeholder_cell())
                                    (placeholder_cell_right_bold())
                                    (placeholder_cell_right())
                                    (placeholder_cell_right_last())
                                }
                            }
                        }
                    }
                }
                p class="m-0 text-[13px] text-[var(--muted)]" {
                    "New bots start at 1200 and stay provisional for 20 matches."
                }
            }
        }
    }
}

/// Left-aligned em-dash cell (Lang).
fn placeholder_cell() -> Markup {
    html! {
        td class="px-3.5 py-[13px] text-[var(--muted)]" { "—" }
    }
}

/// Right-aligned em-dash cell (Win, Matches).
fn placeholder_cell_right() -> Markup {
    html! {
        td class="text-right px-3.5 py-[13px] text-[var(--muted)] tabular-nums" { "—" }
    }
}

/// Last column keeps the mockup's extra right padding.
fn placeholder_cell_right_last() -> Markup {
    html! {
        td class="text-right pl-3.5 pr-[18px] py-[13px] text-[var(--muted)] tabular-nums" { "—" }
    }
}

/// Right-aligned bold em-dash cell (Elo keeps the column's emphasis).
fn placeholder_cell_right_bold() -> Markup {
    html! {
        td class="text-right px-3.5 py-[13px] font-bold text-[var(--muted)] tabular-nums" { "—" }
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
            section class="border-t border-[var(--line)] pt-[26px] flex flex-col gap-3.5" {
                h2 class="m-0 font-[Geologica] font-semibold text-[19px] tracking-[-0.025em] text-[var(--ink)]" {
                    "Every curve up there is a program"
                }
                p class="m-0 text-[15px] leading-[1.6] text-[var(--mid)] max-w-[68ch]" {
                    "A bot is one function: it gets the board each tick and returns -1, 0 or 1 to steer. Upload one and it plays ranked rounds against everyone else's, with its Elo moving after each result."
                }
                div class="flex gap-3.5 items-center flex-wrap pt-0.5" {
                    a href="/agents/new" class="font-semibold text-sm text-[var(--accent-ink)] bg-[var(--accent)] hover:bg-[var(--accent-hover)] px-4 py-2.5 rounded-[3px]" style="color:var(--accent-ink);" {
                        "Upload an example bot"
                    }
                    a href="#bot-file" class="text-sm text-[var(--accent)] font-semibold" {
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
        div class="flex flex-col gap-2 pt-1" {
            span class="text-[13px] text-[var(--muted)]" {
                "Install the CLI once, then build and push the image:"
            }
            (CodeBlock { code: install })
        }
    }
}

impl Render for BotFileSection {
    fn render(&self) -> Markup {
        html! {
            section id="bot-file" class="border-t border-[var(--line)] pt-[26px] flex flex-col gap-3.5" {
                h2 class="m-0 font-[Geologica] font-semibold text-[19px] tracking-[-0.025em] text-[var(--ink)]" {
                    "A bot in one file"
                }
                p class="m-0 text-[15px] leading-[1.6] text-[var(--mid)] max-w-[68ch]" {
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
                p class="m-0 text-[13px] text-[var(--muted)]" {
                    "Missing your language? "
                    a href="https://github.com" class="text-[var(--accent)] font-semibold" {
                        "Open a PR"
                    }
                    "."
                }
            }
        }
    }
}
