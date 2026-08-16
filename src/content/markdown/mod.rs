//! Markdown pages: a source dialect that lowers to Typst.
//!
//! [`Markdown::lower`] turns the body into Typst source and the frontmatter
//! block into a Typst dict; from there the engine sees a `.typ` page.

mod frontmatter;
mod lower;

pub use frontmatter::{Block, Dialect, FENCES, Fence, Spans};
pub use lower::Markdown;

use crate::error::Result;
use crate::error::markdown::MarkdownError;

/// A markdown file split into its frontmatter block and its body.
pub struct Document<'a> {
    /// The text between the fences, absent when the file opens with content.
    pub frontmatter: Option<&'a str>,
    /// The language that text is written in, decided by the fence that opened
    /// it and meaningless without `frontmatter`.
    pub dialect: Dialect,
    /// Everything after the closing fence, or the whole file without one.
    pub body: &'a str,
    /// Byte offset of `frontmatter` within the file, so a diagnostic points at
    /// the line the author wrote rather than at the block's own first line.
    pub offset: usize,
    /// Byte offset of `body` within the file, for the same reason.
    pub body_offset: usize,
}

impl<'a> Document<'a> {
    /// `source` as a document that is body from its first byte, which is what a
    /// `paths { sources { } }` file is read as: its frontmatter came from the
    /// page that named it, so a fence at its top is a thematic break.
    pub fn whole(source: &'a str) -> Self {
        Self {
            frontmatter: None,
            dialect: Dialect::Yaml,
            body: source,
            offset: 0,
            body_offset: 0,
        }
    }

    /// Split `source`; a file whose first line is not a fence has no
    /// frontmatter, which is not an error.
    pub fn split(source: &'a str, path: &str) -> Result<Self> {
        let trimmed = source.trim_start_matches(['\u{feff}', '\n', '\r']);
        let bare = |body| Self {
            frontmatter: None,
            dialect: Dialect::Yaml,
            body,
            offset: 0,
            body_offset: 0,
        };

        let Some(opened) = FENCES.iter().find(|row| trimmed.starts_with(row.open)) else {
            return Ok(bare(source));
        };
        let fence = opened.open;
        let rest = Self::blank(&trimmed[fence.len()..]);
        let Some(rest) = rest
            .strip_prefix('\n')
            .or_else(|| rest.strip_prefix("\r\n"))
        else {
            return Ok(bare(source));
        };

        let start = source.len() - rest.len();
        let closing = |rest: &str, i: usize| {
            let before = &rest[..i];
            let after = Self::blank(&rest[i + fence.len()..]);
            (before.is_empty() || before.ends_with('\n'))
                && (after.is_empty() || after.starts_with('\n') || after.starts_with("\r\n"))
        };
        let end = rest
            .match_indices(fence)
            .find(|(i, _)| closing(rest, *i))
            .map(|(i, _)| i)
            .ok_or_else(|| MarkdownError::UnterminatedFrontmatter {
                path: path.to_owned(),
                fence: fence.to_owned(),
                src: miette::NamedSource::new(path, source.to_owned()),
                span: (
                    start - fence.len() - Self::newline_before(source, start),
                    fence.len(),
                )
                    .into(),
            })?;

        let after = Self::blank(&rest[end + fence.len()..]);
        let body = after
            .strip_prefix('\n')
            .or_else(|| after.strip_prefix("\r\n"))
            .unwrap_or(after);
        Ok(Self {
            frontmatter: Some(&rest[..end]),
            dialect: opened.dialect,
            body,
            offset: start,
            body_offset: source.len() - body.len(),
        })
    }

    /// `text` with the horizontal whitespace at its front removed, which is
    /// what may sit between a fence and the end of its line.
    fn blank(text: &str) -> &str {
        text.trim_start_matches([' ', '\t'])
    }

    /// The width of the line ending immediately before `at`: 2 for CRLF, 1 for
    /// LF.
    fn newline_before(source: &str, at: usize) -> usize {
        if source[..at].ends_with("\r\n") { 2 } else { 1 }
    }

