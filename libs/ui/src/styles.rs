/// Stylesheet contract for `achtung-ui` markup.
///
/// The library owns its component styles; the host application owns its page
/// styles. The host must serve [`CSS`] at [`PATH`] (the website does this with
/// an explicit `/static/ui.css` route) — [`base::Base`](crate::base::Base)
/// already emits that `<link>`, followed by whatever the caller passes via
/// `head_extra` (e.g. the app stylesheet, which may reference the `:root`
/// vars defined here and therefore must load *after* this file).
pub const PATH: &str = "/static/ui.css";

/// Component stylesheet bytes (see `components.css` next to this module).
pub const CSS: &str = include_str!("components.css");
