//! `deploy { s3 { } }`: an S3-compatible bucket.

use crate::config::dispatch::Kind::{Flag, Number, Text, Url};
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;
use crate::config::value::ValueExt;

/// An S3-compatible bucket target. Works against AWS S3 by default; set
/// `endpoint` for R2 or any S3-compatible host.
#[derive(Debug, Clone, Hash)]
pub struct S3Config {
    pub bucket: String,
    /// S3 endpoint host, e.g. `https://ACCOUNT.r2.cloudflarestorage.com`.
    /// `None` targets AWS at the region's default host.
    pub endpoint: Option<String>,
    /// Region code, resolved by [`S3Config::region`] when unset.
    pub region: Option<String>,
    /// Key prefix every uploaded object is placed under, a subdirectory in the
    /// bucket.
    pub prefix: String,
    /// Delete remote objects under `prefix` that the build no longer produces.
    pub delete: bool,
    /// How many objects are transferred at once. `None` is as many as the build
    /// has threads.
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

impl Section for S3Config {
    const RULES: Block<Self> = Block(&[
        (
            "bucket",
            Text,
            "The bucket uploaded into.",
            |c| c.bucket.clone().into(),
            |c, n, t| {
                c.bucket = n.string(t, 0)?;
                Ok(())
            },
        ),
        (
            "endpoint",
            Url,
            "The API endpoint, for an S3-compatible host such as R2.",
            |c| c.endpoint.clone().into(),
            |c, n, t| {
                c.endpoint = Some(n.url(t, 0)?);
                Ok(())
            },
        ),
        (
            "region",
            Text,
            "The bucket's region.",
            |c| c.region.clone().into(),
            |c, n, t| {
                c.region = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "prefix",
            Text,
            "A key prefix every uploaded object goes under.",
            |c| c.prefix.clone().into(),
            |c, n, t| {
                c.prefix = n.string(t, 0)?;
                Ok(())
            },
        ),
        (
            "delete",
            Flag,
            "Delete remote objects this build did not produce.",
            |c| c.delete.into(),
            |c, n, t| {
                c.delete = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "concurrency",
            Number,
            "How many objects are transferred at once. Unset, as many as the build has threads.",
            |c| c.concurrency.into(),
            |c, n, t| {
                let at_once: u16 = n.arg(t, 0)?.bounded(t, NodeExt::span(n), 1, u16::MAX)?;
                c.concurrency = Some(usize::from(at_once));
                Ok(())
            },
        ),
    ]);
}
