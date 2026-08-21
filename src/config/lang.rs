//! `languages { }`: one declared language of a multi-language site.

use kdl::KdlNode;

use dispatch_derive::Table as Derive;

use crate::config::Value;
use crate::config::dispatch::Kind::{Number, Table};
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;
use crate::config::vocab::rule;
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

#[derive(Debug, Clone, Default, Hash, serde::Serialize, Derive)]
pub struct LanguageConfig {
    /// The language's name in its own language, for a switcher.
    ///
    /// Falls back to the code when unset.
    #[key(opt text)]
    pub name: Option<String>,

    /// Writing direction, `ltr` or `rtl`.
    ///
    /// Surfaced as `<html dir>`.
    #[key(opt text)]
    pub dir: Option<String>,

    /// The site name in this language.
    #[key(opt text)]
    pub site: Option<String>,

    /// The default author in this language.
    #[key(opt text)]
    pub author: Option<String>,

    /// What the site is, in this language.
    #[key(opt text)]
    pub description: Option<String>,

    /// Words a reader of this language gets through in a minute.
    ///
    /// Overrides `content { reading { wpm } }`, because a reading rate is a
    /// fact about the language rather than about the site.
    #[key(custom(
        Number,
        |c: &Self| c.wpm.into(),
        |c: &mut Self, n: &kdl::KdlNode, t: &str| {
            c.wpm = Some(usize::from(n.arg(t, 0)?.bounded::<u16>(
                t,
                NodeExt::span(n),
                1,
                u16::MAX,
            )?));
            Ok(())
        },
    ))]
    pub wpm: Option<usize>,

    /// This language's UI string table, one `key value` line per entry.
    ///
    /// Exposed to templates as `page.strings` and to client JS via
    /// `baudelaire:i18n`.
    #[key(custom(
        Table,
        |c: &Self| Value::each(&c.strings, |value| value.into()),
        |c: &mut Self, n: &kdl::KdlNode, t: &str| {
            c.strings = n.table(t)?;
            Ok(())
        },
    ))]
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
