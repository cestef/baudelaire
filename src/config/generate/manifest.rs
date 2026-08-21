//! `generate { manifest { } }`: `manifest.webmanifest` and its icons.

use kdl::KdlNode;

use dispatch_derive::Table;

use crate::config::Value;
use crate::config::dispatch::Kind::Lines;
use crate::config::dispatch::{Attributed, Attrs, Block, Section};
use crate::config::node::NodeExt;
use crate::config::vocab::{attr, rule};
use crate::config::{Config, Named};
use crate::error::Result;

/// `manifest.webmanifest` generation ([the web app manifest][spec]): what a
/// browser reads when a visitor installs the site to a home screen. Enabled by
/// the presence of a `generate { manifest }` block.
///
/// [spec]: https://www.w3.org/TR/appmanifest/
#[derive(Debug, Clone, Hash, Default, Table)]
#[table(hook(switch = enabled))]
pub struct ManifestConfig {
    pub enabled: bool,

    /// The installed app's name. Defaults to the site title.
    ///
    /// In the language the manifest is written for.
    #[key(opt text)]
    pub name: Option<String>,

    /// The name a launcher shows when the full one does not fit.
    #[key(name = "short", opt text)]
    pub short: Option<String>,

    /// One line about the app, shown by an install prompt.
    #[key(opt text)]
    pub description: Option<String>,

    /// How the installed app is presented.
    #[key(choice(DisplayMode))]
    pub display: DisplayMode,

    /// CSS colour of the browser UI around the app, and of every page's `theme-color`.
    ///
    /// Written to every page's `<meta name="theme-color">` so a tab is tinted
    /// before any install.
    #[key(opt text)]
    pub theme: Option<String>,

    /// CSS colour painted before the first page has rendered.
    #[key(opt text)]
    pub background: Option<String>,

    /// Where launching the installed app lands, per language. Defaults to the language's root.
    ///
    /// A root-relative path localized per language: `/home/` launches the
    /// French app into `/fr/home/`.
    #[key(opt text)]
    pub start: Option<String>,

    /// The URLs the installed app covers, per language. Defaults to the language's root.
    ///
    /// Localized like [`start`](Self::start); navigating outside it leaves the
    /// app.
    #[key(opt text)]
    pub scope: Option<String>,

    /// One line per icon, each named by the path it is served from.
    ///
    /// A manifest with none cannot be installed, so a build that emits one
    /// warns.
    #[key(custom(
        Lines(IconConfig::rows),
        |c: &Self| {
            Value::block(
                c.icons
                    .iter()
                    .map(|icon| (icon.src.clone(), icon.values()))
                    .collect(),
            )
        },
        |c: &mut Self, n: &kdl::KdlNode, t: &str| {
            c.icons = n
                .unique(t, "icon", IconConfig::item)?
                .into_iter()
                .map(|(_, icon)| icon)
                .collect();
            Ok(())
        },
    ))]
    pub icons: Vec<IconConfig>,
}

impl ManifestConfig {
    /// The output file name, at the root of each language's scope.
    pub const FILE: &'static str = "manifest.webmanifest";

    /// The manifest of a language: what that language's pages point
    /// `<link rel="manifest">` at. Root-relative rather than absolute, so a
    /// manifest is reachable without a configured site `url`.
    pub fn url(config: &Config, lang: &str) -> String {
        let scope = config.scope(lang, "");
        let path = if scope.is_empty() {
            format!("/{}", Self::FILE)
        } else {
            format!("/{scope}/{}", Self::FILE)
        };
        config.prefixed(&path)
    }
}

/// One entry of a manifest's `icons` array.
#[derive(Debug, Clone, Hash, Table)]
#[table(impl = Attributed, const ATTRS: Attrs<Self> = Attrs, rule = attr)]
pub struct IconConfig {
    /// Where the image is served from, root-relative, written as the node's
    /// name: `"/icon-512.png" size=512`.
    pub src: String,

    /// The square edge in pixels. Absent means the image scales to any size.
    ///
    /// Which is what a vector icon does.
    #[key(opt bounded(u32, 1, 4096))]
    pub size: Option<u32>,

    /// What a launcher may do with the image.
    #[key(choice(IconPurpose))]
    pub purpose: IconPurpose,
}

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
    /// Its own window, with no browser UI.
    #[default]
    Standalone,
    /// Its own window, and the whole screen.
    Fullscreen,
    /// Its own window, keeping the minimum navigation UI the browser insists
    /// on.
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
    /// `minimal`, whose member is `minimal-ui`.
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
    /// Safe to crop to the platform's shape, so the image keeps its subject
    /// inside the safe zone.
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
