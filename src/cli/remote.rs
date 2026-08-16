//! The terminal-facing half of the two publishing commands: `announce` and
//! `deploy` differ only in where they send the site.

use clap::Args;

use crate::cli::prompt::Tty;
use crate::config::Config;
use crate::engine::{Engine, Mode};
use crate::error::{BaudelaireErrorKind, Result};
use crate::remote::Options;
use crate::ui::Ui;

/// The flags every publishing command shares, flattened into each of them.
#[derive(Args, Debug, Clone, Default)]
pub struct PublishArgs {
    /// Secret for the destination (an S3 secret access key, an SSH password or
    /// key passphrase, an atproto app password); `-` reads it from stdin.
    /// Prefer stdin, the backend's environment variable, or the interactive
    /// prompt: a literal flag can leak into shell history.
    #[arg(long, alias = "password")]
    pub secret: Option<String>,
    /// Skip the confirmation prompt.
    #[arg(short = 'y', long)]
    pub yes: bool,
    /// Report what would change without writing to any destination.
    #[arg(long)]
    pub dry_run: bool,
}

impl PublishArgs {
    /// Publish the site to `destination`: check that there is one, build, then
    /// hand the built site over. The check comes first, so a config naming
    /// nowhere fails before the build rather than after it.
    pub(super) fn send(&self, ui: &Ui, config: &Config, destination: &Destination) -> Result<()> {
        destination.check(config)?;
        let stats = Engine::new(config.clone(), Mode::Build)?.build(ui)?;
        ui.built(stats.pages, stats.cached);
        let tty = Tty;
        let options = Options {
            dry_run: self.dry_run,
            yes: self.yes,
            secret: self.secret.clone(),
            interaction: &tty,
        };
        (destination.publish)(config, &options, ui)
    }
}

/// Where a publishing command sends the site, as much of it as the CLI needs to
/// know: whether the config names a destination at all, what to say when it
/// names none, and the module that does the sending.
pub(super) struct Destination {
    /// Whether the config *names* a destination for this command, not whether
    /// one is usable: an unreachable destination has a diagnostic of its own.
    pub(super) named: fn(&Config) -> bool,
    /// The diagnostic for a config that names nowhere to send the site.
    pub(super) unconfigured: fn() -> BaudelaireErrorKind,
    /// The module that publishes, once there is something to publish to.
    pub(super) publish: fn(&Config, &Options, &Ui) -> Result<()>,
}

impl Destination {
    /// Refuse a run that has nowhere to go, before anything is built.
    fn check(&self, config: &Config) -> Result<()> {
        if (self.named)(config) {
            Ok(())
        } else {
            Err((self.unconfigured)())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_config_with_nowhere_to_send_the_site_is_refused_up_front() {
        let destination = crate::cli::DeployArgs::DESTINATION;
        let mut config = Config::default();
        assert!(destination.check(&config).is_err());
        config.deploy.ssh = Some(crate::config::SshConfig::default());
        assert!(destination.check(&config).is_ok());
    }
}
