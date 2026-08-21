//! `typst { }`: engine knobs, `sys.inputs`, fonts, and the package registry.

pub mod fonts;

use dispatch_derive::Table;

use crate::config::FontConfig;
use crate::config::Value;
use crate::config::dispatch::Kind::{Table as Free, Url};
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::config::vocab::rule;

#[derive(Debug, Clone, Hash, Default, Table)]
pub struct TypstConfig {
    /// Typst language features to enable, or `-name` to disable one. `html` cannot be removed.
    ///
    /// `html` is always forced on in `world.rs`, so this list is purely
    /// additive.
    #[key(toggles)]
    pub features: Vec<String>,

    /// Values passed to every compile as `sys.inputs`, one `key value` line per entry.
    #[key(custom(
        Free,
        |c: &Self| Value::each(&c.inputs, |input| input.clone().into()),
        |c: &mut Self, n: &kdl::KdlNode, t: &str| {
            c.inputs = n.pairs(t)?;
            Ok(())
        },
    ))]
    pub inputs: Vec<(String, String)>,

    /// Where a compile looks for glyphs.
    #[key(nested(FontConfig))]
    pub fonts: FontConfig,

    /// Where typst packages are fetched from.
    ///
    /// A mirror of Typst Universe, stored without its trailing slash: the store
    /// joins `/preview/..` onto it, and a doubled slash is a 404 from some
    /// hosts. `None` is the official registry, and only `preview` is ever
    /// fetched.
    #[key(custom(
        Url,
        |c: &Self| c.registry.clone().into(),
        |c: &mut Self, n: &kdl::KdlNode, t: &str| {
            c.registry = Some(n.url(t, 0)?.trim_end_matches('/').to_owned());
            Ok(())
        },
    ))]
    pub registry: Option<String>,
}
