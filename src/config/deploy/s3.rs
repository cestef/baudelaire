//! `deploy { s3 { } }`: an S3-compatible bucket.

use dispatch_derive::Table;

use crate::config::dispatch::Kind::Number;
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;
use crate::config::vocab::rule;

/// An S3-compatible bucket target. Works against AWS S3 by default; set
/// `endpoint` for R2 or any S3-compatible host.
#[derive(Debug, Clone, Hash, Table)]
pub struct S3Config {
    /// The bucket uploaded into.
    #[key(text)]
    pub bucket: String,

    /// The API endpoint, for an S3-compatible host such as R2.
    ///
    /// `None` targets AWS at the region's default host.
    #[key(opt url)]
    pub endpoint: Option<String>,

    /// The bucket's region.
    ///
    /// Resolved by [`S3Config::region`] when unset.
    #[key(opt text)]
    pub region: Option<String>,

    /// A key prefix every uploaded object goes under.
    #[key(text)]
    pub prefix: String,

    /// Delete remote objects this build did not produce.
    #[key(flag)]
    pub delete: bool,

    /// How many objects are transferred at once. Unset, as many as the build has threads.
    #[key(custom(
        Number,
        |c: &Self| c.concurrency.into(),
        |c: &mut Self, n: &kdl::KdlNode, t: &str| {
            let at_once: u16 = n.arg(t, 0)?.bounded(t, NodeExt::span(n), 1, u16::MAX)?;
            c.concurrency = Some(usize::from(at_once));
            Ok(())
        },
    ))]
    pub concurrency: Option<usize>,
}

impl S3Config {
    /// The region code the request is signed under: a stated `region`, else
    /// `auto` when a custom `endpoint` names a non-AWS host, else AWS's own
    /// `us-east-1`.
    pub fn region(&self) -> &str {
        match (&self.region, &self.endpoint) {
            (Some(region), _) => region,
            (None, Some(_)) => "auto",
            (None, None) => "us-east-1",
        }
    }
}

impl Default for S3Config {
    fn default() -> Self {
        Self {
            bucket: String::new(),
            endpoint: None,
            region: None,
            prefix: String::new(),
            delete: true,
            concurrency: None,
        }
    }
}
