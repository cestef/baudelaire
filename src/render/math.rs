//! The stylesheet MathML output depends on: what is in it, and where it is
//! served from.
//!
//! typst-html injects these rules into the `<head>` of every page that holds an
//! equation. It is the same block written again on each of them, it is not
//! cacheable, and every math page pays for it. Under
//! `html { math { styles "link" } }` the build serves it once instead:
//! [`crate::engine::asset`] writes the bytes before any page renders, and
//! [`crate::render::transform`] takes typst's copy back out and points the page
//! at the file.
//!
//! The injection is not a show rule, so no [`crate::world::rules`] entry reaches
//! it: it happens in typst-html's `html_document`, gated on an introspector
//! query for equations, after every rule has run. Taking it out of the finished
//! DOM is the only lever there is, which means the block has to be *recognised*,
//! and typst's copy of the text is private.
//!
//! So [`MathSheet::TYPST`] pins it, the way `world/rules/image.rs` pins typst's
//! `ToCss`. Nothing derives from the pin except the comparison: a page keeps its
//! inline block unless the text matches exactly, so a typst release that edits
//! one declaration degrades to `styles "inline"` rather than dropping rules the
//! output needs. `tests/scenarios/math.kdl` fails loudly so the pin is re-cut.

/// The stylesheet this crate serves for MathML output.
pub struct MathSheet;

impl MathSheet {
    /// Path relative to the asset root. A site or theme shipping its own
    /// `math.css` replaces this file whole, the way it replaces any other asset
    /// a theme beneath it provides.
    pub const REL: &'static str = "math.css";

    /// The rules typst-html injects, pinned verbatim.
    ///
    /// Kept as CSS rather than a Rust string so a re-cut after a typst bump is a
    /// diff of a stylesheet and nothing else.
    const TYPST: &'static str = include_str!("math/typst.css");

    /// The bytes served at [`REL`](Self::REL).
    ///
    /// Only typst's rules for now. The equation numbering layout and the math
    /// `@font-face` land here too, which is the whole reason this is a file.
    pub fn text() -> &'static str {
        Self::TYPST
    }

    /// Whether `text` is the block typst injects.
    ///
    /// Compared ignoring the surrounding whitespace only: the pinned copy is a
    /// file, so it ends with the newline every file ends with, and the injected
    /// one does not.
    pub fn injected(text: &str) -> bool {
        text.trim() == Self::TYPST.trim()
    }
}
