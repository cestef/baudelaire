//! `baudelaire reference`: every config key, read out of the dispatch tables.

use clap::Args;

use super::{Cx, Run, help};
use crate::error::Result;
use crate::error::cli::{Generated, UnknownKey};
use crate::ui::markup;

/// Arguments for `baudelaire reference`.
#[derive(Args, Debug, Clone)]
#[command(after_help = ReferenceArgs::examples())]
pub struct ReferenceArgs {
    /// A dotted key path to narrow to, e.g. `assets.images`.
    pub key: Option<String>,
}

impl ReferenceArgs {
    /// Appended to `reference --help`. The narrowing argument is the part worth
    /// showing: the bare command prints a hundred and fifty keys.
    fn examples() -> String {
        help::Examples::new(&[
            ("baudelaire reference", "Every key"),
            ("baudelaire reference assets", "Just the asset pipeline"),
            (
                "baudelaire reference deploy.s3",
                "Just the S3 deploy backend",
            ),
        ])
        .to_string()
    }
}

impl Run for ReferenceArgs {
    fn run(&self, _cx: &Cx) -> Result<()> {
        use crate::config::reference::{Reference, Terminal};

        let reference = match &self.key {
            None => Reference::new(),
            // The nearest real path, and *not* the whole list: there are over a
            // hundred and fifty of them, and a help that prints them all is a
            // wall rather than an answer. The command with no argument is the
            // list, so the help says so instead.
            Some(key) => Reference::at(key).ok_or_else(|| {
                let all = Reference::new();
                let paths = all.paths();
                let keys = crate::config::dispatch::Keys::of(&paths);
                UnknownKey {
                    key: key.clone(),
                    help: match keys.nearest(key) {
                        Some(near) => markup!("did you mean `{}`?", near),
                        None => markup!("run `{}` for every key", "baudelaire reference"),
                    },
                }
            })?,
        };
        Generated::Reference.emit(Terminal(&reference).to_string().as_bytes())?;
        Ok(())
    }
}
