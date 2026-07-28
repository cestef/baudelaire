use miette::{Diagnostic, NamedSource, SourceSpan};
use thiserror::Error;

#[derive(Error, Debug)]
#[error("{kind}")]
pub struct ConfigError {
    /// The config text, named so the rendered snippet says *which* file it came
    /// from. The name is the conventional one until [`ConfigError::named`]
    /// replaces it with the path actually loaded, which is what makes
    /// `--config prod.kdl` report `prod.kdl`.
    file: NamedSource<String>,
    span: SourceSpan,
    kind: ConfigErrorKind,
}

impl ConfigError {
    /// The conventional config filename, used until the loader supplies the
    /// real one.
    pub const FILE: &'static str = "config.kdl";

    pub fn at(source: &str, kind: ConfigErrorKind, span: SourceSpan) -> Self {
        Self {
            file: NamedSource::new(Self::FILE, source.to_owned()).with_language("KDL"),
            span,
            kind,
        }
    }

    /// Name this diagnostic's source after the file it was really loaded from.
    /// Applied once, where the path is known, so no error has to carry it.
    #[must_use]
    pub fn named(self, path: &std::path::Path) -> Self {
        Self {
            file: NamedSource::new(path.display().to_string(), self.file.inner().clone())
                .with_language("KDL"),
            ..self
        }
    }

    pub fn not_found(path: &str) -> Self {
        Self {
            file: NamedSource::new(path, String::new()),
            span: SourceSpan::new(0.into(), 0),
            kind: ConfigErrorKind::NotFound {
                path: path.to_owned(),
            },
        }
    }

    pub fn unknown_feature(name: &str, valid: &str) -> Self {
        Self {
            file: NamedSource::new(Self::FILE, String::new()),
            span: SourceSpan::new(0.into(), 0),
            kind: ConfigErrorKind::UnknownFeature {
                name: name.to_owned(),
                valid: format!("valid features: {valid}"),
            },
        }
    }

    /// An unrecognized structural key, with a caller-built `help` (nearest match
    /// + the valid keys for the enclosing scope).
    pub fn unknown_key(source: &str, key: &str, help: String, span: SourceSpan) -> Self {
        Self::at(
            source,
            ConfigErrorKind::UnknownKey {
                key: key.to_owned(),
                help,
            },
            span,
        )
    }

    /// An unrecognized enum *value* (a `key=value` where the value is not one of
    /// the allowed variants), distinct from an unknown structural key.
    pub fn unknown_value(source: &str, value: &str, help: String, span: SourceSpan) -> Self {
        Self::at(
            source,
            ConfigErrorKind::UnknownValue {
                value: value.to_owned(),
                help,
            },
            span,
        )
    }

    /// A node missing its required positional argument.
    pub fn missing_arg(source: &str, node: &str, span: SourceSpan) -> Self {
        Self::at(
            source,
            ConfigErrorKind::MissingArg {
                node: node.to_owned(),
            },
            span,
        )
    }

    /// A value of the wrong KDL type (e.g. a string where a boolean was
    /// expected).
    pub fn type_mismatch(
        source: &str,
        expected: &'static str,
        got: &'static str,
        span: SourceSpan,
    ) -> Self {
        Self::at(
            source,
            ConfigErrorKind::TypeMismatch { expected, got },
            span,
        )
    }

    /// An integer literal too large to fit the field's type.
    pub fn integer_overflow(source: &str, value: i128, span: SourceSpan) -> Self {
        Self::at(source, ConfigErrorKind::IntegerOverflow { value }, span)
    }

    /// An integer outside an allowed `[min, max]` range.
    pub fn out_of_range(source: &str, min: i64, max: i64, got: i64, span: SourceSpan) -> Self {
        Self::at(source, ConfigErrorKind::OutOfRange { min, max, got }, span)
    }

    /// A credential-bearing URL on a plaintext scheme.
    pub fn insecure_url(source: &str, got: &str, span: SourceSpan) -> Self {
        Self::at(
            source,
            ConfigErrorKind::InsecureUrl {
                got: got.to_owned(),
            },
            span,
        )
    }

    /// A TCP port outside `0..=65535`.
    pub fn port_range(source: &str, got: i64, span: SourceSpan) -> Self {
        Self::at(source, ConfigErrorKind::PortRange { got }, span)
    }

    /// A count field given a negative value.
    pub fn negative_count(source: &str, field: &str, got: i64, span: SourceSpan) -> Self {
        Self::at(
            source,
            ConfigErrorKind::NegativeCount {
                field: field.to_owned(),
                got,
            },
            span,
        )
    }

    /// A `paginate` size below 1.
    pub fn paginate_too_small(source: &str, got: i64, span: SourceSpan) -> Self {
        Self::at(source, ConfigErrorKind::PaginateTooSmall { got }, span)
    }

