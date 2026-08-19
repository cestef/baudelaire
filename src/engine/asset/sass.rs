//! Sass sources: what makes a file one, and what it compiles to. Not a
//! [`Handler`](super::Handler) of its own, since what grass emits is a
//! stylesheet, owned end to end by [`Stylesheet`](super::css::Stylesheet).

use std::path::{Path, PathBuf};

use grass::{Options, OutputStyle};

use crate::config::Config;
use crate::error::{AssetError, Result};

use super::{Ctx, PathExt};

/// The Sass compiler over one file.
pub(super) struct Sass;

impl Sass {
    /// Whether `path` is a Sass source rather than a stylesheet already; grass
    /// picks between SCSS and the indented syntax off the path itself.
    pub(super) fn claims(path: &Path) -> bool {
        let ext = path.ext().to_ascii_lowercase();
        Config::SASS.contains(&ext.as_str())
    }

    /// The path a compiled source is served from: the same name, as CSS, which
    /// is the MIME type a `<link rel=stylesheet>` needs.
    pub(super) fn served(rel: &Path) -> PathBuf {
        rel.with_extension(Config::CSS)
    }

    /// Compile `file` to CSS, always expanded: minification is lightningcss's,
    /// one pass later.
    pub(super) fn compile(file: &Path, ctx: &Ctx) -> Result<String> {
        let options = Options::default()
            .style(OutputStyle::Expanded)
            .load_paths(&ctx.roots);
        Ok(grass::from_path(file, &options).map_err(|e| AssetError::sass(file.display(), e))?)
    }
}

#[cfg(test)]
mod tests {
    use super::Sass;
    use std::path::{Path, PathBuf};

    #[test]
    fn a_sass_source_is_served_as_css() {
        assert_eq!(
            Sass::served(Path::new("css/app.scss")),
            PathBuf::from("css/app.css")
        );
        assert_eq!(
            Sass::served(Path::new("css/app.sass")),
            PathBuf::from("css/app.css")
        );
    }

    #[test]
    fn only_the_two_sass_extensions_are_claimed() {
        assert!(Sass::claims(Path::new("a.scss")));
        assert!(Sass::claims(Path::new("a.SASS")));
        assert!(!Sass::claims(Path::new("a.css")));
        assert!(!Sass::claims(Path::new("a.less")));
        assert!(!Sass::claims(Path::new("scss")));
    }
}
