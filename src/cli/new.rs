//! `baudelaire new`: scaffold one content page.

use std::path::PathBuf;

use clap::Args;

use super::{Cx, Run, Toggle, group, scaffold};
use crate::config::Config;
use crate::error::Result;
use crate::error::warning::Uninferred;

#[derive(Args, Debug, Clone)]
pub struct NewArgs {
    /// Path for the new content file (e.g. `posts/my-post` or
    /// `content/posts/my-post.typ`). A bare name lands under the content dir.
    pub path: PathBuf,

    /// Page title (default: derived from the filename).
    #[arg(long, help_heading = group::CONTENT)]
    pub title: Option<String>,

    /// Publication date `YYYY-MM-DD` (default: today, for dated collections).
    #[arg(long, help_heading = group::CONTENT)]
    pub date: Option<String>,

    /// Mark the page a draft (default; `--no-draft` publishes it immediately).
    #[arg(long, overrides_with = "no_draft", help_heading = group::CONTENT)]
    pub draft: bool,
    #[arg(long, overrides_with = "draft", hide = true)]
    pub no_draft: bool,

    /// Create a page bundle (`<name>/index.typ`) for colocated assets.
    #[arg(short = 'b', long, help_heading = group::CONTENT)]
    pub bundle: bool,

    /// Open the new file in `$EDITOR` after creating it.
    #[arg(short = 'e', long, alias = "open", help_heading = group::CONTENT)]
    pub edit: bool,
}

impl Run for NewArgs {
    fn run(&self, cx: &Cx) -> Result<()> {
        let config = cx.cli.config()?;
        let opened = crate::theme::Theme::of(&config).and_then(|theme| {
            crate::world::Project::new(&config, crate::engine::Mode::Build, theme.as_ref())
        });
        let project = match opened {
            Ok(project) => Some(project),
            Err(error) => {
                cx.ui.warn(Uninferred {
                    errors: vec![error],
                });
                None
            }
        };
        scaffold::draft::Draft::plan(self, &config, project.as_ref(), cx.ui)?.create(cx.ui)
    }
}

impl NewArgs {
    /// Whether the scaffolded page is a draft; drafting is the default.
    pub(crate) fn is_draft(&self) -> bool {
        Toggle::of(self.draft, self.no_draft).or(true)
    }

    /// The file to create: a relative path lands under the content directory,
    /// and `.typ` is appended when the name does not already carry it. Only a
    /// `.typ` suffix is dropped for a bundle, since `set_extension("")` would
    /// cut `posts/v1.2` down to `v1`.
    pub(crate) fn target(&self, config: &Config) -> PathBuf {
        let mut path = if self.path.is_absolute() || self.path.starts_with(&config.paths.content) {
            self.path.clone()
        } else {
            config.paths.content.join(&self.path)
        };
        if self.bundle {
            if Config::has_ext(&path, Config::TYPST) {
                path.set_extension("");
            }
            return path.join(format!("{}.{}", config.bundle_index(), Config::TYPST));
        }
        if !Config::has_ext(&path, Config::TYPST) {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("untitled");
            path.set_file_name(format!("{name}.{}", Config::TYPST));
        }
        path
    }
}
