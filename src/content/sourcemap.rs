//! Where the Typst a page lowered to came from in the file its author wrote.
//!
//! A markdown page reaches a browser over two hops: markdown lowers to Typst,
//! and that Typst compiles to HTML. Typst's own spans cover the second hop and
//! nothing covers the first, so a location in the finished page names generated
//! source under a virtual path unless it is translated. This is that
//! translation, and both passes that need it read the one map: the diagnostic
//! bridge, which underlines the `.md` line rather than a line nobody wrote, and
//! the `data-typst` stamper, which has to name a file an editor can open.
//!
//! It lives beside the content it describes rather than beside the errors. It
//! *was* an error type, and being one is why the error path stayed its only
//! reader for as long as it did.

use std::ops::Range;
use std::sync::Arc;

/// How a stretch of lowered Typst lines up with the text it was written from.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum Shape {
    /// Copied out unchanged, so the two correspond byte for byte and an offset
    /// inside is kept: an error on the third line of an `eval` fence lands on
    /// the third line of that fence and not at the top of it.
    Verbatim,
    /// Assembled rather than copied - a generated call, prose escaped into a
    /// string literal - so no offset inside it names anything on the other
    /// side. It maps as a whole: to where the construct starts, and over
    /// everything the author wrote for it. Preserving offsets here is what put
    /// a paragraph's stamp one column past the paragraph.
    Whole,
}

/// One stretch of the lowered body and the authored text it came from.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Mapping {
    lowered: Range<usize>,
    source: Range<usize>,
    shape: Shape,
}

impl Mapping {
    pub fn new(lowered: Range<usize>, source: Range<usize>, shape: Shape) -> Self {
        Self {
            lowered,
            source,
            shape,
        }
    }

    /// This mapping shifted by `at`, for a buffer spliced into another: the
    /// child's text landed there, so everything recorded against it did too.
    pub fn shifted(self, at: usize) -> Self {
        Self {
            lowered: (self.lowered.start + at)..(self.lowered.end + at),
            ..self
        }
    }

    /// Where `range` - which starts inside [`lowered`](Self::lowered) - lands
    /// in the authored text.
    fn resolve(&self, range: &Range<usize>) -> Range<usize> {
        match self.shape {
            Shape::Verbatim => {
                let at =
                    self.source.start + (range.start - self.lowered.start).min(self.source.len());
                at..(at + range.len()).min(self.source.end).max(at)
            }
            Shape::Whole => self.source.clone(),
        }
    }
}

/// The lowered body of a page paired with the text its author wrote, as the
/// ranges of one that came from ranges of the other.
///
/// Only the parts the lowering drew from authored text are recorded. The
/// wrapper a page is compiled inside is not lowered from anything, so a span in
/// it maps to nothing at all, and so does any part of the body no markdown
/// event produced.
#[derive(Debug, Clone, Default, Hash, PartialEq, Eq)]
pub struct SourceMap {
    /// The authored file, whole: what a diagnostic renders its snippet from and
    /// what a line and column are counted against. The frontmatter block is
    /// included, so a position is the one the author's editor shows.
    text: String,
    /// Byte length of the lowered body. The body is written last, so it sits at
    /// the end of the wrapper and everything before it is the preamble.
    body: usize,
    /// Every recorded stretch, in the order the lowering wrote them. Innermost
    /// first where two overlap, because [`SourceMap::within`] takes the first
    /// match: a footnote's own body is spliced in before the call that carries
    /// it is recorded, so a span inside the body resolves to the note and not
    /// to the `[^n]` that referenced it.
    spans: Vec<Mapping>,
}

impl SourceMap {
    /// `spans` pairs ranges of the generated body with the ranges of `text`
    /// they were written from. `body` is that body's length.
    pub fn new(text: String, body: usize, spans: Vec<Mapping>) -> Self {
        Self { text, body, spans }
    }

    /// The authored file this maps into.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The one-based line and column of `offset` in [`text`](Self::text), as
    /// every editor counts them: the spelling a `+line:column` jump takes.
    ///
    /// `None` for an offset past the end or inside a character, which is what a
    /// range this map did not produce would give.
    pub fn position(&self, offset: usize) -> Option<(usize, usize)> {
        let before = self.text.get(..offset)?;
        let line = before.bytes().filter(|&byte| byte == b'\n').count() + 1;
        // Characters, not bytes: an editor puts the caret after *n* characters,
        // so an accented word above the caret would shift the column otherwise.
        let column = before
            .rsplit('\n')
            .next()
            .unwrap_or_default()
            .chars()
            .count()
            + 1;
        Some((line, column))
    }

    /// Where `range` - a span in the lowered *body* - came from in
    /// [`text`](Self::text), if it came from there at all.
    ///
    /// Private because a body offset is not something a caller has: what typst
    /// compiled was the wrapper, and [`Rebased`] is what holds the difference.
    fn within(&self, range: &Range<usize>) -> Option<Range<usize>> {
        self.spans
            .iter()
            .find(|span| span.lowered.start <= range.start && range.start < span.lowered.end)
            .map(|span| span.resolve(range))
    }
}

