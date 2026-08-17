//! `baudelaire reference`: every config key, read out of the dispatch tables.

use clap::Args;

use super::{Cx, Run, help};
use crate::error::Result;
use crate::error::cli::{Generated, UnknownKey};
use crate::ui::markup;

#[derive(Args, Debug, Clone)]
#[command(after_help = ReferenceArgs::examples())]
pub struct ReferenceArgs {
    /// A dotted key path to narrow to, e.g. `assets.images`.
    pub key: Option<String>,
}

impl ReferenceArgs {
    /// Appended to `reference --help`.
    fn examples() -> String {
        help::Table::examples(&[
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
            Some(key) => Reference::at(key).ok_or_else(|| {
                let all = Reference::new();
                let paths = all.paths();
                let keys = crate::config::dispatch::Keys::of(&paths);
                UnknownKey {
                    key: key.clone(),
                    help: keys.nearest(key).map_or_else(
                        || markup!("run `{}` for every key", "baudelaire reference"),
                        |near| markup!("did you mean `{}`?", near),
                    ),
                }
            })?,
        };
        Generated::Reference.emit(Terminal(&reference).to_string().as_bytes())?;
        Ok(())
    }
}
