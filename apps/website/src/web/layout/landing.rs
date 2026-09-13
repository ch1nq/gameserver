//! Landing page sections, mirroring `mockup/Landing.dc.html`.
//!
//! Only renders data the backend actually has: bot names, `@author`
//! (via [`AgentWithAuthor`]) and the live spectator stream. Elo, lang,
//! win-rate and match counts render as em-dashes until ranking lands.

use achtung_core::agents::manager::AgentWithAuthor;
use achtung_ui::avatar::AuthorAvatar;
use achtung_ui::badge::LiveBadge;
use achtung_ui::code::{CodeTab, CodeTabs};
use maud::{Markup, Render, html};

pub struct HeroLive;

impl Render for HeroLive {
    fn render(&self) -> Markup {
        html! {
            section class="flex flex-wrap gap-5 items-start" {
                div class="flex-[2_1_460px] min-w-0 w-full max-w-[760px] aspect-square relative bg-[#0A0B10] border border-gray-300 dark:border-gray-700 rounded overflow-hidden" {
                    canvas id="achtung-canvas" width="1000" height="1000" class="absolute inset-0 w-full h-full block" {}
                    div id="spectator-result" class="hidden absolute inset-x-0 bottom-0 max-h-[45%] overflow-y-auto bg-gray-900/85 text-white text-sm p-3" {}
                }
                div class="flex-1 min-w-[280px] flex flex-col gap-8" {
                    div class="flex items-center gap-2.5 flex-wrap" {
                        (LiveBadge)
                        span id="spectator-tick" class="text-[13px] text-gray-600 dark:text-gray-400 font-semibold tabular-nums" {
                            "Waiting for a game…"
                        }
                    }
                    div class="flex flex-col gap-2" {
                        span class="text-[11px] font-bold tracking-[0.1em] uppercase text-gray-500 dark:text-gray-400" {
                            "Playing now"
                        }
                        ul id="spectator-legend" class="flex flex-col gap-1.5" {}
                    }
                    div class="flex flex-col gap-3" {
                        p class="font-semibold text-[15px] text-gray-900 dark:text-white" {
                            "Think you can beat these bots?"
                        }
                        a href="/agents/new" class="self-start font-semibold text-sm text-white bg-blue-700 hover:bg-blue-800 dark:bg-blue-600 dark:hover:bg-blue-700 px-4 py-2.5 rounded" {
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
            section id="board" class="border-t border-gray-300 dark:border-gray-700 pt-6 flex flex-col gap-3.5" {
                div class="flex items-baseline gap-3 flex-wrap" {
                    h2 class="m-0 font-[Geologica] font-semibold text-[19px] tracking-tight text-gray-900 dark:text-white" {
                        "Leaderboard"
                    }
                    span class="text-sm text-gray-500 dark:text-gray-400" {
                        (self.entries.len()) " bots ranked"
                    }
                }
                div class="bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded overflow-hidden overflow-x-auto" {
                    table class="w-full text-[13px] min-w-[740px] border-collapse" {
                        thead {
                            tr class="text-[11px] font-bold tracking-[0.1em] uppercase text-gray-500 dark:text-gray-400 border-b border-gray-200 dark:border-gray-700" {
                                th class="text-right font-bold px-2.5 py-2.5 pl-4 w-[50px]" { "#" }
                                th class="text-left font-bold px-3.5 py-2.5" { "Bot" }
                                th class="text-left font-bold px-3.5 py-2.5" { "Author" }
                                th class="text-left font-bold px-3.5 py-2.5" { "Lang" }
                                th class="text-right font-bold px-3.5 py-2.5" { "Elo" }
                                th class="text-right font-bold px-3.5 py-2.5" { "Win" }
                                th class="text-right font-bold px-3.5 py-2.5 pr-4" { "Matches" }
                            }
                        }
                        tbody {
                            @if self.entries.is_empty() {
                                tr class="border-t border-gray-100 dark:border-gray-700" {
                                    td colspan="7" class="px-4 py-4 text-center text-gray-500 dark:text-gray-400" {
                                        "No bots yet — upload the first one."
                                    }
                                }
                            }
                            @for (i, entry) in self.entries.iter().enumerate() {
                                tr class="border-t border-gray-100 dark:border-gray-700 hover:bg-gray-50 dark:hover:bg-gray-700" {
                                    td class="text-right px-2.5 py-3 pl-4 text-gray-500 dark:text-gray-400 tabular-nums" {
                                        (i + 1)
                                    }
                                    td class="px-3.5 py-3" {
                                        span class="text-gray-900 dark:text-white font-semibold" {
                                            (&*entry.agent.name)
                                        }
                                    }
                                    td class="px-3.5 py-3 text-gray-600 dark:text-gray-300" {
                                        span class="flex items-center gap-2" {
                                            (AuthorAvatar { username: &entry.username })
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
                p class="m-0 text-[13px] text-gray-500 dark:text-gray-400" {
                    "Elo and match stats land with ranking — new bots will start at 1200."
                }
            }
        }
    }
}

/// Left-aligned em-dash cell (Lang).
fn placeholder_cell() -> Markup {
    html! {
        td class="px-3.5 py-3 text-gray-400 dark:text-gray-500" { "—" }
    }
}

/// Right-aligned em-dash cell (Win, Matches).
fn placeholder_cell_right() -> Markup {
    html! {
        td class="text-right px-3.5 py-3 text-gray-400 dark:text-gray-500 tabular-nums" { "—" }
    }
}

/// Right-aligned bold em-dash cell (Elo keeps the column's emphasis).
fn placeholder_cell_right_bold() -> Markup {
    html! {
        td class="text-right px-3.5 py-3 font-bold text-gray-400 dark:text-gray-500 tabular-nums" { "—" }
    }
}

pub struct ExplainerSection;

impl Render for ExplainerSection {
    fn render(&self) -> Markup {
        html! {
            section class="border-t border-gray-300 dark:border-gray-700 pt-6 flex flex-col gap-3.5" {
                h2 class="m-0 font-[Geologica] font-semibold text-[19px] tracking-tight text-gray-900 dark:text-white" {
                    "Every curve up there is a program"
                }
                p class="m-0 text-[15px] leading-relaxed text-gray-600 dark:text-gray-300 max-w-[68ch]" {
                    "A bot is one function: it gets the board each tick and returns -1, 0 or 1 to steer. Upload one and it plays ranked rounds against everyone else's."
                }
                div class="flex gap-3.5 items-center flex-wrap pt-0.5" {
                    a href="/agents/new" class="font-semibold text-sm text-white bg-blue-700 hover:bg-blue-800 dark:bg-blue-600 dark:hover:bg-blue-700 px-4 py-2.5 rounded" {
                        "Upload an example bot"
                    }
                    a href="#bot-file" class="text-sm text-blue-700 dark:text-blue-400 font-semibold" {
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

impl Render for BotFileSection {
    fn render(&self) -> Markup {
        html! {
            section id="bot-file" class="border-t border-gray-300 dark:border-gray-700 pt-6 flex flex-col gap-3.5" {
                h2 class="m-0 font-[Geologica] font-semibold text-[19px] tracking-tight text-gray-900 dark:text-white" {
                    "A bot in one file"
                }
                p class="m-0 text-[15px] leading-relaxed text-gray-600 dark:text-gray-300 max-w-[68ch]" {
                    "Bots run as containers, so the language is up to you — the SDK just speaks the match protocol for you. Package the program as an OCI image and push it to the registry on this site; the CLI wraps the build and push into one command, and each push becomes a new version you can roll back to."
                }
                (CodeTabs {
                    group: "lang",
                    tabs: vec![
                        CodeTab { id: "python", label: "Python", code: PYTHON_CODE, install: PYTHON_INSTALL },
                        CodeTab { id: "rust", label: "Rust", code: RUST_CODE, install: RUST_INSTALL },
                        CodeTab { id: "javascript", label: "JavaScript", code: JS_CODE, install: JS_INSTALL },
                    ],
                })
                p class="m-0 text-[13px] text-gray-500 dark:text-gray-400" {
                    "Missing your language? "
                    a href="https://github.com" class="text-blue-700 dark:text-blue-400 font-semibold" {
                        "Open a PR"
                    }
                    "."
                }
            }
        }
    }
}
