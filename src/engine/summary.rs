//! The end-of-build result line.

use std::path::Path;
use std::time::Duration;

use crate::ui::{Bytes, Count, Dur, Paths, Ui};

/// The end-of-build summary: one tight result line covering pages (and how many
/// were cached), assets processed, generated files, any warnings, the output
/// directory, and elapsed time. Owns its own rendering so [`crate::engine::Engine`]
/// only gathers the numbers.
pub(super) struct Summary<'a> {
    pub pages: usize,
    pub cached: usize,
    pub assets: usize,
    pub statics: usize,
    pub generated: usize,
    pub cards: usize,
    pub bytes: u64,
    pub warnings: usize,
    pub dist: &'a Path,
    pub elapsed: Duration,
}

impl Summary<'_> {
    pub(super) fn report(&self, ui: &Ui) {
        use owo_colors::OwoColorize;

        // Headline: what built and how long, terse enough to stand alone at
        // `--quiet`. Values are bold, their labels dimmed, so the eye lands on
        // the numbers. The breakdown and output path demote to tree sub-lines.
        ui.done(format_args!(
            "built {} {} {}",
            Count::pages(self.pages).styled(),
            "in".dimmed(),
            Dur(self.elapsed).bold(),
        ));

        // Each metric styles by kind: reused pages recede (fully dimmed, nothing
        // happened), fresh work keeps bold numbers, warnings turn yellow.
        let mut parts = Vec::new();
        if self.cached > 0 {
            parts.push(format!("{} cached", self.cached).dimmed().to_string());
        }
        if self.assets > 0 {
            parts.push(Count::assets(self.assets).styled().to_string());
        }
        if self.statics > 0 {
            parts.push(Count::statics(self.statics).styled().to_string());
        }
        if self.generated > 0 {
            parts.push(Count::files(self.generated).styled().to_string());
        }
        if self.cards > 0 {
            parts.push(Count::cards(self.cards).styled().to_string());
        }
        if self.bytes > 0 {
            parts.push(Bytes(self.bytes).styled().to_string());
        }
        if self.warnings > 0 {
            parts.push(Count::warnings(self.warnings).yellow().to_string());
        }

        let mut rows = Vec::new();
        if !parts.is_empty() {
            // Wrap under the tree row's content column (col 5) so a long
            // breakdown flows onto extra lines rather than off-screen.
            rows.push(crate::ui::wrap(&parts, 5, crate::ui::term_width()));
        }
        rows.push(Paths(&self.dist.display().to_string()).to_string());
        ui.tree(&rows);
    }
}
