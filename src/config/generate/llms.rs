//! `generate { llms { } }`: `llms.txt`.

use dispatch_derive::Table;

use crate::config::dispatch::{Block, Section};
use crate::config::vocab::rule;

/// `llms.txt` generation ([llmstxt.org]): a Markdown index of the site's pages
/// for LLM consumption. Enabled by the presence of a `generate { llms }` block.
///
/// [llmstxt.org]: https://llmstxt.org
#[derive(Debug, Clone, Hash, Default, Table)]
#[table(hook(switch = enabled))]
pub struct LlmsConfig {
    pub enabled: bool,

    /// A one-line description of the site, put at the top of the file.
    ///
    /// Rendered as the blockquote under the title.
    #[key(opt text)]
    pub summary: Option<String>,
}
