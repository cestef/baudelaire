//! Sass sources: what makes a file one, and what it compiles to.
//!
//! Not a [`Handler`](super::Handler) of its own. What grass emits is a
//! stylesheet, and a stylesheet is already owned end to end by
//! [`Stylesheet`](super::css::Stylesheet): a second handler would minify,
//! downlevel and rewrite `url()`s a second time, or not at all. So this is the
//! step that turns the bytes on disk into the CSS that handler reads, and
//! everything after it is the ordinary path.

use std::path::{Path, PathBuf};

use grass::{Options, OutputStyle};

use crate::config::Config;
use crate::error::{AssetError, Result};

use super::{Ctx, PathExt};

/// The Sass compiler over one file.
pub(super) struct Sass;

impl Sass {
    /// Whether `path` is a Sass source rather than a stylesheet already.
    ///
    /// The extensions are [`Config::SASS`]: grass reads both, and picks between
    /// SCSS and the indented syntax off the path itself, so nothing here has to.
    pub(super) fn claims(path: &Path) -> bool {
        let ext = path.ext().to_ascii_lowercase();
        Config::SASS.contains(&ext.as_str())
    }

    /// The path a compiled source is served from: the same name, as CSS. A
    /// browser reads the output by its MIME type, and `app.scss` holding CSS is
    /// served as something no `<link rel=stylesheet>` will apply.
    pub(super) fn served(rel: &Path) -> PathBuf {
        rel.with_extension("css")
    }

    /// Compile `file` to CSS.
    ///
    /// Always expanded, whatever the site asked for: minification is
    /// lightningcss's, one pass later, and a sheet compressed here would reach
    /// it as one line with every hint of its structure gone.
    ///
    /// `@warn` and `@debug` go to stderr as grass writes them. They are the
    /// sheet author's own messages, so this build has nothing to add to them
    /// and no better place to put them.
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

    /// The claim is the extension and nothing else, case-insensitively: the
    /// asset tree is walked off a filesystem that may not agree with the one it
    /// was authored on.
    #[test]
    fn only_the_two_sass_extensions_are_claimed() {
        assert!(Sass::claims(Path::new("a.scss")));
        assert!(Sass::claims(Path::new("a.SASS")));
        assert!(!Sass::claims(Path::new("a.css")));
        assert!(!Sass::claims(Path::new("a.less")));
        assert!(!Sass::claims(Path::new("scss")));
    }
}
