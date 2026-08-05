//! `html { highlight { } }`: syntax colours rewritten as CSS classes.

/// Turn typst's inline highlight colours into CSS classes, so a stylesheet owns
/// the palette and can follow a light/dark toggle.
///
/// typst-html bakes a highlight theme's colours into a `style="color: .."` on
/// every span, with no option to emit classes. A fixed colour cannot follow a
/// runtime theme switch, so the documented workaround was to author a `.tmTheme`
/// of *meaningless sentinel hex values* and remap each one with
/// `pre code [style*="e5d004"] { color: var(--kw) !important }`. That is what
/// this replaces.
#[derive(Debug, Clone, Hash, Default)]
pub struct HighlightConfig {
    /// Whether to rewrite at all; the block's presence turns it on.
    pub enabled: bool,
    /// Scope name to the colour the theme paints it, `keyword "#e5d004"`,
    /// mirroring the `.tmTheme`. A colour named here becomes `sx-<name>`;
    /// anything unnamed falls back to `sx-<hex>`, which still beats an
    /// attribute-substring selector.
    pub scopes: Vec<(String, String)>,
}

impl HighlightConfig {
    /// The class a highlight `colour` is rewritten to. The single naming rule,
    /// so the emitted markup and any generated stylesheet agree by construction.
    pub fn class(&self, colour: &str) -> String {
        let named = self
            .scopes
            .iter()
            .find(|(_, hex)| hex.eq_ignore_ascii_case(colour))
            .map(|(name, _)| name.as_str());
        format!(
            "sx-{}",
            named.unwrap_or_else(|| colour.trim_start_matches('#'))
        )
    }
}