    /// A repeated id where each must be unique (a collection, taxonomy, or
    /// profile), naming the kind.
    pub fn duplicate_id(source: &str, noun: &'static str, id: &str, span: SourceSpan) -> Self {
        Self::at(
            source,
            ConfigErrorKind::DuplicateId {
                noun,
                id: id.to_owned(),
            },
            span,
        )
    }

    /// A repeated entry within a list-valued node (e.g. `formats rss rss`).
    pub fn duplicate_entry(source: &str, name: &str, scope: &str, span: SourceSpan) -> Self {
        Self::at(
            source,
            ConfigErrorKind::DuplicateEntry {
                name: name.to_owned(),
                scope: scope.to_owned(),
            },
            span,
        )
    }

    /// A generated file named by something other than a plain name under
    /// `dist`: an absolute path, or one walking out of it.
    pub fn escaping_file(source: &str, path: &str, span: SourceSpan) -> Self {
        Self::at(
            source,
            ConfigErrorKind::EscapingFile {
                path: path.to_owned(),
            },
            span,
        )
    }

    /// An unrecognized image format under `optimize`.
    pub fn unknown_image_format(source: &str, format: &str, span: SourceSpan) -> Self {
        Self::at(
            source,
            ConfigErrorKind::UnknownImageFormat {
                format: format.to_owned(),
            },
            span,
        )
    }

    /// A `-html` entry: HTML export is required and cannot be disabled (other
    /// features may be toggled off with `-name`).
    pub fn feature_removal(source: &str, name: &str, span: SourceSpan) -> Self {
        Self::at(
            source,
            ConfigErrorKind::FeatureRemoval {
                name: name.to_owned(),
            },
            span,
        )
    }

    /// A stray positional argument on a node that takes only `key=value` attrs.
    pub fn unexpected_argument(source: &str, value: &str, node: &str, span: SourceSpan) -> Self {
        Self::at(
            source,
            ConfigErrorKind::UnexpectedArgument {
                value: value.to_owned(),
                node: node.to_owned(),
            },
            span,
        )
    }

    /// A `profiles` block nested inside another profile.
    pub fn nested_profiles(source: &str, span: SourceSpan) -> Self {
        Self::at(source, ConfigErrorKind::NestedProfiles, span)
    }

    /// A `${VAR}` reference to an unset environment variable with no default.
    pub fn env(source: &str, name: &str, span: SourceSpan) -> Self {
        Self::at(
            source,
            ConfigErrorKind::MissingEnv {
                name: name.to_owned(),
            },
            span,
        )
    }

    /// A node that requires a `{ ... }` children block but has none.
    pub fn missing_children(source: &str, span: SourceSpan) -> Self {
        Self::at(source, ConfigErrorKind::MissingChildren, span)
    }

    pub fn parse(source: &str, error: kdl::KdlError) -> Self {
        // kdl 6 is itself on miette 7: `KdlError` is a `Diagnostic` we surface
        // directly (see `diagnostic_source`), no wrapper needed.
        let span = error
            .diagnostics
            .first()
            .map_or_else(|| SourceSpan::new(0.into(), 0), |d| d.span);
        Self {
            file: NamedSource::new(Self::FILE, source.to_owned()).with_language("KDL"),
            span,
            kind: ConfigErrorKind::Parse(Box::new(error)),
        }
    }
}

impl miette::Diagnostic for ConfigError {
    fn code(&self) -> Option<Box<dyn std::fmt::Display + '_>> {
        self.kind.code()
    }

    fn severity(&self) -> Option<miette::Severity> {
        self.kind.severity()
    }

    fn help(&self) -> Option<Box<dyn std::fmt::Display + '_>> {
        self.kind.help()
    }

    fn url(&self) -> Option<Box<dyn std::fmt::Display + '_>> {
        self.kind.url()
    }

    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        // Errors raised outside any config text (missing file, missing profile)
        // carry no source; suppress the snippet instead of pointing at nothing.
        (!self.file.inner().is_empty()).then_some(&self.file as &dyn miette::SourceCode)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
        // For a parse error the nested kdl diagnostics carry their own spans,
        // and a sourceless error has nothing to label.
        if self.file.inner().is_empty() || matches!(self.kind, ConfigErrorKind::Parse(_)) {
            return None;
        }
        Some(Box::new(std::iter::once(
            miette::LabeledSpan::new_with_span(None, self.span),
        )))
    }

    fn related(&self) -> Option<Box<dyn Iterator<Item = &'_ dyn miette::Diagnostic> + '_>> {
        None
    }

    /// Hand the kdl error over transparently: it is itself a miette-7
    /// `Diagnostic` that renders each of its diagnostics (with spans against the
    /// kdl source) as related.
    fn diagnostic_source(&self) -> Option<&dyn miette::Diagnostic> {
        match &self.kind {
            ConfigErrorKind::Parse(e) => Some(e.as_ref() as &dyn miette::Diagnostic),
            _ => None,
        }
    }
}

