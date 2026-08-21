//! `deploy { }`: where the built files go.

pub mod s3;
pub mod ssh;

use dispatch_derive::Table;

use crate::config::dispatch::{Block, Section};
use crate::config::vocab::rule;
use crate::config::{S3Config, SshConfig};

/// Deploy destinations for the built files, one optional block per backend.
/// Credentials are never stored here; a backend reads them from the environment
/// at deploy time.
#[derive(Debug, Clone, Hash, Default, Table)]
pub struct DeployConfig {
    /// Upload to S3 or an S3-compatible bucket. Its presence turns it on.
    #[key(opt nested(S3Config))]
    pub s3: Option<S3Config>,

    /// Upload over SSH. Its presence turns it on.
    ///
    /// Files are transferred with SFTP.
    #[key(opt nested(SshConfig))]
    pub ssh: Option<SshConfig>,
}
