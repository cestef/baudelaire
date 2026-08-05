//! `security { csp { } }`: the generated `Content-Security-Policy`.

use crate::config::dispatch::Kind::{Flag, Text, Url};
use crate::config::dispatch::{Block, Section};
use crate::config::node::NodeExt;

/// A generated `Content-Security-Policy`.
///
/// Each directive is the value it is given, verbatim: a CSP source list is its
/// own small language (`'self'`, `https:`, a host, `'unsafe-inline'`), and
/// inventing a second spelling for it would help nobody. What this adds is the
/// half no author can write down, the digest of every inline script and style
/// the build produced.
#[derive(Debug, Clone, Hash)]
pub struct CspConfig {
    /// Whether a policy is emitted at all; flipped by the block's presence.
    pub enabled: bool,
    /// Enforce it. Off emits `Content-Security-Policy-Report-Only`, which
    /// reports violations and blocks nothing: how a policy is rolled out.
    pub enforce: bool,
    /// Add the digest of every inline `<script>` and `<style>` the build
    /// produced to the script and style directives, which is what lets a strict
    /// policy coexist with the inline blocks a page needs.
    pub hashes: bool,
    /// `default-src`, the fallback every unstated fetch directive inherits.
    pub default: Option<String>,
    /// `script-src`, `style-src`, and the rest, each stated only if set.
    pub script: Option<String>,
    pub style: Option<String>,
    pub img: Option<String>,
    pub font: Option<String>,
    pub connect: Option<String>,
    pub frame: Option<String>,
    pub object: Option<String>,
    /// `base-uri`: what a `<base>` may point the page's relative URLs at.
    pub base: Option<String>,
    /// `form-action`: where a form may submit.
    pub form: Option<String>,
    /// `report-uri`: where a violation report is posted.
    pub report: Option<String>,
}

impl Default for CspConfig {
    fn default() -> Self {
        // opt-in as a whole, like every other generated artifact: a policy this
        // tool invented for a site that never asked for one would be either
        // useless or breaking. Once the block is present, it enforces (a
        // report-only policy is a rollout step, not a destination) and carries
        // the inline digests, which is the half an author cannot write.
        Self {
            enabled: false,
            enforce: true,
            hashes: true,
            // The one directive with a default: a policy with no `default-src`
            // restricts nothing it does not name, which is not what a block
            // saying `csp { }` means.
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

/// The `security { csp { .. } }` section: one key per directive, each taking a
/// CSP source list verbatim.
impl Section for CspConfig {
    fn enable(&mut self) -> bool {
        self.enabled = true;
        true
    }

    const RULES: Block<Self> = Block(&[
        (
            "enforce",
            Flag,
            "Enforce the policy. Off reports violations without blocking anything.",
            |c, n, t| {
                c.enforce = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "hashes",
            Flag,
            "Add the digest of every inline script and style the build produced. Turns `html { pretty }` off, since a digest has to cover the bytes as served.",
            |c, n, t| {
                c.hashes = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "default",
            Text,
            "`default-src`: what every unstated fetch directive falls back to.",
            |c, n, t| {
                c.default = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        ("script", Text, "`script-src`.", |c, n, t| {
            c.script = Some(n.string(t, 0)?);
            Ok(())
        }),
        ("style", Text, "`style-src`.", |c, n, t| {
            c.style = Some(n.string(t, 0)?);
            Ok(())
        }),
        ("img", Text, "`img-src`.", |c, n, t| {
            c.img = Some(n.string(t, 0)?);
            Ok(())
        }),
        ("font", Text, "`font-src`.", |c, n, t| {
            c.font = Some(n.string(t, 0)?);
            Ok(())
        }),
        ("connect", Text, "`connect-src`.", |c, n, t| {
            c.connect = Some(n.string(t, 0)?);
            Ok(())
        }),
        ("frame", Text, "`frame-src`.", |c, n, t| {
            c.frame = Some(n.string(t, 0)?);
            Ok(())
        }),
        ("object", Text, "`object-src`.", |c, n, t| {
            c.object = Some(n.string(t, 0)?);
            Ok(())
        }),
        (
            "base",
            Text,
            "`base-uri`: what a `<base>` may repoint relative URLs at.",
            |c, n, t| {
                c.base = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "form",
            Text,
            "`form-action`: where a form may submit.",
            |c, n, t| {
                c.form = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "report",
            Url,
            "`report-uri`: where a violation report is posted.",
            |c, n, t| {
                c.report = Some(n.url(t, 0)?);
                Ok(())
            },
        ),
    ]);
}
