//! `generate { llms { } }`: `llms.txt`.

use crate::config::dispatch::Kind::Text;
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;

/// `llms.txt` generation ([llmstxt.org]): a Markdown index of the site's pages
/// for LLM consumption. Enabled by the presence of a `generate { llms }` block.
///
/// [llmstxt.org]: https://llmstxt.org
#[derive(Debug, Clone, Hash, Default)]
pub struct LlmsConfig {
    /// Whether to emit `llms.txt`.
    pub enabled: bool,
    /// Optional one-line summary rendered as the blockquote under the title.
    pub summary: Option<String>,
}

impl Section for LlmsConfig {
    const RULES: Block<Self> = Block(&[(
        "summary",
        Text,
        "A one-line description of the site, put at the top of the file.",
        |c, n, t| {
            c.summary = Some(n.string(t, 0)?);
            Ok(())
        },
    )]);

    fn enable(&mut self) -> bool {
        self.enabled = true;
        true
    }
}
