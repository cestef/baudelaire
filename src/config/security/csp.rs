//! `security { csp { } }`: the generated `Content-Security-Policy`.

use crate::config::dispatch::Kind::{Flag, Text, Url};
use crate::config::dispatch::{Block, Section, Switch};
use crate::config::node::NodeExt;

/// A generated `Content-Security-Policy`. Each directive is the value it is
/// given, verbatim.
#[derive(Debug, Clone, Hash)]
pub struct CspConfig {
    /// Whether a policy is emitted at all.
    pub enabled: bool,
    /// Enforce it. Off emits `Content-Security-Policy-Report-Only`, which
    /// reports violations and blocks nothing: how a policy is rolled out.
    pub enforce: bool,
    /// Add the digest of every inline `<script>` and `<style>` the build
    /// produced to the script and style directives.
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

impl Section for CspConfig {
    const SWITCH: Option<Switch<Self>> = Some(Switch {
        set: |c, on| c.enabled = on,
        on: |c| c.enabled,
    });

    const RULES: Block<Self> = Block(&[
        (
            "enforce",
            Flag,
            "Enforce the policy. Off reports violations without blocking anything.",
            |c| c.enforce.into(),
            |c, n, t| {
                c.enforce = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "hashes",
            Flag,
            "Add the digest of every inline script and style the build produced. Turns `html { pretty }` off, since a digest has to cover the bytes as served.",
            |c| c.hashes.into(),
            |c, n, t| {
                c.hashes = n.boolean(t, 0)?;
                Ok(())
            },
        ),
        (
            "default",
            Text,
            "`default-src`: what every unstated fetch directive falls back to.",
            |c| c.default.clone().into(),
            |c, n, t| {
                c.default = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "script",
            Text,
            "`script-src`.",
            |c| c.script.clone().into(),
            |c, n, t| {
                c.script = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "style",
            Text,
            "`style-src`.",
            |c| c.style.clone().into(),
            |c, n, t| {
                c.style = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "img",
            Text,
            "`img-src`.",
            |c| c.img.clone().into(),
            |c, n, t| {
                c.img = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "font",
            Text,
            "`font-src`.",
            |c| c.font.clone().into(),
            |c, n, t| {
                c.font = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "connect",
            Text,
            "`connect-src`.",
            |c| c.connect.clone().into(),
            |c, n, t| {
                c.connect = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "frame",
            Text,
            "`frame-src`.",
            |c| c.frame.clone().into(),
            |c, n, t| {
                c.frame = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "object",
            Text,
            "`object-src`.",
            |c| c.object.clone().into(),
            |c, n, t| {
                c.object = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "base",
            Text,
            "`base-uri`: what a `<base>` may repoint relative URLs at.",
            |c| c.base.clone().into(),
            |c, n, t| {
                c.base = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "form",
            Text,
            "`form-action`: where a form may submit.",
            |c| c.form.clone().into(),
            |c, n, t| {
                c.form = Some(n.string(t, 0)?);
                Ok(())
            },
        ),
        (
            "report",
            Url,
            "`report-uri`: where a violation report is posted.",
            |c| c.report.clone().into(),
            |c, n, t| {
                c.report = Some(n.url(t, 0)?);
                Ok(())
            },
        ),
    ]);
}
