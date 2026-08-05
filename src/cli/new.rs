//! `baudelaire new`: scaffold one content page.

use std::path::PathBuf;

use clap::Args;

use super::{Cx, Run, Toggle, group, scaffold};
use crate::config::Config;
use crate::error::Result;
use crate::error::warning::Uninferred;

/// Arguments for `baudelaire new`.
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
    // `--edit`, not `--open`: `serve --open` opens a browser, and the two are
    // unrelated. The short form was already `-e`, so the flag's own two names
    // disagreed about what it was called.
    #[arg(short = 'e', long, alias = "open", help_heading = group::CONTENT)]
    pub edit: bool,
}

impl Run for NewArgs {
    fn run(&self, cx: &Cx) -> Result<()> {
        let config = cx.cli.config()?;
        // A project lets `new` read the existing content: next order in an
        // ordered collection, and permalink collisions. Both are conveniences,
        // so a content tree that cannot be opened costs the inference and warns
        // rather than refusing to write the file. The theme is part of that: a
        // page of an existing site may import one of its modules.
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
    /// The file `baudelaire new` should create: a relative path lands under
    /// the content directory (unless it already starts with it, so an explicit
    /// `content/posts/foo.typ` is not double-prefixed), and `.typ` is appended
    /// when the name does not already carry it.
    /// Whether the scaffolded page is a draft. Drafting is the default: a page
    /// being written is not one being published, and `--no-draft` says so.
    pub(crate) fn is_draft(&self) -> bool {
        Toggle::of(self.draft, self.no_draft).or(true)
    }

    pub(crate) fn target(&self, config: &Config) -> PathBuf {
        let mut path = if self.path.is_absolute() || self.path.starts_with(&config.paths.content) {
            self.path.clone()
        } else {
            config.paths.content.join(&self.path)
        };
        // A bundle is a directory holding an `index.typ` (the collection's
        // configured index name), so images and data can sit beside the page.
        if self.bundle {
            path.set_extension("");
            return path.join(format!("{}.typ", config.bundle_index()));
        }
        if path.extension().is_none_or(|e| e != "typ") {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("untitled");
            path.set_file_name(format!("{name}.typ"));
        }
        path
    }
}
