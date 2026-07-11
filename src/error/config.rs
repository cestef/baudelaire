use miette::{Diagnostic, SourceSpan};
use thiserror::Error;

#[derive(Error, Debug)]
#[error("{kind}")]
pub struct ConfigError {
    text: String,
    span: SourceSpan,
    kind: ConfigErrorKind,
}

impl ConfigError {
    pub fn at(source: &str, kind: ConfigErrorKind, span: SourceSpan) -> Self {
        Self {
            text: source.to_owned(),
            span,
            kind,
        }
    }

    pub fn not_found(path: &str) -> Self {
        Self {
            text: String::new(),
            span: SourceSpan::new(0.into(), 0),
            kind: ConfigErrorKind::NotFound {
                path: path.to_owned(),
            },
        }
    }

    pub fn unknown_feature(name: &str, valid: &str) -> Self {
        Self {
            text: String::new(),
            span: SourceSpan::new(0.into(), 0),
            kind: ConfigErrorKind::UnknownFeature {
                name: name.to_owned(),
                valid: format!("valid features: {valid}"),
            },
        }
    }

    /// An unrecognized key, with a caller-built `help` (nearest match + the
    /// valid keys for the enclosing scope).
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

    /// A value of the wrong type or an unrecognized variant.
    pub fn bad_value(source: &str, detail: impl Into<String>, span: SourceSpan) -> Self {
        Self::at(
            source,
            ConfigErrorKind::BadValue {
                detail: detail.into(),
            },
            span,
        )
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
            text: source.to_owned(),
            span,
            kind: ConfigErrorKind::Parse(error),
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
        (!self.text.is_empty()).then_some(&self.text as &dyn miette::SourceCode)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
        // For a parse error the nested kdl diagnostics carry their own spans,
        // and a sourceless error has nothing to label.
        if self.text.is_empty() || matches!(self.kind, ConfigErrorKind::Parse(_)) {
            return None;
        }
        Some(Box::new(std::iter::once(
            miette::LabeledSpan::new_with_span(None, self.span),
        )))
    }

    fn related(&self) -> Option<Box<dyn Iterator<Item = &'_ dyn miette::Diagnostic> + '_>> {
        None
    }

    /// Hand the kdl error over transparently — it is itself a miette-7
    /// `Diagnostic` that renders each of its diagnostics (with spans against the
    /// kdl source) as related.
    fn diagnostic_source(&self) -> Option<&dyn miette::Diagnostic> {
        match &self.kind {
            ConfigErrorKind::Parse(e) => Some(e as &dyn miette::Diagnostic),
            _ => None,
        }
    }
}

#[derive(Error, Diagnostic, Debug)]
pub enum ConfigErrorKind {
    #[error("failed to parse config.kdl")]
    #[diagnostic(code(baudelaire::config::parse))]
    Parse(kdl::KdlError),

    #[error("unknown key `{key}` in config.kdl")]
    #[diagnostic(code(baudelaire::config::unknown_key))]
    UnknownKey {
        key: String,
        #[help]
        help: String,
    },

    #[error("missing argument for `{node}`")]
    #[diagnostic(
        code(baudelaire::config::missing_arg),
        help("add the missing value as a positional argument, e.g. `{node} \"value\"`")
    )]
    MissingArg { node: String },

    #[error("{detail}")]
    #[diagnostic(
        code(baudelaire::config::bad_value),
        help("check the expected type / allowed values for this field")
    )]
    BadValue { detail: String },

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

    #[error("profile `{name}` not found in config.kdl")]
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
    Permalink(#[from] crate::content::PermalinkError),
}

