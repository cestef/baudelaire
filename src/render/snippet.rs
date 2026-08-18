//! A code fence as it was written: what a checker reads, which lines the reader
//! never sees, and what came back about them.

use typst::ecow::{EcoString, EcoVec};
use typst::syntax::Span;
use typst_html::{HtmlElement, HtmlNode};

use crate::world::rules::LANG;

/// What a checker objected to: its own words, and where in the snippet.
pub struct Fault {
    /// Foreign text: escaped when reported, never read as markup.
    pub message: String,
    /// Byte offset into the snippet's text, absent for a fault with no place in
    /// it.
    pub at: Option<usize>,
}

/// A fault the checker placed nowhere in particular.
impl From<String> for Fault {
    fn from(message: String) -> Self {
        Self { message, at: None }
    }
}

/// A one-based place in a text, as a checker reporting one counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

/// What a hidden line says to the checker instead of standing in the snippet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Directive {
    /// Check nothing about this fence: it is wrong on purpose.
    Ignore,
}

impl Directive {
    /// The vocabulary, and the single source of what a directive is spelled.
    const NAMES: &'static [(&'static str, Self)] = &[("@ignore", Self::Ignore)];

    fn of(text: &str) -> Option<Self> {
        let text = text.trim();
        Self::NAMES
            .iter()
            .find(|(name, _)| *name == text)
            .map(|(_, directive)| *directive)
    }
}

/// What one line of a fence is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// Written for the reader, and checked like everything else.
    Shown,
    /// Marked: checked, but taken out of the page.
    Hidden,
    /// Marked, and read by the checker rather than checked.
    Says(Directive),
}

impl Kind {
    /// What `text` is under `marker`, and what of it belongs to the snippet.
    fn of<'a>(text: &'a str, marker: Option<&str>) -> (Self, &'a str) {
        let Some(rest) = marker
            .filter(|marker| !marker.is_empty())
            .and_then(|marker| text.strip_prefix(marker))
        else {
            return (Self::Shown, text);
        };
        Directive::of(rest).map_or((Self::Hidden, rest), |directive| {
            (Self::Says(directive), "")
        })
    }

    /// Whether the reader sees this line.
    const fn shown(self) -> bool {
        matches!(self, Self::Shown)
    }

    /// Whether it is part of what the checker reads.
    const fn checked(self) -> bool {
        !matches!(self, Self::Says(_))
    }
}

/// One line of a fence.
struct Line {
    /// Where it starts in [`Snippet::text`], which a line the checker never
    /// reads shares with the one after it.
    at: usize,
    /// Where it was written.
    span: Span,
    kind: Kind,
}

/// A code fence, gathered off the DOM: the language it claims, the text a
/// checker reads, and where each of its lines was written.
///
/// The text is every line, hidden markers taken off, which is not what the page
/// shows: [`Snippet::hide`] is what removes the hidden ones from the DOM.
pub struct Snippet {
    pub lang: String,
    text: String,
    lines: Vec<Line>,
    /// The fence itself, for a fault that names no line of it.
    span: Span,
}

impl Snippet {
    /// The fence `element` is, or `None` for an element claiming no language.
    ///
    /// `marker` is the line prefix that hides a line from the reader while
    /// leaving it in what the checker reads, and must be at the very start of
    /// the line.
    pub fn of(element: &HtmlElement, marker: Option<&str>) -> Option<Self> {
        let lang = element.attrs.get(LANG)?.to_string();
        Some(Self::assemble(
            lang,
            Lines::of(element),
            element.span,
            marker,
        ))
    }

    /// A snippet of loose text, for a checker run over something that is not a
    /// page: it hides nothing and every line is written at `span`.
    pub fn new(lang: &str, text: &str, span: Span) -> Self {
        Self::assemble(lang.to_owned(), Lines::split(text, span), span, None)
    }

    /// Lay the lines out as one text, leaving out the ones that speak to the
    /// checker rather than to it.
    fn assemble(
        lang: String,
        lines: Vec<(String, Span)>,
        span: Span,
        marker: Option<&str>,
    ) -> Self {
        let mut snippet = Self {
            lang,
            text: String::new(),
            lines: Vec::new(),
            span,
        };
        let mut written = false;
        for (line, span) in lines {
            let (kind, text) = Kind::of(&line, marker);
            if kind.checked() {
                if written {
                    snippet.text.push('\n');
                }
                written = true;
            }
            snippet.lines.push(Line {
                at: snippet.text.len(),
                span,
                kind,
            });
            snippet.text.push_str(text);
        }
        snippet
    }

    /// What the checker is handed: every line, hidden or not, each without its
    /// marker.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Whether any of its lines is one the reader is not meant to see.
    pub fn hides(&self) -> bool {
        self.lines.iter().any(|line| !line.kind.shown())
    }

