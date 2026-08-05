//! A fenced block's info string: the language it claims, and whether it runs.

use crate::config::MarkdownConfig;
/// A fence's info string: a language, and the parameters that say what to do
/// with it. Parsed rather than matched whole so a new option is a key here and
/// nothing else, and so `typ eval` cannot be confused for a language nobody
/// registered.
pub(super) struct Fence {
    pub(super) lang: Option<String>,
    /// Whether the block is Typst to run rather than a sample to show.
    pub(super) eval: bool,
}

impl Fence {
    /// The parameter that makes a fence run. Named once.
    pub(super) const EVAL: &'static str = "eval";

    pub(super) fn parse(info: &str) -> Self {
        let mut words = info.split_whitespace();
        let lang = words.next().filter(|w| !w.is_empty()).map(str::to_owned);
        let mut eval = false;
        for param in words {
            // `key=value` is accepted so the grammar has room to grow; today
            // the only key is a bare flag, and `eval=false` reads as written.
            let (key, value) = param.split_once('=').unwrap_or((param, "true"));
            if key == Self::EVAL {
                eval = value == "true";
            }
        }
        Self { lang, eval }
    }

    /// Whether this fence runs, which needs three things to agree: the page
    /// asked, the language is Typst (`sh eval` would otherwise emit a shell
    /// script as Typst source), and the site permits it at all.
    pub(super) fn runs(&self, config: &MarkdownConfig) -> bool {
        config.eval && self.eval && self.lang.as_deref() == Some("typ")
    }
}

/// A construct whose content has to be complete before anything can be written
/// for it: the alt text of an image, the body of a footnote, the text of a code
/// block. Everything else streams straight out.
pub(super) enum Buffered {
    /// `alt` collects the raw text of the alt run. It is *not* taken from the
    /// lowered buffer: an alt attribute is a plain string, and un-escaping
    /// lowered output to recover one loses every inline that is not a text run
    /// and mistakes a `#"` inside the generated source for the start of one.
    Alt {
        dest: String,
        alt: String,
    },
    Code {
        fence: Fence,
    },
}

/// Whether a raw-HTML run is nothing but a comment. A comment is the one shape
/// of raw HTML with no rendered counterpart, so dropping it loses nothing.
pub(super) fn is_comment(raw: &str) -> bool {
    let trimmed = raw.trim();
    trimmed.len() >= 7 && trimmed.starts_with("<!--") && trimmed.ends_with("-->")
}
