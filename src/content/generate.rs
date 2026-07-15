//! Generated (synthetic) pages.
//!
//! Beyond the authored content pages, a build includes pages *derived* from
//! them — taxonomy term indexes, paginated collection listings. Each derivation
//! is a [`Generate`] pass; [`Generators::builtin`] is the single source of what
//! runs, so a new kind of generated page (archives, author pages, ...) is one
//! `impl Generate` plus one line here — mirroring `engine::process::Processors`
//! for emitted files.

use crate::config::Config;
use crate::content::{Collection, Page, Pagination, Taxonomy};
use crate::error::Result;

/// The inputs a generator reads: the config, the content pages planned so far
/// (a fixed snapshot taken before any generated page joins the set), and the
/// source collections.
pub(super) struct PlanCtx<'a> {
    pub config: &'a Config,
    pub pages: &'a [Page],
    pub collections: &'a [Collection],
}

/// One synthetic-page generator: derives extra pages from the planned content.
pub(super) trait Generate {
    fn generate(&self, ctx: &PlanCtx) -> Result<Vec<Page>>;
}

/// The built-in generators, in run order. THE single source of what synthetic
/// pages a build adds: a new kind is one `impl Generate` plus one line here.
pub(super) struct Generators(Vec<Box<dyn Generate>>);

impl Generators {
    pub(super) fn builtin() -> Self {
        Self(vec![Box::new(Taxonomy), Box::new(Pagination)])
    }

    /// Run every generator against the same content snapshot, concatenating
    /// their pages in registry order.
    pub(super) fn generate(&self, ctx: &PlanCtx) -> Result<Vec<Page>> {
        let mut out = Vec::new();
        for generator in &self.0 {
            out.extend(generator.generate(ctx)?);
        }
        Ok(out)
    }
}