/// A [`SourceMap`] positioned in the wrapper module that was compiled.
///
/// The map is written against the lowered body alone; what typst was handed is
/// that body under a preamble binding the page to its template. Folding the
/// difference in here is what keeps it from being a number every caller has to
/// know: `locate` used to take the wrapper's length as an argument, and a
/// caller that passed the wrong text's length got plausible offsets into the
/// author's file rather than an error.
#[derive(Debug, Clone)]
pub struct Rebased {
    map: Arc<SourceMap>,
    /// Byte length of everything the wrapper writes before the body.
    preamble: usize,
}

impl Rebased {
    /// `map` as it sits in `wrapper`, the exact text handed to the compiler.
    ///
    /// Takes that text rather than its length because a length is a number any
    /// caller can produce and none can check. The body is written last, so
    /// everything before it is the preamble; a `wrapper` too short to hold the
    /// body did not come from this page, and is refused rather than shifting
    /// every span by a plausible-looking amount.
    pub fn new(map: Arc<SourceMap>, wrapper: &str) -> Option<Self> {
        let preamble = wrapper.len().checked_sub(map.body)?;
        Some(Self { map, preamble })
    }

    /// The map underneath, for the authored text and the positions counted
    /// against it.
    pub fn map(&self) -> &SourceMap {
        &self.map
    }

    /// Where `range` - a span in the *wrapper* typst compiled - came from in
    /// the authored file, if it came from there at all.
    ///
    /// `None` for a span in generated code, and callers must keep it that way:
    /// an untranslated offset drawn against the authored file would underline
    /// an unrelated place, or overrun it. The diagnostic bridge drops such a
    /// label; the span stamper leaves the element unstamped.
    pub fn locate(&self, range: &Range<usize>) -> Option<Range<usize>> {
        let start = range.start.checked_sub(self.preamble)?;
        self.map.within(&(start..start + range.len()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A wrapper of `preamble` bytes with a body of `body` after it, which is
    /// the shape [`Rebased`] measures against.
    fn wrapper(preamble: usize, body: &str) -> String {
        format!("{}{body}", " ".repeat(preamble))
    }

    #[test]
    fn a_span_in_the_preamble_maps_to_nothing() {
        let map = SourceMap::new(
            "hello\n".to_owned(),
            6,
            vec![Mapping::new(0..6, 0..6, Shape::Verbatim)],
        );
        let text = wrapper(10, "abcdef");
        let rebased = Rebased::new(Arc::new(map), &text).expect("the body fits");
        assert_eq!(rebased.locate(&(2..4)), None);
        assert_eq!(rebased.locate(&(10..12)), Some(0..2));
    }

    /// The wrapper may grow (a template binding, backlinks) without the map
    /// being told: the preamble is derived from the text, never remembered.
    #[test]
    fn the_preamble_is_measured_not_assumed() {
        let map = Arc::new(SourceMap::new(
            "hello\n".to_owned(),
            6,
            vec![Mapping::new(0..6, 0..6, Shape::Verbatim)],
        ));
        for preamble in [0, 1, 500] {
            let text = wrapper(preamble, "abcdef");
            let rebased = Rebased::new(Arc::clone(&map), &text).expect("the body fits");
            assert_eq!(rebased.locate(&(preamble..preamble + 2)), Some(0..2));
        }
    }

    /// A wrapper that cannot hold the body is not this page's, and guessing an
    /// offset into it is worse than declining.
    #[test]
    fn a_wrapper_too_short_for_the_body_is_refused() {
        let map = SourceMap::new("hello\n".to_owned(), 6, Vec::new());
        assert!(Rebased::new(Arc::new(map), "abc").is_none());
    }

    #[test]
    fn a_position_counts_lines_and_characters() {
        let map = SourceMap::new("ab\nécd\n".to_owned(), 0, Vec::new());
        assert_eq!(map.position(0), Some((1, 1)));
        assert_eq!(map.position(3), Some((2, 1)));
        // `é` is two bytes and one column.
        assert_eq!(map.position(6), Some((2, 3)));
        assert_eq!(map.position(99), None);
    }

    /// The first pair wins, which is what makes an inner construct's span
    /// resolve to itself rather than to the outer one that contains it.
    #[test]
    fn the_innermost_recorded_pair_wins() {
        let map = SourceMap::new(
            "0123456789".to_owned(),
            10,
            vec![
                Mapping::new(2..4, 7..9, Shape::Verbatim),
                Mapping::new(0..10, 0..10, Shape::Verbatim),
            ],
        );
        let text = wrapper(0, "0123456789");
        let rebased = Rebased::new(Arc::new(map), &text).expect("the body fits");
        assert_eq!(rebased.locate(&(2..3)), Some(7..8));
        assert_eq!(rebased.locate(&(5..6)), Some(5..6));
    }
}
