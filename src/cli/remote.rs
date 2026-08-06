//! The terminal-facing half of the two publishing commands.
//!
//! `announce` and `deploy` differ only in where they send the site; the flags,
//! the build, the flag-to-[`Options`] mapping and the [`Tty`] interaction are
//! identical, so they live here once.

use clap::Args;

use crate::cli::prompt::Tty;
use crate::config::Config;
use crate::engine::{Engine, Mode};
use crate::error::{BaudelaireErrorKind, Result};
use crate::remote::Options;
use crate::ui::Ui;

/// The flags every publishing command shares, flattened into each of them.
///
/// One declaration, so the two commands cannot drift about what `--secret`
/// takes or which short form `--yes` has. They used to declare all three
/// separately and then re-expose them through a three-method trait with one
/// impl each: two copies of the flags, plus a third copy of their names in a
/// trait that abstracted nothing.
#[derive(Args, Debug, Clone, Default)]
pub struct PublishArgs {
    /// Secret for the destination (an S3 secret access key, an SSH password or
    /// key passphrase, an atproto app password); `-` reads it from stdin.
    /// Prefer stdin, the backend's environment variable, or the interactive
    /// prompt: a literal flag can leak into shell history.
    // `--password` stays as an alias: it is what the atproto side calls an app
    // password, and it was `announce`'s own spelling before the two commands
    // shared one flag.
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
    /// hand the built site over.
    ///
    /// The check comes first, and that ordering is the point. Both commands
    /// used to build the whole site and only then discover that the config
    /// named nowhere to send it, so the one error a fresh project always hits
    /// arrived a full build late.
    pub(super) fn send(&self, ui: &Ui, config: &Config, destination: &Destination) -> Result<()> {
        destination.check(config)?;
        Engine::new(config.clone(), Mode::Build)?.build(ui)?;
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
///
/// A table, one per command, declared beside that command's args.
pub(super) struct Destination {
    /// Whether the config names a destination for this command.
    ///
    /// The question is deliberately "is one *named*", not "is one usable": a
    /// destination the build cannot reach (an SSH target in a binary built
    /// without the `ssh` feature) still has a precise diagnostic of its own,
    /// and this must not answer for it.
    ///
    /// Each implementation destructures its config section, so a new backend
    /// fails to compile until it is answered for here. Without that, adding one
    /// would leave this saying "nothing configured" about a config that names
    /// it.
    pub(super) named: fn(&Config) -> bool,
    /// The diagnostic for a config that names nowhere to send the site.
    pub(super) unconfigured: fn() -> BaudelaireErrorKind,
    /// The module that publishes, once there is something to publish to.
    pub(super) publish: fn(&Config, &Options, &Ui) -> Result<()>,
}

impl Destination {
    /// Refuse a run that has nowhere to go, before anything is built.
    fn check(&self, config: &Config) -> Result<()> {
        match (self.named)(config) {
            true => Ok(()),
            false => Err((self.unconfigured)()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A config naming no destination is refused, and one naming a destination
    /// is not. The check is what stands between a fresh project and a full
    /// build it was never going to publish.
    #[test]
    fn a_config_with_nowhere_to_send_the_site_is_refused_up_front() {
        let destination = crate::cli::DeployArgs::DESTINATION;
        let mut config = Config::default();
        assert!(destination.check(&config).is_err());
        config.deploy.ssh = Some(crate::config::SshConfig::default());
        assert!(destination.check(&config).is_ok());
    }
}
