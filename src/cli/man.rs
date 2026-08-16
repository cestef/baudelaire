//! `baudelaire man`: a roff manual page on stdout.

use clap::Args;

use super::{Cli, Cx, Run};
use crate::error::Result;
use crate::error::cli::Generated;

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
