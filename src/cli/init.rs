//! `baudelaire init`: scaffold a whole project.

use std::path::PathBuf;

use clap::Args;

use super::{Cx, Run, group, scaffold};
use crate::error::{Result, ScaffoldError};

/// Arguments for `baudelaire init`.
#[derive(Args, Debug, Clone)]
pub struct InitArgs {
    /// Directory to scaffold into (default: current directory).
    pub dir: Option<PathBuf>,

    /// Starter shape. The accepted values and the default both come from the
    /// template registry, so the help cannot name a shape that is not there.
    ///
    /// Left `None` rather than defaulted by clap, because `--theme` scaffolds a
    /// shape of its own and a defaulted value could not be told from a chosen
    /// one: the two together used to write a blog's config over a theme that
    /// declares its own collections.
    #[arg(
        short = 't',
        long,
        help = scaffold::templates::Template::help(),
        help_heading = group::PROJECT,
    )]
    pub template: Option<String>,

    /// Site title (default: prompted, or the directory name).
    #[arg(long, help_heading = group::PROJECT)]
    pub title: Option<String>,

    /// Site author (default: prompted, or your git `user.name`).
    #[arg(long, help_heading = group::PROJECT)]
    pub author: Option<String>,

    /// Canonical base URL (default: prompted).
    #[arg(long, help_heading = group::PROJECT)]
    pub url: Option<String>,

    /// Default language code.
    #[arg(long, default_value = "en", help_heading = group::PROJECT)]
    pub lang: String,

    /// Take templates and assets from a theme package instead of scaffolding
    /// copies of them.
    #[arg(long, value_name = "SPEC", help_heading = group::PROJECT)]
    pub theme: Option<String>,

    /// Switch on optional features. Listed from the same table that resolves
    /// them, so a new feature is documented by existing.
    #[arg(
        long,
        value_name = "FEATURE",
        value_delimiter = ',',
        help = scaffold::templates::Extra::help(),
        help_heading = group::PROJECT,
    )]
    pub with: Vec<String>,

    /// Scaffold the shape without the example pages.
    #[arg(long, help_heading = group::PROJECT)]
    pub no_sample: bool,

    /// Take the default answer to every prompt instead of asking.
    #[arg(short = 'y', long)]
    pub yes: bool,
    // The accepted values are not spelled out: `value_enum` already lists them
    // from [`scaffold::vcs::Vcs`] itself, so a new variant documents itself.
    /// Set up this version-control system without asking.
    #[arg(long, value_enum)]
    pub vcs: Option<scaffold::vcs::Vcs>,
}

impl Run for InitArgs {
    fn run(&self, cx: &Cx) -> Result<()> {
        // The two globals `init` cannot honour: it writes the config every other
        // command reads, so `--config` names the file to write rather than one
        // to read, and there is no profile to select in a project that does not
        // exist yet. Both used to be accepted and ignored.
        if cx.cli.global.profile.is_some() {
            return Err(ScaffoldError::Profile.into());
        }
        cx.ui.banner("init");
        scaffold::init::Init::run(cx.ui, cx.root, self, &cx.cli.global.config)
    }
}
