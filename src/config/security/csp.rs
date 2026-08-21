//! `security { csp { } }`: the generated `Content-Security-Policy`.

use dispatch_derive::Table;

use crate::config::dispatch::{Block, Section};
use crate::config::vocab::rule;

/// A generated `Content-Security-Policy`. Each directive is the value it is
/// given, verbatim.
#[derive(Debug, Clone, Hash, Table)]
#[table(hook(switch = enabled))]
pub struct CspConfig {
    /// Whether a policy is emitted at all.
    pub enabled: bool,

    /// Enforce the policy. Off reports violations without blocking anything.
    #[key(flag)]
    pub enforce: bool,

    /// Add the digest of every inline script and style the build produced. Turns `html { pretty }` off, since a digest has to cover the bytes as served.
    #[key(flag)]
    pub hashes: bool,

    /// `default-src`: what every unstated fetch directive falls back to.
    #[key(opt text)]
    pub default: Option<String>,

    /// `script-src`.
    #[key(opt text)]
    pub script: Option<String>,

    /// `style-src`.
    #[key(opt text)]
    pub style: Option<String>,

    /// `img-src`.
    #[key(opt text)]
    pub img: Option<String>,

    /// `font-src`.
    #[key(opt text)]
    pub font: Option<String>,

    /// `connect-src`.
    #[key(opt text)]
    pub connect: Option<String>,

    /// `frame-src`.
    #[key(opt text)]
    pub frame: Option<String>,

    /// `object-src`.
    #[key(opt text)]
    pub object: Option<String>,

    /// `base-uri`: what a `<base>` may repoint relative URLs at.
    #[key(opt text)]
    pub base: Option<String>,

    /// `form-action`: where a form may submit.
    #[key(opt text)]
    pub form: Option<String>,

    /// `report-uri`: where a violation report is posted.
    #[key(opt url)]
    pub report: Option<String>,
}

impl Default for CspConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            enforce: true,
            hashes: true,
            default: Some("'self'".into()),
            script: None,
            style: None,
            img: None,
            font: None,
            connect: None,
            frame: None,
            object: None,
            base: None,
            form: None,
            report: None,
        }
    }
}
