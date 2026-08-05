//! `generate { manifest { } }`: `manifest.webmanifest` and its icons.

use kdl::KdlNode;

use crate::config::dispatch::Kind::{Choice, Lines, Number, Text};
use crate::config::dispatch::{Attributed, Attrs, Block, Section};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;
use crate::config::{Config, Named};
use crate::error::Result;

/// `manifest.webmanifest` generation ([the web app manifest][spec]): what a
/// browser reads when a visitor installs the site to a home screen. Enabled by
/// the presence of a `generate { manifest }` block.
///
/// Everything here is what only the author knows. What the build already knows
/// (the site title, the language's root URL) is filled in from the config it is
/// written for, so a bare `manifest { }` beside an icon is a valid manifest.
///
/// [spec]: https://www.w3.org/TR/appmanifest/
#[derive(Debug, Clone, Hash, Default)]
pub struct ManifestConfig {
    /// Whether to emit `manifest.webmanifest`.
    pub enabled: bool,
    /// The installed app's name. Defaults to the site title in the language the
    /// manifest is written for.
    pub name: Option<String>,
    /// The name a launcher falls back to when the full one does not fit.
    pub short: Option<String>,
    /// One line about the app, shown by an install prompt.
    pub description: Option<String>,
    /// How the installed app is presented.
    pub display: DisplayMode,
    /// CSS colour of the browser UI around the app, also written to every
    /// page's `<meta name="theme-color">` so a tab is tinted before any install.
    pub theme: Option<String>,
    /// CSS colour painted before the first page has rendered.
    pub background: Option<String>,
    /// Where launching the installed app lands, as a root-relative path.
    /// Localized per language, like the default it replaces: `/home/` launches
    /// the French app into `/fr/home/`. Defaults to the language's root.
    pub start: Option<String>,
    /// The URLs the installed app covers; navigating outside it leaves the app.
    /// Localized the same way, since a `start_url` outside its `scope` is a
    /// manifest a browser refuses. Defaults to the language's root.
    pub scope: Option<String>,
    /// The icons a launcher picks from. A manifest with none cannot be
    /// installed, so a build that emits one warns.
    pub icons: Vec<IconConfig>,
}

impl ManifestConfig {
    /// The output file name, at the root of each language's scope. The
    /// `.webmanifest` extension is the one the spec registers; `manifest.json`
    /// is the older spelling, and browsers accept both.
    pub const FILE: &'static str = "manifest.webmanifest";

    /// The manifest of a language, root-relative and under the site's base
    /// path: what that language's pages point `<link rel="manifest">` at.
    ///
    /// Beside [`FILE`](Self::FILE) for the reason
    /// [`FeedConfig::url`](crate::config::FeedConfig::url) sits beside its file
    /// name: the processor that writes the file and the tag that names
    /// it derive the path once, so they cannot drift. Root-relative rather than
    /// absolute, so a manifest is reachable without a configured site `url`.
    pub fn url(config: &Config, lang: &str) -> String {
        let scope = config.scope(lang, "");
        let path = match scope.is_empty() {
            true => format!("/{}", Self::FILE),
            false => format!("/{scope}/{}", Self::FILE),
        };
        config.prefixed(&path)
    }
}

/// One entry of a manifest's `icons` array.
#[derive(Debug, Clone, Hash)]
pub struct IconConfig {
    /// Where the image is served from, root-relative, exactly as a browser will
    /// request it. Written as the node's name: `"/icon-512.png" size=512`.
    pub src: String,
    /// The square edge in pixels. Absent means the image scales to any size,
    /// which is what a vector icon does.
    pub size: Option<u32>,
    /// What a launcher may do with the image.
    pub purpose: IconPurpose,
}

/// An icon is written as a path with its dimensions attached, so the path is
/// what one is built from and the rest is filled in from the line's attributes.
impl From<String> for IconConfig {
    fn from(src: String) -> Self {
        Self {
            src,
            size: None,
            purpose: IconPurpose::default(),
        }
    }
}

/// How an installed app is presented, [as the manifest spells it][spec].
///
/// [spec]: https://www.w3.org/TR/appmanifest/#display-member
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DisplayMode {
    /// Its own window, with no browser UI. Why a site ships a manifest at all,
    /// hence the default.
    #[default]
    Standalone,
    /// Its own window, and the whole screen.
    Fullscreen,
    /// Its own window, keeping the minimum navigation UI the browser insists on.
    Minimal,
    /// An ordinary browser tab.
    Browser,
}