    /// Whether a line of it told the checker to leave the fence alone.
    pub fn ignored(&self) -> bool {
        self.lines
            .iter()
            .any(|line| line.kind == Kind::Says(Directive::Ignore))
    }

    /// Take the hidden lines out of the element this was gathered from, leaving
    /// the highlighting of the lines that stay untouched.
    pub fn hide(&self, element: &mut HtmlElement) {
        let shown: Vec<bool> = self.lines.iter().map(|line| line.kind.shown()).collect();
        Self::prune(element, &mut 0, &shown);
        if shown.last() == Some(&false) {
            Self::unterminate(element);
        }
    }

    /// The language as a filename extension, for a checker that decides what it
    /// is reading from the name it is handed. A fence may claim anything at
    /// all, so a tag that is not a bare name is written out as text.
    pub fn extension(&self) -> &str {
        let bare = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-';
        if self.lang.is_empty() || !self.lang.chars().all(bare) {
            "txt"
        } else {
            &self.lang
        }
    }

    /// Where `at` falls in the text, or `None` for a line the snippet has not
    /// got.
    pub fn offset(&self, at: Position) -> Option<usize> {
        let line = self.lines.get(at.line.checked_sub(1)?)?;
        Some(line.at + at.column.saturating_sub(1))
    }

    /// The span of the line `at` falls on, or the fence's own for an offset past
    /// its text and for a fault that named no place at all.
    pub fn at(&self, at: Option<usize>) -> Span {
        let Some(at) = at else {
            return self.span;
        };
        self.lines
            .iter()
            .rev()
            .find(|line| line.at <= at)
            .map_or(self.span, |line| line.span)
    }

    /// Keep only what falls on a line `visible` leaves in, in document order.
    /// The line's own break goes with it, so removing one closes the gap.
    fn prune(element: &mut HtmlElement, line: &mut usize, visible: &[bool]) {
        let shown = |at: usize| visible.get(at).copied().unwrap_or(true);
        let mut kept: EcoVec<HtmlNode> = EcoVec::new();
        for node in &element.children {
            match node {
                HtmlNode::Text(text, span) => {
                    let mut out = EcoString::new();
                    for piece in text.split_inclusive('\n') {
                        if shown(*line) {
                            out.push_str(piece);
                        }
                        if piece.ends_with('\n') {
                            *line += 1;
                        }
                    }
                    if !out.is_empty() {
                        kept.push(HtmlNode::Text(out, *span));
                    }
                }
                HtmlNode::Element(child) => {
                    let at = *line;
                    let mut child = child.clone();
                    Self::prune(&mut child, line, visible);
                    if shown(at) {
                        kept.push(HtmlNode::Element(child));
                    }
                }
                other => kept.push(other.clone()),
            }
        }
        element.children = kept;
    }

    /// Drop the break the last visible line ends on, which a hidden line after
    /// it would otherwise leave hanging as a blank one.
    ///
    /// The last *written* node, not the last node: a fence's children carry the
    /// introspection tags of the lines they came from, and those trail the text.
    fn unterminate(element: &mut HtmlElement) {
        let mut emptied = None;
        for (at, node) in element.children.make_mut().iter_mut().enumerate().rev() {
            match node {
                HtmlNode::Text(text, _) => {
                    if let Some(rest) = text.strip_suffix('\n') {
                        *text = rest.into();
                    }
                    if text.is_empty() {
                        emptied = Some(at);
                    }
                    break;
                }
                HtmlNode::Element(child) => {
                    Self::unterminate(child);
                    break;
                }
                _ => {}
            }
        }
        if let Some(at) = emptied {
            element.children.remove(at);
        }
    }
}

/// A fence's lines, as the runs of text under it split at their breaks. Each
/// keeps the span of the run that opened it, which is the line it was written
/// on.
struct Lines(Vec<(String, Span)>);

impl Lines {
    fn of(element: &HtmlElement) -> Vec<(String, Span)> {
        let mut lines = Self(Vec::new());
        lines.gather(element);
        lines.0
    }

    /// The lines of a text that came from nowhere in the DOM.
    fn split(text: &str, span: Span) -> Vec<(String, Span)> {
        let mut lines = Self(Vec::new());
        lines.push(text, span);
        lines.0
    }

    fn gather(&mut self, element: &HtmlElement) {
        for child in &element.children {
            match child {
                HtmlNode::Text(text, span) => self.push(text, *span),
                HtmlNode::Element(child) => self.gather(child),
                _ => {}
            }
        }
    }

    /// Add one run, opening a line at each break in it.
    fn push(&mut self, text: &str, span: Span) {
        for (n, piece) in text.split('\n').enumerate() {
            if n > 0 || self.0.is_empty() {
                self.0.push((String::new(), span));
            }
            self.0.last_mut().expect("a line is open").0.push_str(piece);
        }
    }
}

