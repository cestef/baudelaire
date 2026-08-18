//! `languages { }`: one declared language of a multi-language site.

use kdl::KdlNode;

use crate::config::Value;
use crate::config::dispatch::Kind::{Number, Table, Text};
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;
use crate::error::Result;

/// Languages written right to left, so `dir="rtl"` is right without the site
/// having to say so.
pub(crate) struct Rtl;
impl Rtl {
    /// Primary subtags; [`Rtl::SCRIPTS`] holds the script subtags that imply
    /// the direction whatever the language (`az-Arab`).
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

#[derive(Debug, Clone, Default, Hash, serde::Serialize)]
pub struct LanguageConfig {
    /// Display name for a language switcher, e.g. `Français`. Falls back to the
    /// code when unset.
    pub name: Option<String>,
    /// Writing direction, `ltr` (default) or `rtl`, surfaced as `<html dir>`.
    pub dir: Option<String>,
    /// Overrides the site-wide `site` when set.
    pub site: Option<String>,
    /// Overrides the site-wide `description` when set.
    pub description: Option<String>,
    /// Overrides the site-wide `author` when set.
    pub author: Option<String>,
    /// Overrides `content { reading { wpm } }` when set, because a reading rate
    /// is a fact about the language rather than about the site.
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

impl Section for LanguageConfig {
    const RULES: Block<Self> = Block(&[
        (
            "name",
            Text,
            "The language's name in its own language, for a switcher.",
            |c| c.name.clone().into(),
            |c, n, t| {
                c.name = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "dir",
            Text,
            "Writing direction, `ltr` or `rtl`.",
            |c| c.dir.clone().into(),
            |c, n, t| {
                c.dir = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "site",
            Text,
            "The site name in this language.",
            |c| c.site.clone().into(),
            |c, n, t| {
                c.site = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "author",
            Text,
            "The default author in this language.",
            |c| c.author.clone().into(),
            |c, n, t| {
                c.author = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "description",
            Text,
            "What the site is, in this language.",
            |c| c.description.clone().into(),
            |c, n, t| {
                c.description = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "wpm",
            Number,
            "Words a reader of this language gets through in a minute.",
            |c| c.wpm.into(),
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
            |c| Value::each(&c.strings, |value| value.into()),
            |c, n, t| {
                c.strings = n.table(t)?;
                Ok(())
            },
        ),
    ]);
}
