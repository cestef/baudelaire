//! Dev server: serve the built site, watch for changes, rebuild and live-reload.

#[macro_use]
mod endpoint;

mod dev;
mod live;
mod open;
mod route;
mod watch;

use std::path::PathBuf;

use clap::Args;

use crate::cli::{BuildOverrides, Cx, Overrides, Root, Run, Toggle, group};
use crate::config::Config;
use crate::error::Result;
use crate::ui::Ui;

use dev::Dev;

/// Run the dev server: build once, serve `dist`, and (unless `--no-watch`)
/// watch for changes to rebuild and live-reload browsers.
///
/// CLI flags (`--port`, `--bind`, `--open`, `--no-watch`) are already folded
/// into `config.serve` by [`ServeArgs::apply`].
pub(crate) fn run<'a>(
    ui: &'a Ui,
    config: Config,
    root: &'a Root,
    config_path: PathBuf,
    reload: impl FnMut() -> Result<Config> + 'a,
) -> Result<()> {
    Dev {
        config,
        ui,
        root,
        config_path,
        reload: Box::new(reload),
        tracked: Vec::new(),
        rewatch: false,
    }
    .run()
}

/// Arguments for `baudelaire serve`.
#[derive(Args, Debug, Clone)]
pub struct ServeArgs {
    #[command(flatten)]
    pub overrides: BuildOverrides,

    /// Port to listen on (overrides config).
    #[arg(long, help_heading = group::SERVER)]
    pub port: Option<u16>,

    /// Address to bind (overrides config).
    #[arg(long, help_heading = group::SERVER)]
    pub bind: Option<String>,

    /// Open a browser on start (default; `--no-open` suppresses it).
    #[arg(long, overrides_with = "no_open", help_heading = group::SERVER)]
    pub open: bool,
    #[arg(long, overrides_with = "open", hide = true)]
    pub no_open: bool,

    /// Watch for changes and rebuild (default; `--no-watch` serves statically).
    #[arg(long, overrides_with = "no_watch", help_heading = group::SERVER)]
    pub watch: bool,
    #[arg(long, overrides_with = "watch", hide = true)]
    pub no_watch: bool,

    /// Stamp each element with the source it came from, so alt-clicking the
    /// preview opens that line in `serve { editor }` (`html { spans }`).
    #[arg(long, overrides_with = "no_spans", help_heading = group::SERVER)]
    pub spans: bool,
    #[arg(long, overrides_with = "spans", hide = true)]
    pub no_spans: bool,
}

impl Run for ServeArgs {
    fn run(&self, cx: &Cx) -> Result<()> {
        use owo_colors::OwoColorize;

        let mut config = cx.cli.config()?;
        self.apply(&mut config);
        // Prefix the site name with the active profile, if one was requested.
        match cx.cli.global.profile.as_deref() {
            Some(profile) => cx.ui.banner(format_args!(
                "{} · {}",
                profile.cyan().bold(),
                config.label()
            )),
            None => cx.ui.banner(format_args!("{}", config.label())),
        }
        // Re-reads config.kdl with the same profile + overrides, so the dev
        // server picks up config edits live.
        let reload = || -> Result<Config> {
            let mut config = cx.cli.config()?;
            self.apply(&mut config);
            Ok(config)
        };
        crate::cli::serve::run(cx.ui, config, cx.root, cx.cli.global.config.clone(), reload)
    }
}

impl ServeArgs {
    fn apply(&self, config: &mut Config) {
        self.overrides.apply(config);
        if let Some(port) = self.port {
            config.serve.port = port;
        }
        if let Some(bind) = &self.bind {
            config.serve.bind.clone_from(bind);
        }
        Toggle::of(self.open, self.no_open).apply(&mut config.serve.open);
        Toggle::of(self.watch, self.no_watch).apply(&mut config.serve.watch);
        // Into `html`, not `serve`: the stamps are markup, and the cache keys on
        // the config that shapes markup. A preview-only switch that the
        // fingerprint could not see would leave a later `build` reusing these
        // pages with their attributes still on.
        Toggle::of(self.spans, self.no_spans).apply(&mut config.html.spans);
    }
}
