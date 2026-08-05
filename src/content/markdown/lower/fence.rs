//! What the writer has to decide *about* a run before it can write it: the info
//! string of a fenced block, the constructs whose content has to be complete
//! first, and whether a run of raw HTML holds anything a page would lose.

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

    /// The language a fence has to claim before that parameter means anything.
    /// Named beside it because the two are one rule, and half a rule spelled as
    /// a literal is how the halves drift apart.
    pub(super) const TYPST: &'static str = "typ";

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
        config.eval && self.eval && self.lang.as_deref() == Some(Self::TYPST)
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

/// A run of raw HTML, as the parser handed it over.
pub(super) struct Html<'a>(pub(super) &'a str);

impl Html<'_> {
    /// What opens a comment.
    const OPEN: &'static str = "<!--";
    /// What closes one.
    const CLOSE: &'static str = "-->";
    /// The two empty comments, `<!-->` and `<!--->`, written as what follows
    /// their `<!--`. Both close on a terminator that overlaps the opening, so
    /// scanning for `-->` finds neither and they have to be matched first.
    const EMPTY: [&'static str; 2] = [">", "->"];

    /// Whether the run is nothing but comments and the whitespace around them.
    /// A comment is the one shape of raw HTML with no rendered counterpart, so
    /// dropping *that* loses nothing.
    ///
    /// Scanned, rather than matched at its two ends. CommonMark ends an HTML
    /// block on the line carrying `-->`, so
    /// `<!-- a --><div>secret</div><!-- b -->` is one event that opens and
    /// closes like a comment with an element hidden between: testing the ends
    /// dropped the whole run, which lost the content *and* walked past the
    /// `html` policy that would have refused it.
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
            // Empty comments, whose terminator overlaps the `<!--` that opened
            // them. The length guard that used to defeat that overlap rejected
            // both outright.
            "<!-->",
            "<!--->",
            // A comment whose content is a single `-`.
            "<!----->",
        ] {
            assert!(Html(run).is_comment(), "{run:?}");
        }
    }

    #[test]
    fn a_comment_cannot_hide_markup_behind_it() {
        for run in [
            // One HTML block, and the whole reason for the scan.
            "<!-- a --><div>secret</div><!-- b -->",
            // `<!-->` is a *complete* comment, so what follows it is markup.
            "<!--><div>secret</div>-->",
            "<div>secret</div>",
            // Never closed, so nothing here is a comment.
            "<!-- unterminated",
        ] {
            assert!(!Html(run).is_comment(), "{run:?}");
        }
    }
}
