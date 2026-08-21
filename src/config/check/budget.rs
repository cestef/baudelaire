//! `check { budget { } }`: per-page weight limits.

use dispatch_derive::Table;

use crate::config::dispatch::{Block, Section};
use crate::config::vocab::rule;
use crate::ui::Bytes;

/// Per-page weight limits, in bytes. Each is the ceiling for one class of what
/// a page ships; `None` is no limit.
#[derive(Debug, Clone, Hash, Table)]
pub struct BudgetConfig {
    /// Fail the build when a page is over. Off, the same report is a warning.
    #[key(flag)]
    pub strict: bool,

    /// The page's own markup, as written to the output directory.
    #[key(opt size)]
    pub html: Option<Bytes>,

    /// Every script the page loads, plus its inline `<script>` bodies.
    #[key(opt size)]
    pub js: Option<Bytes>,

    /// Every stylesheet it loads, plus its inline `<style>` bodies.
    #[key(opt size)]
    pub css: Option<Bytes>,

    /// Every image it references, responsive alternatives excluded.
    #[key(opt size)]
    pub images: Option<Bytes>,

    /// All of the above at once: the page's whole transfer weight.
    #[key(opt size)]
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
