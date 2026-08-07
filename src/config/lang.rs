//! `languages { }`: one declared language of a multi-language site.

use kdl::KdlNode;

use crate::config::dispatch::Kind::{Number, Table, Text};
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;
use crate::error::Result;

/// Languages written right to left, so `dir="rtl"` is right without the site
/// having to say so. A monolingual `lang "ar"` site has no `languages` block to
/// declare `dir` in, and so could never get it.
pub(crate) struct Rtl;
impl Rtl {
    /// Primary subtags, and the script subtags that imply the direction
    /// whatever the language (`az-Arab`).
    const LANGS: &'static [&'static str] = &[
        "ar", "arc", "ckb", "dv", "fa", "he", "khw", "ks", "ps", "sd", "ug", "ur", "yi",
    ];
    const SCRIPTS: &'static [&'static str] = &["adlm", "arab", "hebr", "nkoo", "thaa"];

    pub(crate) fn of(code: &str) -> Option<&'static str> {
        let mut parts = code.split(['-', '_']).map(str::to_ascii_lowercase);
        let primary = parts.next()?;
        let rtl = Self::LANGS.contains(&primary.as_str())
            || parts.any(|part| Self::SCRIPTS.contains(&part.as_str()));
        rtl.then_some("rtl")
    }
}

/// One declared language in a multi-language site.
#[derive(Debug, Clone, Default, Hash, serde::Serialize)]
pub struct LanguageConfig {
    /// Display name for a language switcher, e.g. `Français`. Falls back to the
    /// code when unset.
    pub name: Option<String>,
    /// Writing direction, `ltr` (default) or `rtl`, surfaced as `<html dir>`.
    pub dir: Option<String>,
    /// Per-language site title override (else the site-wide `site`).
    pub site: Option<String>,
    /// Per-language description override (else the site-wide `description`), so
    /// a French feed does not carry an English blurb.
    pub description: Option<String>,
    /// Per-language author override (else the site-wide `author`).
    pub author: Option<String>,
    /// Per-language reading rate override (else `content { reading { wpm } }`).
    ///
    /// The one number here that is a fact about the *language* rather than a
    /// label for it: 200 words a minute is European prose, and a reader of
    /// Japanese gets through three times as many, so a shared figure reports
    /// every article in one of those languages as a third of the read it is.
    pub wpm: Option<usize>,
    /// UI-string table for this language, exposed to templates as
    /// `page.strings` and to client JS via `baudelaire:i18n`.
    pub strings: Vec<(String, crate::codegen::Value)>,
}

impl LanguageConfig {
    /// One declared language, keyed by its code.
    pub(crate) fn item(node: &KdlNode, text: &str) -> Result<(String, Self)> {
        let mut lang = Self::default();
        lang.fill(node, text)?;
        Ok((node.name().value().to_owned(), lang))
    }
}

/// Scalar fields dispatch like any other section; the nested `strings { .. }`
/// table reuses the very parser `client { .. }` uses, so a UI string dictionary
/// and a client-constant block stay one shape.
impl Section for LanguageConfig {
    const RULES: Block<Self> = Block(&[
        (
            "name",
            Text,
            "The language's name in its own language, for a switcher.",
            |c, n, t| {
                c.name = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "dir",
            Text,
            "Writing direction, `ltr` or `rtl`.",
            |c, n, t| {
                c.dir = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "site",
            Text,
            "The site name in this language.",
            |c, n, t| {
                c.site = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "author",
            Text,
            "The default author in this language.",
            |c, n, t| {
                c.author = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "description",
            Text,
            "What the site is, in this language.",
            |c, n, t| {
                c.description = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "wpm",
            Number,
            "Words a reader of this language gets through in a minute.",
            |c, n, t| {
                c.wpm = Some(usize::from(n.arg(t, 0)?.bounded::<u16>(
                    t,
                    NodeExt::span(n),
                    1,
                    u16::MAX,
                )?));
                Ok(())
            },
        ),
        (
            "strings",
            Table,
            "This language's UI string table, one `key value` line per entry.",
            |c, n, t| {
                c.strings = n.table(t)?;
                Ok(())
            },
        ),
    ]);
}
