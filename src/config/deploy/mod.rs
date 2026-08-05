//! `deploy { }`: where the built files go.

pub mod s3;
pub mod ssh;

use crate::config::dispatch::Kind::Block as Nested;
use crate::config::dispatch::{Block, Section};
use crate::config::{S3Config, SshConfig};

/// Deploy destinations for the built files. Each backend is an optional block
/// under `deploy { .. }`; adding one is a field here plus a backend in
/// [`crate::deploy`]. Credentials are never stored here; a backend reads them
/// from the environment at deploy time.
#[derive(Debug, Clone, Hash, Default)]
pub struct DeployConfig {
    /// An S3-compatible bucket (AWS S3, Cloudflare R2, ..).
    pub s3: Option<S3Config>,
    /// A host reachable over SSH, files transferred with SFTP.
    pub ssh: Option<SshConfig>,
}

/// The `deploy { .. }` section: one block per destination backend.
impl Section for DeployConfig {
    const RULES: Block<Self> = Block(&[
        (
            "s3",
            Nested(S3Config::rows),
            "Upload to S3 or an S3-compatible bucket. Its presence turns it on.",
            |c, n, t| S3Config::optional(&mut c.s3, n, t),
        ),
        (
            "ssh",
            Nested(SshConfig::rows),
            "Upload over SSH. Its presence turns it on.",
            |c, n, t| SshConfig::optional(&mut c.ssh, n, t),
        ),
    ]);
}
