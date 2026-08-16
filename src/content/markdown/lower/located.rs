//! Where a lowered span came from: mapping a markdown offset back to the file.

use std::ops::Range;
/// Where an event came from in the file being lowered: the text a snippet is
/// rendered from, and the byte range of each event within it.
pub(super) struct Located<'a> {
    pub(super) file: &'a str,
    /// Byte offset of the body within `file`.
    pub(super) offset: usize,
    pub(super) spans: &'a [Range<usize>],
    /// The event being written, as an index into `spans`.
    pub(super) at: usize,
}
impl<'a> Located<'a> {
    pub(super) fn new(file: &'a str, offset: usize, spans: &'a [Range<usize>]) -> Self {
        Self {
            file,
            offset,
            spans,
            at: 0,
        }
    }

    /// A copy pointing at the same file and spans, for a nested writer.
    pub(super) fn borrowed(&self) -> Self {
        Self { ..*self }
    }

    /// The current event's range in the file's own coordinates; `None` for an
    /// event with no span table entry, and for an empty range, which would read
    /// as the top of the file rather than as "nowhere".
    pub(super) fn range(&self) -> Option<Range<usize>> {
        let range = self.spans.get(self.at)?;
        (!range.is_empty()).then(|| (range.start + self.offset)..(range.end + self.offset))
    }

    /// The current event's span, in the file's own coordinates.
    pub(super) fn span(&self) -> miette::SourceSpan {
        let range = self.range().unwrap_or(self.offset..self.offset);
        miette::SourceSpan::new(range.start.into(), range.len())
    }

    /// The range of the current fenced block's *content* in the file: its span
    /// less the info line, so an offset inside the block maps to the line the
    /// author wrote rather than to the fence marker above it.
    pub(super) fn fenced(&self, len: usize) -> Option<Range<usize>> {
        let span = self.spans.get(self.at)?;
        let start = span.start + self.offset;
        let end = (span.end + self.offset).min(self.file.len());
        let newline = self.file.get(start..end)?.find('\n')?;
        let text = start + newline + 1;
        Some(text..(text + len).min(self.file.len()))
    }

    /// The same block, paired a line at a time: `(range within `content`, range
    /// in the file)` for each of its lines.
    ///
    /// An indented fence is handed back with its indentation stripped, so only
    /// the lines are byte-for-byte with the file; a line that does not
    /// correspond is skipped rather than guessed.
    pub(super) fn fenced_lines(&self, content: &str) -> Vec<(Range<usize>, Range<usize>)> {
        let Some(block) = self.fenced(content.len()) else {
            return Vec::new();
        };
        let mut pairs = Vec::new();
        let mut cursor = block.start;
        let mut at = 0;
        for line in content.split_inclusive('\n') {
            let stripped = line.trim_end_matches(['\n', '\r']);
            let authored = self.file.get(cursor..).unwrap_or_default();
            let width = authored.find('\n').unwrap_or(authored.len());
            let Some(text) = authored.get(..width) else {
                break;
            };
            if !stripped.is_empty()
                && let Some(indent) = text
                    .trim_end_matches('\r')
                    .strip_suffix(stripped)
                    .map(str::len)
            {
                let from = cursor + indent;
                pairs.push((at..at + stripped.len(), from..from + stripped.len()));
            }
            cursor += width + 1;
            at += line.len();
        }
        pairs
    }

    /// The file, named, for a diagnostic to render its snippet from.
    pub(super) fn source(&self, path: &str) -> miette::NamedSource<String> {
        miette::NamedSource::new(path, self.file.to_owned())
    }
}
