//! Theme resolution failures.

use miette::Diagnostic;
use thiserror::Error;

use crate::ui::{Bytes, Code, Text};

/// A configured theme that could not be resolved. Fatal: the site's templates
/// and assets are expected to come from it, so continuing would build a
/// stripped version of the site rather than the one that was asked for.
#[derive(Debug, Error, Diagnostic)]
pub enum ThemeError {
    #[error("{} is not a package spec: {}", Code(.spec), Text(.why))]
    #[diagnostic(
        code(baudelaire::theme::spec),
        help("a theme is named like any Typst package: `@preview/name:1.0.0`")
    )]
    Spec { spec: String, why: String },

    /// The package store could not produce the package. Its own
    /// [`PackageError`](typst::diag::PackageError) is kept as the source: it
    /// tells "no such package" from "no such version" from "the download
    /// failed", which a flattened message reduces to one sentence.
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

    /// The record beside a theme's files could not be serialized. The
    /// serializer's own error is kept as the source, so the offending field is
    /// still named.
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

    /// The download itself failed. Boxed because the two halves of one fetch
    /// fail with different types (the client's error for the request, an
    /// `io::Error` for the body), and both are the answer.
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

    /// The archive could not be decompressed or walked. Boxed for the same
    /// reason: the tar branch fails with an `io::Error`, the zip branch with
    /// the zip reader's own error.
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

    /// The download ran past the ceiling a theme may weigh.
    ///
    /// Refused rather than truncated. The reader used to stop at the limit and
    /// hand back what it had, so an oversize (or endless) URL installed as a
    /// theme missing whatever fell off the end: a green `theme add` and a
    /// directory of half-written files.
    #[error("the archive at {} is larger than the {} a theme may weigh", Code(.url), Code(Bytes(*limit)))]
    #[diagnostic(
        code(baudelaire::theme::oversize),
        help(
            "nothing was written; a theme is templates, assets and a `theme.kdl`, and the \
             shipped ones weigh about 60 KiB each"
        )
    )]
    Oversize { url: String, limit: u64 },

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
    /// A theme supplies templates, assets, and defaults for what the *pages*
    /// are. Where the project's files live, what its build runs and where it
    /// publishes are the site's own, and a theme is fetched: a package theme is
    /// downloaded at build time, so its `theme.kdl` need never appear in the
    /// site's repository at all. A hook inherited from one would be a command
    /// run through a shell that the site never wrote and cannot read.
    ///
    /// Refused rather than ignored, so a theme author learns their block does
    /// nothing instead of shipping one that silently never applies.
    #[error("theme defaults at {} set {}, which is the site's", Code(.path), Code(.section))]
    #[diagnostic(
        code(baudelaire::theme::governs),
        help(
            "a theme supplies templates, assets and their defaults; where a site's files live, \
             what its build runs and where it publishes are the site's own. Drop the block from \
             the theme, and write it in the project's own config"
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
    /// `help` is the nearest-name suggestion built from the bundled table, and
    /// arrives already marked up.
    pub fn unknown(name: &str, help: String) -> Self {
        Self::Unknown {
            name: name.to_owned(),
            help,
        }
    }

    /// A spec no source claims, or an origin no source owns: the same answer
    /// either way, because both mean this binary cannot go and get it.
    pub fn unsupported(spec: String) -> Self {
        Self::Unsupported { spec }
    }

    /// The transport's own error, kept whole: what it says about a redirect or
    /// a TLS failure is the answer, and it is a different type for the request
    /// and for the body.
    pub fn fetch(url: &str, source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Fetch {
            url: url.to_owned(),
            source: Box::new(source),
        }
    }

    /// The decompressor's own error, for the same reason.
    pub fn unpack(url: &str, source: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Unpack {
            url: url.to_owned(),
            source: Box::new(source),
        }
    }

    /// The download stopped at the ceiling, so what arrived is a prefix of a
    /// theme and not a theme.
    pub fn oversize(url: &str, limit: u64) -> Self {
        Self::Oversize {
            url: url.to_owned(),
            limit,
        }
    }

    /// `section` is one of the config's own top-level key names, so it is this
    /// binary's text rather than the theme author's; `path` is the theme's
    /// `theme.kdl`, as the run spells it.
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

    /// The serializer's own error, kept whole.
    pub fn lock(path: impl std::fmt::Display, source: serde_json::Error) -> Self {
        Self::Lock {
            path: path.to_string(),
            source,
        }
    }

    /// The one place a `why: String` is right rather than a `#[source]`:
    /// `PackageSpec::from_str` fails with an `EcoString`, so there is no error
    /// to keep. The text *is* the parser's whole answer.
    pub fn spec(spec: &str, why: impl std::fmt::Display) -> Self {
        Self::Spec {
            spec: spec.to_owned(),
            why: why.to_string(),
        }
    }

    /// The package store's own error, kept whole: it is a real error type, and
    /// it tells a missing package from a missing version from a failed
    /// download.
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
