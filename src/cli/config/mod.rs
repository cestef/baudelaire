//! `baudelaire config`: what can be said about a config without building it.
//!
//! ```text
//! check    parse it, resolve its theme, apply its profiles
//! explain  what one key is, and where this config sets it
//! get      what it is set to, one value per line
//! set      write one key back, the rest of the file as authored
//! show     the config, or one block of it, highlighted
//! ```

pub mod check;
pub mod explain;
pub mod get;
mod key;
pub mod set;
pub mod show;

use clap::{Args, Subcommand};

use super::{Cx, Run};
use crate::error::Result;

pub use check::CheckArgs;
pub use explain::ExplainArgs;
pub use get::GetArgs;
pub use set::SetArgs;
pub use show::ShowArgs;

#[derive(Args, Debug, Clone)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub verb: Verb,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Verb {
    /// Validate a config, its theme and its profiles, without building.
    #[command(visible_alias = "c")]
    Check(CheckArgs),
    /// Say what one key is and where this config sets it.
    #[command(visible_alias = "e")]
    Explain(ExplainArgs),
    /// Print what this config sets one key to.
    #[command(visible_alias = "g")]
    Get(GetArgs),
    /// Set one key in the config file, leaving the rest of it as written.
    #[command(visible_alias = "s")]
    Set(SetArgs),
    /// Print the config, or one block of it, highlighted.
    Show(ShowArgs),
}

impl ConfigArgs {
    /// Whether this verb's own answer *is* the stdout payload, so a `--json`
    /// summary would corrupt what a script is reading.
    pub(super) fn owns_stdout(&self) -> bool {
        matches!(self.verb, Verb::Get(_) | Verb::Show(_))
    }
}

impl Run for ConfigArgs {
    fn run(&self, cx: &Cx) -> Result<()> {
        match &self.verb {
            Verb::Check(args) => args.run(cx),
            Verb::Explain(args) => args.run(cx),
            Verb::Get(args) => args.run(cx),
            Verb::Set(args) => args.run(cx),
            Verb::Show(args) => args.run(cx),
        }
    }
}