    /// The block this document declares, read in its own dialect; an absent
    /// block is the empty one, which is still walked so a schema requiring a
    /// field fails on the page that wrote none.
    pub fn block(&self, path: &str, source: &str) -> Result<Block> {
        match self.frontmatter {
            Some(text) => self.dialect.parse(text, self.offset, path, source),
            None => Ok(Block::empty()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_a_frontmatter_block() {
        let doc = Document::split("---\ntitle: A\n---\n# Heading\n", "a.md").expect("split");
        assert_eq!(doc.frontmatter, Some("title: A\n"));
        assert_eq!(doc.dialect, Dialect::Yaml);
        assert_eq!(doc.body, "# Heading\n");
    }

    #[test]
    fn the_fence_decides_the_dialect() {
        for fence in FENCES {
            let open = fence.open;
            let source = format!("{open}\ntitle = 1\n{open}\nBody.\n");
            let doc = Document::split(&source, "a.md").expect("split");
            assert_eq!(doc.dialect, fence.dialect, "`{open}`");
            assert_eq!(doc.frontmatter, Some("title = 1\n"), "`{open}`");
            assert_eq!(doc.body, "Body.\n", "`{open}`");
        }
    }

    #[test]
    fn a_fence_may_carry_trailing_whitespace() {
        let doc = Document::split("--- \ntitle: A\n---\t\n# Heading\n", "a.md").expect("split");
        assert_eq!(doc.frontmatter, Some("title: A\n"));
        assert_eq!(doc.body, "# Heading\n");
        let source = "+++ \ntitle = \"A\"\n+++ \nBody.\n";
        let doc = Document::split(source, "a.md").expect("split");
        assert_eq!(&source[doc.offset..doc.offset + 12], "title = \"A\"\n");
        assert_eq!(&source[doc.body_offset..], "Body.\n");
    }

    #[test]
    fn the_block_knows_where_it_sits_in_the_file() {
        let source = "+++\ntitle = \"A\"\n+++\nBody.\n";
        let doc = Document::split(source, "a.md").expect("split");
        assert_eq!(&source[doc.offset..doc.offset + 12], "title = \"A\"\n");
        assert_eq!(&source[doc.body_offset..], "Body.\n");
    }

    #[test]
    fn a_file_without_a_block_is_all_body() {
        let doc = Document::split("# Heading\n", "a.md").expect("split");
        assert_eq!(doc.frontmatter, None);
        assert_eq!(doc.body, "# Heading\n");
    }

    #[test]
    fn an_unterminated_block_is_an_error() {
        for fence in FENCES {
            let open = fence.open;
            let source = format!("{open}\ntitle = 1\n");
            assert!(Document::split(&source, "a.md").is_err(), "`{open}`");
        }
    }

    #[test]
    fn only_its_own_fence_closes_a_block() {
        assert!(Document::split("+++\ntitle = 1\n---\nBody.\n", "a.md").is_err());
    }

    #[test]
    fn a_closing_fence_has_to_be_alone_on_its_line() {
        assert!(Document::split("---\ntitle: A\n--- and more\n\nBody\n", "a.md").is_err());
        let doc = Document::split("---\ntitle: A\n--- and more\n---\nBody\n", "a.md")
            .expect("the bare fence closes it");
        assert_eq!(doc.body, "Body\n");
    }

    #[test]
    fn a_crlf_file_splits_and_labels_the_whole_fence() {
        let doc = Document::split("---\r\ntitle: A\r\n---\r\nBody.\r\n", "a.md").expect("split");
        assert_eq!(doc.frontmatter, Some("title: A\r\n"));
        assert_eq!(doc.body, "Body.\r\n");

        let source = "---\r\ntitle: A\r\n";
        let Err(error) = Document::split(source, "a.md") else {
            panic!("an unterminated block is an error");
        };
        let rendered = format!("{error:?}");
        assert!(
            !rendered.contains("--\r"),
            "underlines the fence: {rendered}"
        );
    }

    #[test]
    fn a_thematic_break_is_not_a_block() {
        let doc = Document::split("--- not a fence\n", "a.md").expect("split");
        assert_eq!(doc.frontmatter, None);
    }

    #[test]
    fn a_page_without_frontmatter_still_has_an_empty_block() {
        let doc = Document::split("Body.\n", "a.md").expect("split");
        let block = doc.block("a.md", "Body.\n").expect("an empty block");
        assert!(block.dict.is_empty());
    }
}
