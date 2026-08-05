//! Markdown pages: a source dialect that lowers to Typst.
//!
//! A `.md` file under `content/` is a page like any other. It is not a second
//! pipeline: [`Markdown::lower`] turns the body into Typst source and the
//! frontmatter block into a Typst dict, and from there the engine sees exactly
//! what it sees for a `.typ` page. Highlighting, math, link resolution,
//! sidecars, the transform chain and the cache fingerprint all apply unchanged,
//! because by then there is nothing markdown-shaped left.
//!
//! Frontmatter is whatever its fence opens: `---` is YAML, `+++` is TOML, `;;;`
//! is KDL. A post pasted out of another generator therefore needs no rewriting,
//! and the fence is the whole of the declaration, so a block is never read as a
//! language it is not. See [`frontmatter`].

mod frontmatter;
mod lower;

pub use frontmatter::{Block, Dialect, FENCES, Spans};
pub use lower::Markdown;

use crate::error::Result;
use crate::error::markdown::MarkdownError;

/// A markdown file split into its frontmatter block and its body.
pub struct Document<'a> {
    /// The text between the fences, absent when the file opens with content.
    pub frontmatter: Option<&'a str>,
    /// The language that text is written in, decided by the fence that opened
    /// it. Meaningless without `frontmatter`, and defaulted rather than
    /// optional: an absent block is an empty block of some dialect, and which
    /// one cannot matter because there is nothing in it.
    pub dialect: Dialect,
    /// Everything after the closing fence, or the whole file without one.
    pub body: &'a str,
    /// Byte offset of `frontmatter` within the file, so a diagnostic can point
    /// at the line the author wrote rather than at the block's own first line.
    pub offset: usize,
    /// Byte offset of `body` within the file, for the same reason: a fault the
    /// lowering finds is reported against the page, not against the body.
    pub body_offset: usize,
}

impl<'a> Document<'a> {
    /// Split `source`. A file whose first line is not a fence has no
    /// frontmatter, which is not an error: a page may declare nothing.
    pub fn split(source: &'a str, path: &str) -> Result<Self> {
        let trimmed = source.trim_start_matches(['\u{feff}', '\n', '\r']);
        let bare = |body| Self {
            frontmatter: None,
            dialect: Dialect::Yaml,
            body,
            offset: 0,
            body_offset: 0,
        };

        // Whichever fence the file opens with decides the language. Read from
        // the table, so a new dialect is openable the moment it is listed.
        let Some((fence, dialect)) = FENCES
            .iter()
            .find(|(fence, _)| trimmed.starts_with(*fence))
            .map(|(fence, dialect)| (*fence, *dialect))
        else {
            return Ok(bare(source));
        };
        let rest = &trimmed[fence.len()..];
        // A fence opens a block only on a line of its own; anything else on that
        // line is a thematic break or a setext heading, and the file is body.
        let Some(rest) = rest
            .strip_prefix('\n')
            .or_else(|| rest.strip_prefix("\r\n"))
        else {
            return Ok(bare(source));
        };

        let start = source.len() - rest.len();
        let end = rest
            .match_indices(fence)
            .find(|(i, _)| {
                let before = &rest[..*i];
                before.is_empty() || before.ends_with('\n')
            })
            .map(|(i, _)| i)
            .ok_or_else(|| MarkdownError::UnterminatedFrontmatter {
                path: path.to_owned(),
                fence: fence.to_owned(),
                src: miette::NamedSource::new(path, source.to_owned()),
                span: (start - fence.len() - 1, fence.len()).into(),
            })?;

        let after = &rest[end + fence.len()..];
        let body = after
            .strip_prefix('\n')
            .or_else(|| after.strip_prefix("\r\n"))
            .unwrap_or(after);
        Ok(Self {
            frontmatter: Some(&rest[..end]),
            dialect,
            body,
            offset: start,
            body_offset: source.len() - body.len(),
        })
    }

    /// The block this document declares, read in its own dialect. An absent
    /// block is the empty one, which is still walked: a collection that requires
    /// fields is not satisfied by a page that declares none, and the emptiest
    /// page is exactly what a schema exists to catch.
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

    /// Each fence opens its own language, and closes it: a block is never
    /// terminated by a different dialect's fence.
    #[test]
    fn the_fence_decides_the_dialect() {
        for (fence, dialect) in FENCES {
            let source = format!("{fence}\ntitle = 1\n{fence}\nBody.\n");
            let doc = Document::split(&source, "a.md").expect("split");
            assert_eq!(doc.dialect, *dialect, "`{fence}`");
            assert_eq!(doc.frontmatter, Some("title = 1\n"), "`{fence}`");
            assert_eq!(doc.body, "Body.\n", "`{fence}`");
        }
    }

    /// The offsets are what every span a dialect records is shifted by, so a
    /// diagnostic underlines the page rather than the block.
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
        for (fence, _) in FENCES {
            let source = format!("{fence}\ntitle = 1\n");
            assert!(Document::split(&source, "a.md").is_err(), "`{fence}`");
        }
    }

    /// A closing fence has to be the one that opened: `---` does not end a
    /// `+++` block, or a TOML page whose body contains a thematic break would
    /// silently lose everything after it.
    #[test]
    fn only_its_own_fence_closes_a_block() {
        assert!(Document::split("+++\ntitle = 1\n---\nBody.\n", "a.md").is_err());
    }

    #[test]
    fn a_thematic_break_is_not_a_block() {
        let doc = Document::split("--- not a fence\n", "a.md").expect("split");
        assert_eq!(doc.frontmatter, None);
    }

    /// A page that declares nothing still produces a block, because a schema
    /// requiring a field has to fail on the page that wrote none.
    #[test]
    fn a_page_without_frontmatter_still_has_an_empty_block() {
        let doc = Document::split("Body.\n", "a.md").expect("split");
        let block = doc.block("a.md", "Body.\n").expect("an empty block");
        assert!(block.dict.is_empty());
    }
}
