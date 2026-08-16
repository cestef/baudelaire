//! Client-side navigation over the ordinary multi-file output: a runtime that
//! intercepts internal link clicks, fetches the target, and swaps one
//! container.

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

/// The router core, shared verbatim with the single-file export.
pub(super) const ROUTER: &str = include_str!("js/router.js");

/// The fetch adapter that turns the built site into single-page navigation.
const ADAPTER: &str = include_str!("js/spa.js");

/// The runtime's entry point.
const MOUNT: &str = "mountSpa";

impl SpaConfig {
    /// The generated client's file name at the `dist` root.
    pub const FILE: &'static str = "spa.js";

    /// The standalone client: core, adapter, and an auto-mount, so dropping one
    /// `<script type="module">` in is enough.
    fn client(&self) -> String {
        self.script().mount(MOUNT)
    }

    /// The composable module source served through `baudelaire:spa`, exporting
    /// [`MOUNT`] for the configured runtime and `mountRouter` for a site
    /// driving the core itself, with no auto-mount.
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
            entities: crate::content::Registries::none(),
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

    #[cfg(feature = "js")]
    #[test]
    fn module_exports_instead_of_auto_mounting() {
        let js = config().navigation.spa.module();
        assert!(js.contains("export { mountSpa, mountRouter };"), "{js}");
        assert!(!js.contains("mountSpa();"), "no auto-mount: {js}");
    }

    #[test]
    fn stays_off_without_a_spa_block() {
        assert!(!Spa.enabled(&Config::default()));
    }
}