#[cfg(test)]
mod tests {
    use typst::ecow::EcoVec;
    use typst::syntax::Span;
    use typst_html::{HtmlAttrs, HtmlElement, HtmlNode, tag};

    use super::{LANG, Position, Snippet};

    fn fence(lang: &str, runs: &[&str]) -> HtmlElement {
        let mut attrs = HtmlAttrs::new();
        attrs.push(LANG, lang);
        let mut element = HtmlElement::new(tag::code);
        element.attrs = attrs;
        element.children = runs
            .iter()
            .map(|text| HtmlNode::Text((*text).into(), Span::detached()))
            .collect();
        element
    }

    fn shown(element: &HtmlElement) -> String {
        element
            .children
            .iter()
            .map(|node| match node {
                HtmlNode::Text(text, _) => text.to_string(),
                _ => String::new(),
            })
            .collect()
    }

    #[test]
    fn a_fence_is_gathered_as_the_text_that_was_written() {
        let snippet = Snippet::of(&fence("kdl", &["site ", "\"x\""]), None).expect("a fence");
        assert_eq!(snippet.lang, "kdl");
        assert_eq!(snippet.text(), "site \"x\"");
        assert!(!snippet.hides());
    }

    #[test]
    fn an_element_carrying_no_language_is_not_a_fence() {
        assert!(Snippet::of(&HtmlElement::new(tag::code), None).is_none());
    }

    #[test]
    fn a_marked_line_can_tell_the_checker_to_leave_the_fence_alone() {
        let snippet = Snippet::of(&fence("kdl", &["%% @ignore\nsite \"x"]), Some("%% ")).unwrap();
        assert!(snippet.ignored());
        assert_eq!(snippet.text(), "site \"x", "the directive is not checked");
    }

    #[test]
    fn a_directive_is_gone_from_the_page_like_any_marked_line() {
        let mut element = fence("kdl", &["%% @ignore\nsite \"x\""]);
        let snippet = Snippet::of(&element, Some("%% ")).unwrap();
        snippet.hide(&mut element);
        assert_eq!(shown(&element), "site \"x\"");
    }

    #[test]
    fn a_fence_that_says_nothing_is_not_ignored() {
        let snippet = Snippet::of(&fence("kdl", &["%% a {\nb 1"]), Some("%% ")).unwrap();
        assert!(!snippet.ignored());
    }

    #[test]
    fn a_marked_line_is_checked_without_its_marker() {
        let snippet = Snippet::of(&fence("kdl", &["%% a {\nb 1\n%% }"]), Some("%% ")).expect("one");
        assert_eq!(snippet.text(), "a {\nb 1\n}");
        assert!(snippet.hides());
    }

    #[test]
    fn a_marker_is_only_a_marker_at_the_start_of_a_line() {
        let snippet = Snippet::of(&fence("kdl", &["a \"%% b\""]), Some("%% ")).expect("one");
        assert_eq!(snippet.text(), "a \"%% b\"");
        assert!(!snippet.hides());
    }

    #[test]
    fn hiding_leaves_the_reader_the_lines_that_were_not_marked() {
        let mut element = fence("kdl", &["%% a {\nb 1\n%% }"]);
        let snippet = Snippet::of(&element, Some("%% ")).expect("one");
        snippet.hide(&mut element);
        assert_eq!(shown(&element), "b 1");
    }

    #[test]
    fn hiding_keeps_the_highlighting_of_the_lines_that_stay() {
        let mut element = fence("kdl", &["%% a {\n"]);
        let mut token = HtmlElement::new(tag::span);
        token.children = EcoVec::from(vec![HtmlNode::Text("b".into(), Span::detached())]);
        element.children.push(HtmlNode::Element(token));
        element
            .children
            .push(HtmlNode::Text("\n%% }".into(), Span::detached()));

        let snippet = Snippet::of(&element, Some("%% ")).expect("one");
        assert_eq!(snippet.text(), "a {\nb\n}");
        snippet.hide(&mut element);

        assert_eq!(element.children.len(), 1, "only the token should be left");
        assert!(matches!(element.children[0], HtmlNode::Element(_)));
    }

    #[test]
    fn a_position_resolves_to_the_offset_of_that_character() {
        let snippet = Snippet::of(&fence("kdl", &["one\ntwo\nthree"]), None).expect("one");
        let at = |line, column| snippet.offset(Position { line, column });
        assert_eq!(at(1, 1), Some(0));
        assert_eq!(at(2, 1), Some(4));
        assert_eq!(at(3, 3), Some(10));
        assert_eq!(at(4, 1), None);
    }
}
