use maud::{Markup, Render, html};

/// Small round avatar with a text fallback underneath.
///
/// If `src` is `Some` and the image fails to load, `onerror` removes it
/// so the fallback stays readable. Generic: only strings + size, no user
/// or provider types — provider helpers below are pure string functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AvatarSize {
    #[default]
    Standard,
    Large,
}

pub struct Avatar<'a> {
    pub src: Option<&'a str>,
    pub fallback: &'a str,
    pub size: AvatarSize,
}

impl<'a> Avatar<'a> {
    pub fn small(src: Option<&'a str>, fallback: &'a str) -> Self {
        Self {
            src,
            fallback,
            size: AvatarSize::Standard,
        }
    }

    pub fn large(src: Option<&'a str>, fallback: &'a str) -> Self {
        Self {
            src,
            fallback,
            size: AvatarSize::Large,
        }
    }
}

impl Render for Avatar<'_> {
    fn render(&self) -> Markup {
        let class = match self.size {
            AvatarSize::Standard => "avatar",
            AvatarSize::Large => "avatar is-lg",
        };
        html! {
            span class=(class) {
                span class="avatar-fallback" {
                    (self.fallback)
                }
                @if let Some(src) = self.src {
                    img src=(src) alt="" loading="lazy" onerror="this.remove()" class="avatar-img" {}
                }
            }
        }
    }
}

/// Pure helpers so apps don't duplicate the GitHub image URL format.
/// Still standalone: no app/core imports, just string manipulation.
pub fn github_avatar_url(username: &str, size_px: u32) -> String {
    format!("https://github.com/{username}.png?size={size_px}")
}

pub fn github_avatar_url_default(username: &str) -> String {
    format!("https://github.com/{username}.png")
}

pub fn fallback_initial(username: &str) -> String {
    username
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string())
}

/// Generic GitHub avatar: builds the image URL + initial fallback,
/// then renders [`Avatar`]. Takes only a username string, so the ui
/// crate stays dependency-free.
pub struct GithubAvatar<'a> {
    pub username: &'a str,
    pub size: AvatarSize,
}

impl<'a> GithubAvatar<'a> {
    pub fn small(username: &'a str) -> Self {
        Self {
            username,
            size: AvatarSize::Standard,
        }
    }

    pub fn large(username: &'a str) -> Self {
        Self {
            username,
            size: AvatarSize::Large,
        }
    }
}

impl Render for GithubAvatar<'_> {
    fn render(&self) -> Markup {
        let (size_px, avatar_size) = match self.size {
            AvatarSize::Standard => (40, AvatarSize::Standard),
            AvatarSize::Large => (64, AvatarSize::Large),
        };
        let src = github_avatar_url(self.username, size_px);
        let initial = fallback_initial(self.username);
        html! {
            (Avatar { src: Some(&src), fallback: &initial, size: avatar_size })
        }
    }
}
