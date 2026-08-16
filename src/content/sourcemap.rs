//! Where the Typst a page lowered to came from in the file its author wrote:
//! typst's own spans cover the compile, and this map covers the lowering.

use std::ops::Range;
use std::sync::Arc;

/// How a stretch of lowered Typst lines up with the text it was written from.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum Shape {
    /// Copied out unchanged, so the two correspond byte for byte and an offset
    /// inside is kept.
    Verbatim,
    /// Assembled rather than copied, so no offset inside names anything on the
    /// other side and the stretch maps as a whole.
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

    /// This mapping shifted by `at`, for a buffer spliced into another.
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
/// ranges of one that came from ranges of the other. Only what the lowering
/// drew from authored text is recorded, so anything else maps to nothing.
#[derive(Debug, Clone, Default, Hash, PartialEq, Eq)]
pub struct SourceMap {
    /// The authored file, whole, frontmatter block included, so a position is
    /// the one the author's editor shows.
    text: String,
    /// Byte length of the lowered body, which the wrapper writes last.
    body: usize,
    /// Every recorded stretch, innermost first where two overlap, because
    /// [`SourceMap::within`] takes the first match.
    spans: Vec<Mapping>,
}

impl SourceMap {
    /// `spans` pairs ranges of the generated body, whose length is `body`, with
    /// the ranges of `text` they were written from.
    pub fn new(text: String, body: usize, spans: Vec<Mapping>) -> Self {
        Self { text, body, spans }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// The one-based line and column of `offset` in [`text`](Self::text), the
    /// column in characters as every editor counts it; `None` for an offset
    /// past the end or inside a character.
    pub fn position(&self, offset: usize) -> Option<(usize, usize)> {
        let before = self.text.get(..offset)?;
        let line = before.bytes().filter(|&byte| byte == b'\n').count() + 1;
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
    fn within(&self, range: &Range<usize>) -> Option<Range<usize>> {
        self.spans
            .iter()
            .find(|span| span.lowered.start <= range.start && range.start < span.lowered.end)
            .map(|span| span.resolve(range))
    }
}

/// A [`SourceMap`] positioned in the wrapper module that was compiled, since
/// the map itself is written against the lowered body alone.
#[derive(Debug, Clone)]
pub struct Rebased {
    map: Arc<SourceMap>,
    /// Byte length of everything the wrapper writes before the body.
    preamble: usize,
}

impl Rebased {
    /// `map` as it sits in `wrapper`, the exact text handed to the compiler; a
    /// `wrapper` too short to hold the body did not come from this page and is
    /// refused rather than shifting every span by a plausible amount.
    pub fn new(map: Arc<SourceMap>, wrapper: &str) -> Option<Self> {
        let preamble = wrapper.len().checked_sub(map.body)?;
        Some(Self { map, preamble })
    }

    pub fn map(&self) -> &SourceMap {
        &self.map
    }

    /// Where `range` - a span in the *wrapper* typst compiled - came from in
    /// the authored file, if it came from there at all. `None` means generated
    /// code, and a caller must drop the label rather than draw an untranslated
    /// offset against the authored file.
    pub fn locate(&self, range: &Range<usize>) -> Option<Range<usize>> {
        let start = range.start.checked_sub(self.preamble)?;
        self.map.within(&(start..start + range.len()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(map.position(6), Some((2, 3)));
        assert_eq!(map.position(99), None);
    }

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