impl Named for DisplayMode {
    const NAMES: &'static [(&'static str, Self)] = &[
        ("standalone", Self::Standalone),
        ("fullscreen", Self::Fullscreen),
        ("minimal", Self::Minimal),
        ("browser", Self::Browser),
    ];
}

impl DisplayMode {
    /// The spelling the manifest takes, which is the config spelling bar
    /// `minimal`: the member is `minimal-ui`, and a config key or value is one
    /// word.
    pub fn member(self) -> &'static str {
        match self {
            Self::Minimal => "minimal-ui",
            other => other.name(),
        }
    }
}

/// What a launcher may do with an icon, [as the manifest spells it][spec].
///
/// [spec]: https://www.w3.org/TR/appmanifest/#purpose-member
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum IconPurpose {
    /// Shown as drawn, whatever the platform's icon shape is.
    #[default]
    Any,
    /// Safe to crop to the platform's shape: the image keeps its subject inside
    /// the safe zone and fills the rest with its own background.
    Maskable,
    /// A single-colour glyph the platform recolours, for a notification badge.
    Monochrome,
}

impl Named for IconPurpose {
    const NAMES: &'static [(&'static str, Self)] = &[
        ("any", Self::Any),
        ("maskable", Self::Maskable),
        ("monochrome", Self::Monochrome),
    ];
}

/// The `manifest { .. }` block. Its presence enables the manifest; every key is
/// something only the author knows, since what the build knows it fills in
/// itself.
impl Section for ManifestConfig {
    const RULES: Block<Self> = Block(&[
        (
            "name",
            Text,
            "The installed app's name. Defaults to the site title.",
            |c, n, t| {
                c.name = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "short",
            Text,
            "The name a launcher shows when the full one does not fit.",
            |c, n, t| {
                c.short = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "description",
            Text,
            "One line about the app, shown by an install prompt.",
            |c, n, t| {
                c.description = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "display",
            Choice(DisplayMode::names),
            "How the installed app is presented.",
            |c, n, t| {
                c.display = n.arg(t, 0)?.one::<DisplayMode>(t, NodeExt::span(n))?;
                Ok(())
            },
        ),
        (
            "theme",
            Text,
            "CSS colour of the browser UI around the app, and of every page's `theme-color`.",
            |c, n, t| {
                c.theme = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "background",
            Text,
            "CSS colour painted before the first page has rendered.",
            |c, n, t| {
                c.background = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "start",
            Text,
            "Where launching the installed app lands, per language. Defaults to the language's root.",
            |c, n, t| {
                c.start = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "scope",
            Text,
            "The URLs the installed app covers, per language. Defaults to the language's root.",
            |c, n, t| {
                c.scope = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "icons",
            Lines(IconConfig::rows),
            "One line per icon, each named by the path it is served from.",
            |c, n, t| {
                c.icons = n
                    .unique(t, "icon", IconConfig::item)?
                    .into_iter()
                    .map(|(_, icon)| icon)
                    .collect();
                Ok(())
            },
        ),
    ]);

    fn enable(&mut self) -> bool {
        self.enabled = true;
        true
    }
}

impl IconConfig {
    /// One `"/icon-512.png" size=512` line: the node name is the path the image
    /// is served from, which is also what makes two icons the same icon.
    pub(crate) fn item(node: &KdlNode, text: &str) -> Result<(String, Self)> {
        let src = node.name().value().to_owned();
        let mut icon = Self::from(src.clone());
        icon.read(node, text)?;
        Ok((src, icon))
    }
}

impl Attributed for IconConfig {
    const ATTRS: Attrs<Self> = Attrs(&[
        (
            "size",
            Number,
            "The square edge in pixels. Absent means the image scales to any size.",
            |c, v, t, s| {
                c.size = Some(v.bounded(t, s, 1, 4096)?);
                Ok(())
            },
        ),
        (
            "purpose",
            Choice(IconPurpose::names),
            "What a launcher may do with the image.",
            |c, v, t, s| {
                c.purpose = v.one::<IconPurpose>(t, s)?;
                Ok(())
            },
        ),
    ]);
}
