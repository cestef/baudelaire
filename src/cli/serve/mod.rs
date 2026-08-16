//! Dev server: serve the built site, watch for changes, rebuild and live-reload.

#[macro_use]
mod endpoint;

mod dev;
mod live;
mod open;
mod route;
mod watch;

use clap::Args;

use crate::cli::{BuildOverrides, Cx, Overrides, Run, Toggle, group};
use crate::config::Config;
use crate::error::Result;

use dev::Dev;

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
        match cx.cli.global.profile.as_deref() {
            Some(profile) => cx.ui.banner(format_args!(
                "{} · {}",
                profile.cyan().bold(),
                config.label()
            )),
            None => cx.ui.banner(format_args!("{}", config.label())),
        }
        let reload = || -> Result<Config> {
            let mut config = cx.cli.config()?;
            self.apply(&mut config);
            Ok(config)
        };
        Dev::start(cx.ui, config, cx.root, cx.cli.global.config.clone(), reload)
    }
}

impl ServeArgs {
    /// `--spans` lands in `html`, not `serve`: the stamps are markup, and the
    /// cache fingerprint keys on the config that shapes markup.
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
        Toggle::of(self.spans, self.no_spans).apply(&mut config.html.spans);
    }
}
