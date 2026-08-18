//! `baudelaire config set`: write one key back, leaving the rest as authored.

use clap::Args;

use super::super::{Cx, Run, help};
use super::{check::CheckArgs, key};
use crate::error::Result;
use crate::error::cli::Generated;
use crate::ui::{Highlighted, markup};

#[derive(Args, Debug, Clone)]
#[command(after_help = SetArgs::examples())]
pub struct SetArgs {
    /// The dotted key path, e.g. `paths.dist`.
    #[arg(value_parser = key::Parser, hide_possible_values = true)]
    pub key: String,
    /// The value, read as the shape the key takes.
    pub value: String,
    /// Print the config that would be written instead of writing it.
    #[arg(long)]
    pub dry_run: bool,
}

impl SetArgs {
    /// Appended to `config set --help`.
    fn examples() -> String {
        help::Table::examples(&[
            ("baudelaire config set paths.dist site", "A path"),
            ("baudelaire config set serve.port 4000", "A number"),
            (
                "baudelaire config set content.collections.blog.sort date",
                "A key under a collection you named",
            ),
        ])
        .to_string()
    }
}

impl Run for SetArgs {
    fn run(&self, cx: &Cx) -> Result<()> {
        use crate::config::edit::Edit;

        let path = &cx.cli.global.config;
        let text = CheckArgs::read(path)?;
        let written = Edit::new(&self.key, &self.value)?.applied(&text)?;
        if self.dry_run {
            Generated::Value.emit(Highlighted::new(&written, "kdl").to_string().as_bytes())?;
            return Ok(());
        }
        crate::fs::write_atomic(path, written.as_bytes())?;
        let said = markup!("`{}` set to `{}`", &self.key, &self.value);
        cx.ui.done(cx.ui.markup(&said));
        Ok(())
    }
}
