//! Theme resolution failures.

use miette::Diagnostic;
use thiserror::Error;

use crate::ui::{Bytes, Code, Text};

/// A configured theme that could not be resolved. Fatal: the site's templates
/// and assets come from it, so continuing would build a stripped site.
#[derive(Debug, Error, Diagnostic)]
pub enum ThemeError {
    #[error("{} is not a package spec: {}", Code(.spec), Text(.why))]
    #[diagnostic(
        code(baudelaire::theme::spec),
        help("a theme is named like any Typst package: `@preview/name:1.0.0`")
    )]
    Spec { spec: String, why: String },

    #[error("theme {} could not be obtained", Code(.spec))]
    #[diagnostic(
        code(baudelaire::theme::unavailable),
        help("check the name and version, and that the machine can reach the package registry")
    )]
    Unavailable {
        spec: String,
        #[source]
        source: typst::diag::PackageError,
    },

    #[error("theme directory {} is outside the project", Code(.path))]
    #[diagnostic(
        code(baudelaire::theme::outside),
        help(
            "a Typst import cannot leave the project root: move the theme inside it, \
             or publish it and name it as `@namespace/name:version`"
        )
    )]
    Outside { path: String },

    #[error("theme directory {} does not exist", Code(.path))]
    #[diagnostic(
        code(baudelaire::theme::missing),
        help(
            "create it, `baudelaire theme add <name>` to write one of the shipped themes \
             there, or name a published theme as `@namespace/name:version`"
        )
    )]
    Missing { path: String },

    #[error("no theme named {} ships with baudelaire", Code(.name))]
    #[diagnostic(code(baudelaire::theme::unknown), help("{help}"))]
    Unknown { name: String, help: String },

    #[error("no theme baudelaire installed is at {}", Code(.path))]
    #[diagnostic(
        code(baudelaire::theme::uninstalled),
        help(
            "`baudelaire theme add <name>` writes one there; a theme you wrote or copied \
             in yourself is yours to move and delete"
        )
    )]
    Uninstalled { path: String },

    #[error("the theme record at {} could not be written", Code(.path))]
    #[diagnostic(
        code(baudelaire::theme::lock),
        help("it records which files are baudelaire's, so `theme update` can keep yours")
    )]
    Lock {
        path: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("{} holds no files, so it is not a theme", Code(.path))]
    #[diagnostic(
        code(baudelaire::theme::empty),
        help(
            "a theme is `templates {{ }}`, `assets {{ }}`, `static {{ }}` and a `theme.kdl`, \
             in a directory of its own"
        )
    )]
    Empty { path: String },

    #[error("{} does not name a directory a theme could be called after", Code(.path))]
    #[diagnostic(
        code(baudelaire::theme::unnamed),
        help("a copy is known by its directory's name, so name the directory, or pass `--dir`")
    )]
    Unnamed { path: String },

    /// Boxed because the two halves of one fetch fail with different types: the
    /// client's error for the request, an `io::Error` for the body.
    #[error("{} could not be fetched", Code(.url))]
    #[diagnostic(
        code(baudelaire::theme::fetch),
        help("this is the one theme command that needs the network; nothing else here does")
    )]
    Fetch {
        url: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Boxed for the same reason: the tar branch fails with an `io::Error`, the
    /// zip branch with the zip reader's own error.
    #[error("the archive at {} could not be read", Code(.url))]
    #[diagnostic(
        code(baudelaire::theme::unpack),
        help("`.tar.gz`, `.tgz` and `.zip` are what this reads; a forge's source download is one")
    )]
    Unpack {
        url: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Refused rather than truncated: a reader that stops at the ceiling
    /// installs a theme missing whatever fell off the end.
    #[error("the archive at {} is larger than the {} a theme may weigh", Code(.url), Code(Bytes(*limit)))]
    #[diagnostic(
        code(baudelaire::theme::oversize),
        help(
            "nothing was written; a theme is templates, assets and a `theme.kdl`, and the \
             shipped ones weigh about 60 KiB each"
        )
    )]
    Oversize { url: String, limit: u64 },

    /// The compressed ceiling says nothing about what the bytes expand to: gzip
    /// reaches roughly a thousand to one, so an archive well inside
    /// [`ThemeError::Oversize`] can unpack into gigabytes.
    #[error("the archive at {} unpacks to more than the {} a theme may weigh", Code(.url), Code(Bytes(*limit)))]
    #[diagnostic(
        code(baudelaire::theme::unpacked),
        help(
            "nothing was written; the archive is small but its contents are not, which is a \
             compression bomb rather than a theme"
        )
    )]
    Unpacked { url: String, limit: u64 },

    /// The other half of the same ceiling: a great many tiny entries cost
    /// nothing to compress and are just as effective.
    #[error("the archive at {} holds more than the {limit} files a theme may have", Code(.url))]
    #[diagnostic(
        code(baudelaire::theme::crowded),
        help("nothing was written; the shipped themes hold a few dozen files each")
    )]
    Crowded { url: String, limit: usize },

    #[error("the archive at {} names a file outside itself: {}", Code(.url), Code(.entry))]
    #[diagnostic(
        code(baudelaire::theme::escapes),
        help(
            "nothing was written; an archive that climbs out of its own directory is not unpacked"
        )
    )]
    Escapes { url: String, entry: String },

    /// A theme's `theme.kdl` naming a section that is not a theme's to name.
    ///
    /// Refused rather than ignored, so a theme author learns their block does
    /// nothing instead of shipping one that silently never applies.
    #[error("theme defaults at {} set {}, which is the site's", Code(.path), Code(.section))]
    #[diagnostic(
        code(baudelaire::theme::governs),
        help(
            "a theme supplies templates, assets and their defaults; where a site's files live, \
             what its build runs, where it publishes, and what a visitor's browser is told to \
             trust are the site's own. Drop the block from the theme, and write it in the \
             project's own config"
        )
    )]
    Governs { path: String, section: String },

    #[error("nothing knows how to fetch {}", Code(.spec))]
    #[diagnostic(
        code(baudelaire::theme::unsupported),
        help(
            "a theme comes from a name `baudelaire theme list` prints, or from a spelling \
             this build recognises; a copy whose record names a source this baudelaire \
             does not have was written by a newer one"
        )
    )]
    Unsupported { spec: String },
}

