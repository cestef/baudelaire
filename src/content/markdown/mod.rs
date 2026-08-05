//! Markdown pages: a source dialect that lowers to Typst.
//!
//! A `.md` file under `content/` is a page like any other. It is not a second
//! pipeline: [`Markdown::lower`] turns the body into Typst source and the
//! frontmatter block into a Typst dict, and from there the engine sees exactly
//! what it sees for a `.typ` page. Highlighting, math, link resolution,
//! sidecars, the transform chain and the cache fingerprint all apply unchanged,
//! because by then there is nothing markdown-shaped left.
//!
//! Frontmatter is KDL, in a `---` fenced block at the top of the file. KDL is
//! what `config.kdl` is written in, so a site has one configuration language
//! rather than three, and the parser and its diagnostics are the ones already
//! here.

mod frontmatter;
mod lower;

pub use frontmatter::Fields;
pub use lower::Markdown;

use crate::error::Result;
use crate::error::markdown::MarkdownError;

/// The fence that opens and closes a frontmatter block.
const FENCE: &str = "---";

/// A markdown file split into its frontmatter block and its body.
pub struct Document<'a> {
    /// The KDL between the fences, absent when the file opens with content.
    pub frontmatter: Option<&'a str>,
    /// Everything after the closing fence, or the whole file without one.
    pub body: &'a str,
    /// Byte offset of `frontmatter` within the file, so a KDL diagnostic can
    /// point at the line the author wrote rather than at the block's own first
    /// line.
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

        let Some(rest) = trimmed.strip_prefix(FENCE) else {
            return Ok(Self {
                frontmatter: None,
                body: source,
                offset: 0,
                body_offset: 0,
            });
        };
        // `---` opens a block only on a line of its own; anything else on that
        // line is a thematic break or a setext heading, and the file is body.
        let Some(rest) = rest
            .strip_prefix('\n')
            .or_else(|| rest.strip_prefix("\r\n"))
        else {
            return Ok(Self {
                frontmatter: None,
                body: source,
                offset: 0,
                body_offset: 0,
            });
        };

        let start = source.len() - rest.len();
        let end = rest
            .match_indices(FENCE)
            .find(|(i, _)| {
                let before = &rest[..*i];
                before.is_empty() || before.ends_with('\n')
            })
            .map(|(i, _)| i)
            .ok_or_else(|| MarkdownError::UnterminatedFrontmatter {
                path: path.to_owned(),
                src: miette::NamedSource::new(path, source.to_owned()),
                span: (start - FENCE.len() - 1, FENCE.len()).into(),
            })?;

        let after = &rest[end + FENCE.len()..];
        let body = after
            .strip_prefix('\n')
            .or_else(|| after.strip_prefix("\r\n"))
            .unwrap_or(after);
        Ok(Self {
            frontmatter: Some(&rest[..end]),
            body,
            offset: start,
            body_offset: source.len() - body.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_a_frontmatter_block() {
        let doc = Document::split("---\ntitle \"A\"\n---\n# Heading\n", "a.md").expect("split");
        assert_eq!(doc.frontmatter, Some("title \"A\"\n"));
        assert_eq!(doc.body, "# Heading\n");
    }

    #[test]
    fn a_file_without_a_block_is_all_body() {
        let doc = Document::split("# Heading\n", "a.md").expect("split");
        assert_eq!(doc.frontmatter, None);
        assert_eq!(doc.body, "# Heading\n");
    }

    #[test]
    fn an_unterminated_block_is_an_error() {
        assert!(Document::split("---\ntitle \"A\"\n", "a.md").is_err());
    }

    #[test]
    fn a_thematic_break_is_not_a_block() {
        let doc = Document::split("--- not a fence\n", "a.md").expect("split");
        assert_eq!(doc.frontmatter, None);
    }
}
