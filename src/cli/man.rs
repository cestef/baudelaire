//! `baudelaire man`: a roff manual page on stdout.

use clap::Args;

use super::{Cli, Cx, Run};
use crate::error::Result;
use crate::error::cli::Generated;

/// Arguments for `baudelaire man`. Empty, and kept as a struct rather than
/// collapsed into a unit variant so the command matches every other one: a
/// subcommand is a `Command` variant, an args struct, one [`Run`] impl and one
/// delegating arm, with nowhere for a special case to grow.
#[derive(Args, Debug, Clone)]
pub struct ManArgs {}
impl Run for ManArgs {
    fn run(&self, _cx: &Cx) -> Result<()> {
        use clap::CommandFactory;

        let mut page = Vec::new();
        Generated::Man.check(clap_mangen::Man::new(Cli::command()).render(&mut page))?;
        Generated::Man.emit(&page)?;
        Ok(())
    }
}
