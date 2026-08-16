//! The stylesheet typst-html injects for MathML output, served once as a file
//! under `html { math { styles "link" } }` instead of inlined into every page
//! that holds an equation.

/// The stylesheet this crate serves for MathML output.
pub struct MathSheet;

impl MathSheet {
    /// The rules typst-html injects, pinned verbatim: a typst release that
    /// edits one degrades the page to its inline block rather than dropping
    /// rules the output needs.
    const TYPST: &'static str = include_str!("math/typst.css");

    /// The bytes served at the configured path.
    pub fn text() -> &'static str {
        Self::TYPST
    }

    /// Whether `text` is the block typst injects, compared ignoring the
    /// surrounding whitespace only.
    pub fn injected(text: &str) -> bool {
        text.trim() == Self::TYPST.trim()
    }
}