impl ThemeError {
    /// `help` is a nearest-name suggestion, and arrives already marked up.
    pub fn unknown(name: &str, help: String) -> Self {
        Self::Unknown {
            name: name.to_owned(),
            help,
        }
    }

    pub fn unsupported(spec: String) -> Self {
        Self::Unsupported { spec }
    }

    pub fn fetch(url: &str, source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Fetch {
            url: url.to_owned(),
            source: Box::new(source),
        }
    }

    pub fn unpack(url: &str, source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Unpack {
            url: url.to_owned(),
            source: Box::new(source),
        }
    }

    pub fn oversize(url: &str, limit: u64) -> Self {
        Self::Oversize {
            url: url.to_owned(),
            limit,
        }
    }

    pub fn unpacked(url: &str, limit: u64) -> Self {
        Self::Unpacked {
            url: url.to_owned(),
            limit,
        }
    }

    pub fn crowded(url: &str, limit: usize) -> Self {
        Self::Crowded {
            url: url.to_owned(),
            limit,
        }
    }

    /// `section` is one of the config's own top-level key names, never the
    /// theme author's text.
    pub fn governs(path: impl std::fmt::Display, section: &str) -> Self {
        Self::Governs {
            path: path.to_string(),
            section: section.to_owned(),
        }
    }

    pub fn escapes(url: &str, entry: &str) -> Self {
        Self::Escapes {
            url: url.to_owned(),
            entry: entry.to_owned(),
        }
    }

    pub fn empty(path: &str) -> Self {
        Self::Empty {
            path: path.to_owned(),
        }
    }

    pub fn unnamed(path: &str) -> Self {
        Self::Unnamed {
            path: path.to_owned(),
        }
    }

    pub fn not_installed(path: &str) -> Self {
        Self::Uninstalled {
            path: path.to_owned(),
        }
    }

    pub fn lock(path: impl std::fmt::Display, source: serde_json::Error) -> Self {
        Self::Lock {
            path: path.to_string(),
            source,
        }
    }

    /// A `why: String` rather than a `#[source]`: `PackageSpec::from_str` fails
    /// with an `EcoString`, so there is no error to keep.
    pub fn spec(spec: &str, why: impl std::fmt::Display) -> Self {
        Self::Spec {
            spec: spec.to_owned(),
            why: why.to_string(),
        }
    }

    pub fn unavailable(spec: &str, source: typst::diag::PackageError) -> Self {
        Self::Unavailable {
            spec: spec.to_owned(),
            source,
        }
    }

    pub fn outside(path: &str) -> Self {
        Self::Outside {
            path: path.to_owned(),
        }
    }

    pub fn missing(path: &str) -> Self {
        Self::Missing {
            path: path.to_owned(),
        }
    }
}
