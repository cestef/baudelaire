//! `lint { budget { } }`: per-page weight limits.

use crate::config::dispatch::Kind::{Flag, Size};
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::ui::Bytes;

/// Per-page weight limits, in bytes. Each is the ceiling for one class of what
/// a page ships; `None` is no limit.
#[derive(Debug, Clone, Hash)]
pub struct BudgetConfig {
    /// Fail the build when a page is over. Off, the same report is a warning.
    pub strict: bool,
    /// The page's own markup, as written to `dist`.
    pub html: Option<Bytes>,
    /// Every script the page loads, plus its inline `<script>` bodies.
    pub js: Option<Bytes>,
    /// Every stylesheet it loads, plus its inline `<style>` bodies.
    pub css: Option<Bytes>,
    /// Every image it references, responsive candidates excluded.
    pub images: Option<Bytes>,
    /// All of the above at once, the page's total transfer weight.
    pub total: Option<Bytes>,
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            strict: true,
            html: None,
            js: None,
            css: None,
            images: None,
            total: None,
        }
    }
}

impl Section for BudgetConfig {
    const RULES: Block<Self> = Block(&[
        (
            "strict",
            Flag,
            "Fail the build when a page is over. Off, the same report is a warning.",
            |c, n, t| {
                c.strict = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "html",
            Size,
            "The page's own markup, as written to the output directory.",
            |c, n, t| {
                c.html = Some(n.size(t, 0)?);
                Ok(())
            },
        ),
        (
            "js",
            Size,
            "Every script the page loads, plus its inline `<script>` bodies.",
            |c, n, t| {
                c.js = Some(n.size(t, 0)?);
                Ok(())
            },
        ),
        (
            "css",
            Size,
            "Every stylesheet it loads, plus its inline `<style>` bodies.",
            |c, n, t| {
                c.css = Some(n.size(t, 0)?);
                Ok(())
            },
        ),
        (
            "images",
            Size,
            "Every image it references, responsive alternatives excluded.",
            |c, n, t| {
                c.images = Some(n.size(t, 0)?);
                Ok(())
            },
        ),
        (
            "total",
            Size,
            "All of the above at once: the page's whole transfer weight.",
            |c, n, t| {
                c.total = Some(n.size(t, 0)?);
                Ok(())
            },
        ),
    ]);
}
