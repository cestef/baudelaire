//! What the writer has to decide *about* a run before it can write it: the info
//! string of a fenced block, the constructs whose content has to be complete
//! first, and whether a run of raw HTML holds anything a page would lose.

use crate::config::MarkdownConfig;
/// A fence's info string: a language, and the parameters that say what to do
/// with it.
pub(super) struct Fence {
    pub(super) lang: Option<String>,
    /// Whether the block is Typst to run rather than a sample to show.
    pub(super) eval: bool,
}

impl Fence {
    /// The parameter that makes a fence run.
    pub(super) const EVAL: &'static str = "eval";

    /// The languages a fence has to claim before that parameter means anything,
    /// read from the highlighter's own set so the two cannot drift apart.
    pub(super) const TYPST: &'static [&'static str] = crate::world::rules::TYPST;

    pub(super) fn parse(info: &str) -> Self {
        let mut words = info.split_whitespace();
        let lang = words.next().filter(|w| !w.is_empty()).map(str::to_owned);
        let mut eval = false;
        for param in words {
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
        config.eval
            && self.eval
            && self
                .lang
                .as_deref()
                .is_some_and(|lang| Self::TYPST.contains(&lang))
    }
}

/// A construct whose content has to be complete before anything can be written
/// for it: the alt text of an image, the body of a footnote, the text of a code
/// block. Everything else streams straight out.
pub(super) enum Buffered {
    /// `alt` collects the raw text of the alt run, never the lowered output
    /// read back, which would leave generated source in the attribute.
    Alt {
        dest: String,
        alt: String,
    },
    Code {
        fence: Fence,
    },
}

/// A run of raw HTML, as the parser handed it over.
pub(super) struct Html<'a>(pub(super) &'a str);

impl Html<'_> {
    const OPEN: &'static str = "<!--";
    const CLOSE: &'static str = "-->";
    /// The two empty comments, `<!-->` and `<!--->`, written as what follows
    /// their `<!--`; both close on a terminator overlapping the opening, so
    /// scanning for `-->` finds neither and they have to be matched first.
    const EMPTY: [&'static str; 2] = [">", "->"];

    /// Whether the run is nothing but comments and the whitespace around them,
    /// scanned rather than matched at its two ends because
    /// `<!-- a --><div>x</div><!-- b -->` is one event that opens and closes
    /// like a comment.
    pub(super) fn is_comment(&self) -> bool {
        let mut rest = self.0.trim();
        while let Some(after) = rest.strip_prefix(Self::OPEN) {
            let Some(taken) = Self::closed(after) else {
                return false;
            };
            rest = after[taken..].trim();
        }
        rest.is_empty()
    }

    /// How much of `after` -- everything past a `<!--` -- the comment it opened
    /// takes, terminator included. `None` for one that never closes, which is
    /// not a comment at all.
    fn closed(after: &str) -> Option<usize> {
        for empty in Self::EMPTY {
            if after.starts_with(empty) {
                return Some(empty.len());
            }
        }
        after.find(Self::CLOSE).map(|at| at + Self::CLOSE.len())
    }
}

#[cfg(test)]
mod tests {
    use super::Html;

    #[test]
    fn a_run_of_only_comments_is_a_comment() {
        for run in [
            "<!-- a note -->",
            "  <!-- a -->\n<!-- b -->\n",
            "<!-->",
            "<!--->",
            "<!----->",
        ] {
            assert!(Html(run).is_comment(), "{run:?}");
        }
    }

    #[test]
    fn a_comment_cannot_hide_markup_behind_it() {
        for run in [
            "<!-- a --><div>secret</div><!-- b -->",
            "<!--><div>secret</div>-->",
            "<div>secret</div>",
            "<!-- unterminated",
        ] {
            assert!(!Html(run).is_comment(), "{run:?}");
        }
    }
}
