//! Generated (synthetic) pages: taxonomy term indexes and paginated collection
//! listings, each derived from the authored pages by a [`Generate`] pass.

use crate::config::Config;
use crate::content::{Collection, Page, Pagination, Taxonomy};
use crate::error::Result;

/// The inputs a generator reads; `pages` is a fixed snapshot taken before any
/// generated page joins the set.
pub(super) struct PlanCtx<'a> {
    pub config: &'a Config,
    pub entities: &'a crate::content::Registries,
    pub pages: &'a [Page],
    pub collections: &'a [Collection],
}

/// One synthetic-page generator: derives extra pages from the planned content.
pub(super) trait Generate {
    fn generate(&self, ctx: &PlanCtx) -> Result<Vec<Page>>;
}

/// The built-in generators, in run order.
pub(super) struct Generators(Vec<Box<dyn Generate>>);

impl Generators {
    pub(super) fn builtin() -> Self {
        Self(vec![Box::new(Taxonomy), Box::new(Pagination)])
    }

    pub(super) fn generate(&self, ctx: &PlanCtx) -> Result<Vec<Page>> {
        let mut out = Vec::new();
        for generator in &self.0 {
            out.extend(generator.generate(ctx)?);
        }
        Ok(out)
    }
}
