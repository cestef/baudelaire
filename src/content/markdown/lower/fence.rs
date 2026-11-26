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
    /// Every other parameter, as the `data-*` attribute it puts on the block's
    /// `<pre>` and its value, empty for a bare one.
    pub(super) attrs: Vec<(String, String)>,
}

impl Fence {
    /// The parameter that makes a fence run, which is never an attribute.
    pub(super) const EVAL: &'static str = "eval";

    /// The languages a fence has to claim before that parameter means anything,
    /// read from the highlighter's own set so the two cannot drift apart.
    pub(super) const TYPST: &'static [&'static str] = crate::world::rules::TYPST;

    /// Read an info string, or hand back the parameter that names no attribute.
    ///
    /// Parameters are separated by whitespace, or by commas when there are any,
    /// which is how Zola writes them (`rust, linenos, hl_lines=1 3`).
    pub(super) fn parse(info: &str) -> Result<Self, String> {
        let words: Vec<&str> = if info.contains(',') {
            info.split(',').map(str::trim).collect()
        } else {
            info.split_whitespace().collect()
        };
        let mut words = words.into_iter().filter(|word| !word.is_empty());
        let lang = words.next().map(str::to_owned);
        let mut fence = Self {
            lang,
            eval: false,
            attrs: Vec::new(),
        };
        for param in words {
            let (key, value) = param.split_once('=').unwrap_or((param, ""));
            if key == Self::EVAL {
                fence.eval = value.is_empty() || value == "true";
                continue;
            }
            if key.is_empty()
                || !key
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            {
                return Err(param.to_owned());
            }
            let value = value
                .strip_prefix('"')
                .and_then(|inner| inner.strip_suffix('"'))
                .unwrap_or(value);
            fence.attrs.push((
                format!("data-{}", key.to_ascii_lowercase()),
                value.to_owned(),
            ));
        }
        Ok(fence)
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
    use super::{Fence, Html};

    fn attrs(info: &str) -> Vec<(String, String)> {
        Fence::parse(info).expect("a valid info string").attrs
    }

    #[test]
    fn a_parameter_becomes_a_data_attribute() {
        assert_eq!(attrs("v copy"), [("data-copy".to_owned(), String::new())]);
        assert_eq!(
            attrs("sh title=install.sh"),
            [("data-title".to_owned(), "install.sh".to_owned())]
        );
        assert_eq!(
            attrs(r#"sh title="install.sh""#),
            [("data-title".to_owned(), "install.sh".to_owned())]
        );
        assert_eq!(Fence::parse("v copy").unwrap().lang.as_deref(), Some("v"));
    }

    #[test]
    fn eval_is_read_and_never_emitted() {
        let fence = Fence::parse("typ eval copy").unwrap();
        assert!(fence.eval);
        assert_eq!(fence.attrs, [("data-copy".to_owned(), String::new())]);
        assert!(!Fence::parse("typ eval=false").unwrap().eval);
    }

    #[test]
    fn zola_commas_separate_the_language_from_its_parameters() {
        let fence = Fence::parse("v, copy, hl_lines=1 3").unwrap();
        assert_eq!(fence.lang.as_deref(), Some("v"));
        assert_eq!(
            fence.attrs,
            [
                ("data-copy".to_owned(), String::new()),
                ("data-hl_lines".to_owned(), "1 3".to_owned()),
            ]
        );
    }

    #[test]
    fn a_parameter_that_names_no_attribute_is_refused() {
        assert_eq!(Fence::parse("v <b>").err().as_deref(), Some("<b>"));
        assert_eq!(Fence::parse("v =x").err().as_deref(), Some("=x"));
    }

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
