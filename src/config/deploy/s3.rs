//! `deploy { s3 { } }`: an S3-compatible bucket.

use crate::config::dispatch::Kind::{Flag, Text, Url};
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;

/// An S3-compatible bucket target. Works against AWS S3 by default; set
/// `endpoint` for R2 or any S3-compatible host.
#[derive(Debug, Clone, Hash)]
pub struct S3Config {
    /// Bucket name.
    pub bucket: String,
    /// S3 endpoint host, e.g. `https://ACCOUNT.r2.cloudflarestorage.com`. `None`
    /// targets AWS at the region's default host.
    pub endpoint: Option<String>,
    /// Region code, resolved by [`S3Config::region`] when unset.
    pub region: Option<String>,
    /// Key prefix every uploaded object is placed under (a subdirectory in the
    /// bucket). Empty by default.
    pub prefix: String,
    /// Delete remote objects under `prefix` that the build no longer produces.
    pub delete: bool,
}

impl S3Config {
    /// The region code the request is signed under.
    ///
    /// A stated `region` always wins. Otherwise it follows the target: AWS is
    /// signed as `us-east-1`, its own default, and a custom `endpoint` is not
    /// AWS, so it is signed as `auto`, which is what R2 and most S3-compatible
    /// hosts want. Defaulting the second case to `us-east-1` meant an R2 user
    /// who set `endpoint` and left `region` alone got a 403 with nothing in it
    /// naming the region.
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
            // None targets AWS; R2/custom hosts set it.
            endpoint: None,
            // Resolved from the target by `S3Config::region` when unset.
            region: None,
            prefix: String::new(),
            // reconcile: remove what the build no longer produces.
            delete: true,
        }
    }
}

/// The `s3 { .. }` block: presence enables the S3 backend.
impl Section for S3Config {
    const RULES: Block<Self> = Block(&[
        ("bucket", Text, "The bucket uploaded into.", |c, n, t| {
            c.bucket = n.string(t, 0)?;
            Ok(())
        }),
        (
            "endpoint",
            Url,
            "The API endpoint, for an S3-compatible host such as R2.",
            |c, n, t| {
                c.endpoint = Some(n.url(t, 0)?);
                Ok(())
            },
        ),
        ("region", Text, "The bucket's region.", |c, n, t| {
            c.region = Some(n.string(t, 0)?);
            Ok(())
        }),
        (
            "prefix",
            Text,
            "A key prefix every uploaded object goes under.",
            |c, n, t| {
                c.prefix = n.string(t, 0)?;
                Ok(())
            },
        ),
        (
            "delete",
            Flag,
            "Delete remote objects this build did not produce.",
            |c, n, t| {
                c.delete = n.boolean(t, 0)?;
                Ok(())
            },
        ),
    ]);
}
