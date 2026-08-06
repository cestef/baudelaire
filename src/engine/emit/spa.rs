//! Client-side navigation over the ordinary multi-file output.
//!
//! The site keeps building exactly as it did: every route stays a real document
//! that loads on its own. On top of that, a small runtime intercepts internal
//! link clicks, fetches the target page, and swaps one container, so shared
//! chrome (a header, a sidebar, an audio player) survives a navigation and the
//! browser skips a full parse. Progressive enhancement throughout: without
//! JavaScript nothing changes.
//!
//! The runtime ships two ways, from one source: `spa.js` at the `dist` root for
//! a site that just drops a `<script type="module">`, and the `baudelaire:spa`
//! virtual module for one that bundles its own entry and wants to call
//! `mountSpa` itself.

use super::script::Script;
use super::{Emit, Processor, Site};
use crate::config::{Config, Named, SpaConfig};
use crate::error::Result;

/// Emits the standalone navigation client.
pub(super) struct Spa;

impl Processor for Spa {
    fn enabled(&self, config: &Config) -> bool {
        config.navigation.spa.enabled
    }

    fn run(&self, site: &Site, out: &mut dyn Emit) -> Result<()> {
        let cfg = &site.config.navigation.spa;
        let path = site.dist(&[SpaConfig::FILE]);
        out.file(&path, &cfg.client())?;
        out.wrote(&path);
        Ok(())
    }
}

/// The router core, shared verbatim with the single-file export: this is the
/// one place link interception, history handling, and container swapping are
/// implemented.
pub(super) const ROUTER: &str = include_str!("js/router.js");

/// The fetch adapter that turns the built site into single-page navigation.
const ADAPTER: &str = include_str!("js/spa.js");

/// The runtime's entry point: what the standalone client auto-calls and what a
/// bundling site imports.
const MOUNT: &str = "mountSpa";

impl SpaConfig {
    /// The generated client's file name at the `dist` root. Single source, so
    /// the emitted file and the documented `<script src>` cannot drift.
    pub const FILE: &'static str = "spa.js";

    /// The standalone client: core, adapter, and an auto-mount, so dropping one
    /// `<script type="module">` in is enough.
    fn client(&self) -> String {
        self.script().mount(MOUNT)
    }

    /// The composable module source served through `baudelaire:spa`, exporting
    /// [`MOUNT`] for the configured runtime and `mountRouter` for a site driving
    /// the core itself. No auto-mount: the importer decides when to mount, and
    /// against which container.
    #[cfg(feature = "js")]
    pub(crate) fn module(&self) -> String {
        self.script().exports(&[MOUNT, "mountRouter"])
    }

    /// The sources both builds share, and the two constants the runtime closes
    /// over: the container selector and the prefetch policy.
    fn script(&self) -> Script<'_> {
        Script::new(&[("ROOT", &self.root), ("PREFETCH", self.prefetch.name())])
            .part(ROUTER)
            .part(ADAPTER)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Prefetch;
    use crate::engine::emit::Recorder;

    fn config() -> Config {
        let mut config = Config::default();
        config.navigation.spa.enabled = true;
        config.navigation.spa.root = "#content".into();
        config.navigation.spa.prefetch = Prefetch::Visible;
        config
    }

    #[test]
    fn client_closes_over_the_configured_selector_and_policy() {
        let config = config();
        let site = Site {
            config: &config,
            pages: &[],
            outputs: &[],
        };

        let mut rec = Recorder::default();
        Spa.run(&site, &mut rec).unwrap();

        let (path, js) = &rec.files[0];
        assert!(path.ends_with("spa.js"), "{path:?}");
        assert!(js.contains(r##"const ROOT = "#content";"##), "{js}");
        assert!(js.contains(r#"const PREFETCH = "visible";"#), "{js}");
        assert!(js.contains("mountSpa();"), "auto-mounts: {js}");
    }

    /// The module is the same source minus the auto-mount, plus exports: a
    /// bundling site mounts it itself, against its own container if it likes.
    #[cfg(feature = "js")]
    #[test]
    fn module_exports_instead_of_auto_mounting() {
        let js = config().navigation.spa.module();
        assert!(js.contains("export { mountSpa, mountRouter };"), "{js}");
        assert!(!js.contains("mountSpa();"), "no auto-mount: {js}");
    }

    /// Nothing is emitted unless the site asked for it.
    #[test]
    fn stays_off_without_a_spa_block() {
        assert!(!Spa.enabled(&Config::default()));
    }
}