#[derive(Error, Diagnostic, Debug)]
pub enum ConfigErrorKind {
    #[error("failed to parse the config")]
    #[diagnostic(code(baudelaire::config::parse))]
    Parse(Box<kdl::KdlError>),

    #[error("unknown config key `{key}`")]
    #[diagnostic(code(baudelaire::config::unknown_key))]
    UnknownKey {
        key: String,
        #[help]
        help: String,
    },

    #[error("unknown value `{value}`")]
    #[diagnostic(code(baudelaire::config::unknown_value))]
    UnknownValue {
        value: String,
        #[help]
        help: String,
    },

    #[error("missing argument for `{node}`")]
    #[diagnostic(
        code(baudelaire::config::missing_arg),
        help("add the missing value as a positional argument, e.g. `{node} \"value\"`")
    )]
    MissingArg { node: String },

    #[error("expected {expected}, got {got}")]
    #[diagnostic(code(baudelaire::config::type_mismatch))]
    TypeMismatch {
        expected: &'static str,
        got: &'static str,
    },

    #[error("integer {value} is out of range")]
    #[diagnostic(code(baudelaire::config::integer_overflow))]
    IntegerOverflow { value: i128 },

    #[error("must be {min}-{max}, got {got}")]
    #[diagnostic(code(baudelaire::config::out_of_range))]
    OutOfRange { min: i64, max: i64, got: i64 },

    #[error("port must be 0-65535, got {got}")]
    #[diagnostic(code(baudelaire::config::port_range))]
    PortRange { got: i64 },

    #[error("`{got}` is not https")]
    #[diagnostic(
        code(baudelaire::config::insecure_url),
        help(
            "credentials are sent to this host; use `https://`, or `http://localhost` for a local service"
        )
    )]
    InsecureUrl { got: String },

    #[error("`{field}` must not be negative, got {got}")]
    #[diagnostic(code(baudelaire::config::negative_count))]
    NegativeCount { field: String, got: i64 },

    #[error("paginate must be at least 1, got {got}")]
    #[diagnostic(code(baudelaire::config::paginate_too_small))]
    PaginateTooSmall { got: i64 },

    #[error("duplicate {noun} `{id}`")]
    #[diagnostic(code(baudelaire::config::duplicate_id))]
    DuplicateId { noun: &'static str, id: String },

    #[error("duplicate `{name}` in `{scope}`")]
    #[diagnostic(code(baudelaire::config::duplicate_entry))]
    DuplicateEntry { name: String, scope: String },

    #[error("unknown image format `{format}` (valid: png, jpeg)")]
    #[diagnostic(code(baudelaire::config::unknown_image_format))]
    UnknownImageFormat { format: String },

    #[error("`{path}` would be written outside the output directory")]
    #[diagnostic(
        code(baudelaire::config::escaping_file),
        help("name a file relative to `dist`, with no leading `/` and no `..`")
    )]
    EscapingFile { path: String },

    #[error("feature `{name}` is required and cannot be disabled")]
    #[diagnostic(
        code(baudelaire::config::feature_removal),
        help(
            "HTML export underpins the whole build; other features may be turned off with `-name`"
        )
    )]
    FeatureRemoval { name: String },

    #[error("unexpected argument {value}; `{node}` takes `key=value` attributes")]
    #[diagnostic(code(baudelaire::config::unexpected_argument))]
    UnexpectedArgument { value: String, node: String },

    #[error("`profiles` cannot be nested inside a profile")]
    #[diagnostic(code(baudelaire::config::nested_profiles))]
    NestedProfiles,

    #[error("node missing children block")]
    #[diagnostic(
        code(baudelaire::config::missing_children),
        help("add a `{{ ... }}` block with the node's child entries")
    )]
    MissingChildren,

    #[error("config file not found at `{path}`")]
    #[diagnostic(
        code(baudelaire::config::not_found),
        help("run `baudelaire init` to scaffold a new project, or pass `--config <path>`")
    )]
    NotFound { path: String },

    #[error("unknown typst feature `{name}`")]
    #[diagnostic(code(baudelaire::config::unknown_feature))]
    UnknownFeature {
        name: String,
        #[help]
        valid: String,
    },

    #[error("profile `{name}` not found")]
    #[diagnostic(code(baudelaire::config::missing_profile))]
    MissingProfile {
        name: String,
        #[help]
        help: String,
    },

    #[error("environment variable `{name}` is not set")]
    #[diagnostic(
        code(baudelaire::config::missing_env),
        help("set `{name}` or provide a default with `${{{name}:-default}}`")
    )]
    MissingEnv { name: String },

    /// An invalid permalink template on a collection, surfaced with the config
    /// span. Transparent: message, code, and help come from [`PermalinkError`].
    #[error(transparent)]
    #[diagnostic(transparent)]
    Permalink(#[from] crate::config::PermalinkError),
}
