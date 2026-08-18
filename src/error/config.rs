use miette::{Diagnostic, NamedSource, SourceSpan};
use thiserror::Error;

use crate::config::Config;
use crate::ui::{Code, Text, markup};

#[derive(Error, Debug)]
#[error("{kind}")]
pub struct ConfigError {
    /// The config text, named [`Config::FILE`] until [`ConfigError::named`]
    /// replaces it with the path actually loaded.
    file: NamedSource<String>,
    span: SourceSpan,
    kind: ConfigErrorKind,
}

impl ConfigError {
    pub fn at(source: &str, kind: ConfigErrorKind, span: SourceSpan) -> Self {
        Self {
            file: NamedSource::new(Config::FILE, source.to_owned()).with_language("KDL"),
            span,
            kind,
        }
    }

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

    /// A config that did not check out, for a caller that has already said
    /// what was wrong with it in its own words.
    pub fn invalid(path: &str) -> Self {
        Self {
            file: NamedSource::new(path, String::new()),
            span: SourceSpan::new(0.into(), 0),
            kind: ConfigErrorKind::Invalid {
                path: path.to_owned(),
            },
        }
    }

    pub fn unknown_feature(name: &str, valid: &str) -> Self {
        Self {
            file: NamedSource::new(Config::FILE, String::new()),
            span: SourceSpan::new(0.into(), 0),
            kind: ConfigErrorKind::UnknownFeature {
                name: name.to_owned(),
                valid: markup!("valid features: {}", valid),
            },
        }
    }

    /// A `dist` that contains one of the directories the build reads from.
    ///
    /// Sourceless, and so unlabeled: `dist` is settled only after the profile
    /// overlay and `--out`, so the offending value often has no span in the
    /// config text at all.
    pub fn dist_contains_source(dist: &std::path::Path, key: &'static str, path: &str) -> Self {
        Self {
            file: NamedSource::new(Config::FILE, String::new()),
            span: SourceSpan::new(0.into(), 0),
            kind: ConfigErrorKind::DistContainsSource {
                dist: dist.display().to_string(),
                key,
                path: path.to_owned(),
            },
        }
    }

    /// A `typst { fonts { paths } }` entry that is not a directory.
    ///
    /// Sourceless, and so unlabeled: it is checked against the filesystem once
    /// the paths are settled, well after the text that named them was parsed.
    pub fn missing_font_dir(path: &std::path::Path) -> Self {
        Self {
            file: NamedSource::new(Config::FILE, String::new()),
            span: SourceSpan::new(0.into(), 0),
            kind: ConfigErrorKind::MissingFontDir {
                path: path.display().to_string(),
            },
        }
    }

    /// `help` is caller-built: the nearest match, plus the valid keys for the
    /// enclosing scope.
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

    /// An unrecognized enum *value*, distinct from an unknown structural key.
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

    pub fn command_line(source: &str, got: &str, span: SourceSpan) -> Self {
        Self::at(
            source,
            ConfigErrorKind::CommandLine {
                got: got.to_owned(),
            },
            span,
        )
    }

    pub fn not_an_element(source: &str, name: &str, why: &str, span: SourceSpan) -> Self {
        Self::at(
            source,
            ConfigErrorKind::NotAnElement {
                name: name.to_owned(),
                why: why.to_owned(),
            },
            span,
        )
    }

    pub fn missing_arg(source: &str, node: &str, span: SourceSpan) -> Self {
        Self::at(
            source,
            ConfigErrorKind::MissingArg {
                node: node.to_owned(),
            },
            span,
        )
    }

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

    pub fn integer_overflow(source: &str, value: i128, span: SourceSpan) -> Self {
        Self::at(source, ConfigErrorKind::IntegerOverflow { value }, span)
    }

    pub fn out_of_range(source: &str, min: i64, max: i64, got: i64, span: SourceSpan) -> Self {
        Self::at(source, ConfigErrorKind::OutOfRange { min, max, got }, span)
    }

    pub fn insecure_url(source: &str, got: &str, span: SourceSpan) -> Self {
        Self::at(
            source,
            ConfigErrorKind::InsecureUrl {
                got: got.to_owned(),
            },
            span,
        )
    }

    pub fn port_range(source: &str, got: i64, span: SourceSpan) -> Self {
        Self::at(source, ConfigErrorKind::PortRange { got }, span)
    }

    pub fn bad_size(source: &str, got: &str, span: SourceSpan) -> Self {
        Self::at(
            source,
            ConfigErrorKind::BadSize {
                got: got.to_owned(),
            },
            span,
        )
    }

    pub fn bad_duration(source: &str, got: &str, span: SourceSpan) -> Self {
        Self::at(
            source,
            ConfigErrorKind::BadDuration {
                got: got.to_owned(),
            },
            span,
        )
    }

    pub fn bad_version(source: &str, got: &str, span: SourceSpan) -> Self {
        Self::at(
            source,
            ConfigErrorKind::BadVersion {
                got: got.to_owned(),
            },
            span,
        )
    }

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

    pub fn paginate_too_small(source: &str, got: i64, span: SourceSpan) -> Self {
        Self::at(source, ConfigErrorKind::PaginateTooSmall { got }, span)
    }

    /// A repeated id where each must be unique; `noun` names the kind.
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

    /// A `paths { sources }` name that typst cannot bind.
    ///
    /// The names are emitted as generated typst, so one that is not an
    /// identifier fails at the first import, inside a file nobody has opened.
    pub fn not_an_identifier(source: &str, name: &str, span: SourceSpan) -> Self {
        Self::at(
            source,
            ConfigErrorKind::NotAnIdentifier {
                name: name.to_owned(),
            },
            span,
        )
    }

    /// A generated asset's served path that leaves the tree it is written in.
    ///
    /// The pipeline writes the file under `paths { assets }` and the render pass
    /// links a page to the same name, so an absolute path or one climbing out
    /// with `..` leaves the page naming a URL nothing was written to.
    pub fn not_an_asset_path(source: &str, got: &str, span: SourceSpan) -> Self {
        Self::at(
            source,
            ConfigErrorKind::NotAnAssetPath {
                got: got.to_owned(),
            },
            span,
        )
    }

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

    pub fn escaping_file(source: &str, path: &str, span: SourceSpan) -> Self {
        Self::at(
            source,
            ConfigErrorKind::EscapingFile {
                path: path.to_owned(),
            },
            span,
        )
    }

    /// A `-html` entry, the one feature that cannot be disabled.
    pub fn feature_removal(source: &str, name: &str, span: SourceSpan) -> Self {
        Self::at(
            source,
            ConfigErrorKind::FeatureRemoval {
                name: name.to_owned(),
            },
            span,
        )
    }

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

    /// `example` is the line the author meant, built from the very key and
    /// value they wrote.
    pub fn unexpected_attribute(
        source: &str,
        key: &str,
        node: &str,
        example: &str,
        span: SourceSpan,
    ) -> Self {
        Self::at(
            source,
            ConfigErrorKind::UnexpectedAttribute {
                key: key.to_owned(),
                node: node.to_owned(),
                example: example.to_owned(),
            },
            span,
        )
    }

    /// `example` is the section written the way it parses.
    pub fn unexpected_section_argument(
        source: &str,
        value: &str,
        node: &str,
        example: &str,
        span: SourceSpan,
    ) -> Self {
        Self::at(
            source,
            ConfigErrorKind::UnexpectedSectionArgument {
                value: value.to_owned(),
                node: node.to_owned(),
                example: example.to_owned(),
            },
            span,
        )
    }

    pub fn extra_argument(source: &str, value: &str, node: &str, span: SourceSpan) -> Self {
        Self::at(
            source,
            ConfigErrorKind::ExtraArgument {
                value: value.to_owned(),
                node: node.to_owned(),
            },
            span,
        )
    }

    /// `example` is the node written the way it parses, built from its own key
    /// table.
    pub fn unexpected_block(source: &str, node: &str, example: &str, span: SourceSpan) -> Self {
        Self::at(
            source,
            ConfigErrorKind::UnexpectedBlock {
                node: node.to_owned(),
                example: example.to_owned(),
            },
            span,
        )
    }

    pub fn not_absolute_url(source: &str, got: &str, span: SourceSpan) -> Self {
        Self::at(
            source,
            ConfigErrorKind::NotAbsoluteUrl {
                got: got.to_owned(),
            },
            span,
        )
    }

    /// A URL that climbs out of the output directory, wherever a config key
    /// names one a build writes.
    pub fn url_traversal(source: &str, got: &str, span: SourceSpan) -> Self {
        Self::at(
            source,
            ConfigErrorKind::UrlTraversal {
                got: got.to_owned(),
            },
            span,
        )
    }

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

    pub fn missing_children(source: &str, span: SourceSpan) -> Self {
        Self::at(source, ConfigErrorKind::MissingChildren, span)
    }

    pub fn parse(source: &str, error: kdl::KdlError) -> Self {
        let span = error
            .diagnostics
            .first()
            .map_or_else(|| SourceSpan::new(0.into(), 0), |d| d.span);
        Self {
            file: NamedSource::new(Config::FILE, source.to_owned()).with_language("KDL"),
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

    /// `None` for an error raised outside any config text (missing file,
    /// missing profile), which would render a snippet of nothing.
    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        (!self.file.inner().is_empty()).then_some(&self.file as &dyn miette::SourceCode)
    }

    /// `None` for a parse error, whose nested kdl diagnostics carry their own
    /// spans, and for a sourceless one, which has nothing to label.
    fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
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

    /// Hands a kdl parse error over as-is: it is itself a miette-7 `Diagnostic`,
    /// and renders each of its own diagnostics as related.
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

    #[error("unknown config key {}", Code(.key))]
    #[diagnostic(code(baudelaire::config::unknown_key))]
    UnknownKey {
        key: String,
        #[help]
        help: String,
    },

    #[error("unknown value {}", Code(.value))]
    #[diagnostic(code(baudelaire::config::unknown_value))]
    UnknownValue {
        value: String,
        #[help]
        help: String,
    },

    /// Not an [`UnknownValue`](Self::UnknownValue): the value is drawn from no
    /// fixed set, so there is nothing to list and nothing to suggest.
    #[error("{} is a command line, not a program", Code(.got))]
    #[diagnostic(
        code(baudelaire::config::command_line),
        help(
            "nothing runs this through a shell, so give the program and each argument its own word: `editor \"code\" \"--goto\" \"{{file}}:{{line}}:{{column}}\"`"
        )
    )]
    CommandLine { got: String },

    /// The judgement and the reason both come from typst, which owns the set;
    /// `why` is that foreign text, escaped rather than parsed as markup.
    #[error("{} is not an HTML element", Code(.name))]
    #[diagnostic(
        code(baudelaire::config::not_an_element),
        help("name an element your layout emits, like `article`: {}", Text(.why))
    )]
    NotAnElement { name: String, why: String },

    #[error("missing argument for {}", Code(.node))]
    #[diagnostic(
        code(baudelaire::config::missing_arg),
        help(
            "add the missing value as a positional argument, e.g. `{} \"value\"`",
            Text(.node)
        )
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

    #[error("{} is not a byte size", Code(.got))]
    #[diagnostic(
        code(baudelaire::config::bad_size),
        help("a size is a number and an optional unit: `0`, `500`, `50kB`, `1.5 MB`")
    )]
    BadSize { got: String },

    #[error("{} is not a length of time", Code(.got))]
    #[diagnostic(
        code(baudelaire::config::bad_duration),
        help(
            "a duration is a number and an optional unit: `30`, `10s`, `5m`, `7d`; a bare number is seconds"
        )
    )]
    BadDuration { got: String },

    #[error("{} is not a browser version", Code(.got))]
    #[diagnostic(
        code(baudelaire::config::bad_version),
        help(
            "a version is one to three numbers, each 0-255: `15`, `15.4`, `15.4.1`; write it as a string, since `15.10` as a number is `15.1`"
        )
    )]
    BadVersion { got: String },

    #[error("{} is not an absolute URL", Code(.got))]
    #[diagnostic(
        code(baudelaire::config::not_absolute_url),
        help(
            "`url` is the site's own base, scheme and all: {}",
            Code("https://example.com")
        )
    )]
    NotAbsoluteUrl { got: String },

    #[error("{} has a {} segment", Code(.got), Code(".."))]
    #[diagnostic(
        code(baudelaire::config::url_traversal),
        help("a URL this site writes cannot point outside the output directory")
    )]
    UrlTraversal { got: String },

    #[error("{} is not https", Code(.got))]
    #[diagnostic(
        code(baudelaire::config::insecure_url),
        help(
            "credentials are sent to this host; use `https://`, or `http://localhost` for a local service"
        )
    )]
    InsecureUrl { got: String },

    #[error("{} must not be negative, got {got}", Code(.field))]
    #[diagnostic(code(baudelaire::config::negative_count))]
    NegativeCount { field: String, got: i64 },

    #[error("{} is not a name a page can import", Code(.name))]
    #[diagnostic(
        code(baudelaire::config::not_an_identifier),
        help(
            "a declared source is bound under its name in `@baudelaire/sources`, so the name has to be a typst identifier: a letter or `_` first, then letters, digits, `_` or `-`, and not one of typst's own keywords (`none`, `auto`, `let`, `set`, `show`, `context`, `in`, `as`, ..)"
        )
    )]
    NotAnIdentifier { name: String },

    #[error("{} is not a path a generated asset can be served from", Code(.got))]
    #[diagnostic(
        code(baudelaire::config::not_an_asset_path),
        help(
            "it is written under `paths {{ assets }}` and linked from a page by the same name, so it is relative and stays inside the tree: {} or {}",
            Code("utilities.css"),
            Code("css/utilities.css")
        )
    )]
    NotAnAssetPath { got: String },

    #[error("paginate must be at least 1, got {got}")]
    #[diagnostic(code(baudelaire::config::paginate_too_small))]
    PaginateTooSmall { got: i64 },

    /// Reported after the fault itself has been written out, so it carries no
    /// help of its own: it is the exit code with a name.
    #[error("{} did not check out", Code(.path))]
    #[diagnostic(code(baudelaire::config::invalid))]
    Invalid { path: String },

    #[error("duplicate {noun} {}", Code(.id))]
    #[diagnostic(code(baudelaire::config::duplicate_id))]
    DuplicateId { noun: &'static str, id: String },

    /// A snippet language with no command and no parser, refused rather than
    /// ignored: it parses, checks nothing, and leaves a site believing its
    /// fences of that language are looked at.
    #[error("nothing here checks a {} snippet", Code(.lang))]
    #[diagnostic(code(baudelaire::config::no_snippet_checker), help("{help}"))]
    NoSnippetChecker { lang: String, help: String },

    /// A schema field declaring a type its built-in frontmatter key cannot
    /// hold, which nothing would satisfy, so it fails here rather than on every
    /// page of the collection.
    #[error("schema field {} must be {builtin}, not {declared}", Code(.key))]
    #[diagnostic(
        code(baudelaire::config::field_conflict),
        help(
            "{} is a built-in frontmatter key with a fixed type: drop the type to require it as it is",
            Code(.key)
        )
    )]
    FieldConflict {
        key: String,
        declared: String,
        builtin: String,
    },

    /// A taxonomy key that only means something beside another one, refused
    /// rather than ignored: it parses, configures nothing, and leaves a site
    /// waiting for output no code path can produce.
    #[error("the {} taxonomy writes {} without {}", Code(.taxonomy), Code(.key), Code(.needs))]
    #[diagnostic(code(baudelaire::config::taxonomy_requires), help("{help}"))]
    TaxonomyRequires {
        taxonomy: String,
        key: &'static str,
        needs: &'static str,
        help: String,
    },

    /// A registry slot naming a field its entities do not declare, refused at
    /// the block that wrote it: where a renderer reads the slot the answer is
    /// merely "no value", so a green build renders every entity without it.
    #[error(
        "the {} slot of the {} registry names {}, which its entities do not declare",
        Code(.slot),
        Code(.registry),
        Code(.field)
    )]
    #[diagnostic(code(baudelaire::config::entity_slot), help("{help}"))]
    EntitySlot {
        slot: &'static str,
        registry: String,
        field: String,
        help: String,
    },

    #[error("schema field {} is {declared}, so it has no fields", Code(.key))]
    #[diagnostic(
        code(baudelaire::config::field_not_dict),
        help("a block declares what a `dict` holds: write the type as `dict` or `list<dict>`")
    )]
    FieldNotDict { key: String, declared: String },

    #[error("{} is not a type", Code(.ty))]
    #[diagnostic(code(baudelaire::config::type_expr))]
    TypeExpr {
        ty: String,
        #[help]
        help: String,
    },

    #[error("duplicate {} in {}", Code(.name), Code(.scope))]
    #[diagnostic(code(baudelaire::config::duplicate_entry))]
    DuplicateEntry { name: String, scope: String },

    #[error("{} would be written outside the output directory", Code(.path))]
    #[diagnostic(
        code(baudelaire::config::escaping_file),
        help("name a file relative to `dist`, with no leading `/` and no `..`")
    )]
    EscapingFile { path: String },

    #[error("{} is a filename, not a stem", Code(.got))]
    #[diagnostic(
        code(baudelaire::config::index_extension),
        help(
            "`index` names the stem a bundle's own page is keyed by, with no extension: write {}. Left as a filename it matches no page, and the site builds with nothing at `/`",
            Code(.stem)
        )
    )]
    IndexExtension { got: String, stem: String },

    #[error("the output directory contains {}", Code(.key))]
    #[diagnostic(
        code(baudelaire::config::dist_contains_source),
        help(
            "everything under {} that the build did not write is pruned, which here is the whole {} tree; give `dist` a directory of its own",
            Code(.dist),
            Code(.path)
        )
    )]
    DistContainsSource {
        dist: String,
        key: &'static str,
        path: String,
    },

    #[error("font directory {} is not there", Code(.path))]
    #[diagnostic(
        code(baudelaire::config::missing_font_dir),
        help(
            "scanning it would yield no faces and say nothing, leaving the site to typeset in a fallback; create it, or drop it from `typst {{ fonts {{ paths }} }}`"
        )
    )]
    MissingFontDir { path: String },

    #[error("feature {} is required and cannot be disabled", Code(.name))]
    #[diagnostic(
        code(baudelaire::config::feature_removal),
        help(
            "HTML export underpins the whole build; other features may be turned off with `-name`"
        )
    )]
    FeatureRemoval { name: String },

    #[error("unexpected argument {}; {} takes `key=value` attributes", Text(.value), Code(.node))]
    #[diagnostic(code(baudelaire::config::unexpected_argument))]
    UnexpectedArgument { value: String, node: String },

    /// Distinct from [`UnknownKey`](Self::UnknownKey): the key is often a real
    /// one (`content { drafts suffix=".x" }`), written in the one spelling its
    /// scope does not take.
    #[error("unexpected attribute {} on {}", Code(.key), Code(.node))]
    #[diagnostic(
        code(baudelaire::config::unexpected_attribute),
        help("a `{{ }}` block's keys are child nodes: write {}", Code(.example))
    )]
    UnexpectedAttribute {
        key: String,
        node: String,
        example: String,
    },

    /// A value on the line of a section that reads none, such as `paths` or
    /// `serve`.
    #[error("unexpected argument {} for {}", Text(.value), Code(.node))]
    #[diagnostic(
        code(baudelaire::config::unexpected_section_argument),
        help("{} is configured from its block: write {}", Code(.node), Code(.example))
    )]
    UnexpectedSectionArgument {
        value: String,
        node: String,
        example: String,
    },

    /// A positional past the ones a key reads (`serve { port 1 2 }`). The
    /// attribute scope's counterpart is
    /// [`UnexpectedArgument`](Self::UnexpectedArgument), which advises
    /// `key=value` where this advises a key of its own.
    #[error("unexpected argument {} for {}", Text(.value), Code(.node))]
    #[diagnostic(
        code(baudelaire::config::extra_argument),
        help("{} reads a single value; give anything further a key of its own", Code(.node))
    )]
    ExtraArgument { value: String, node: String },

    #[error("{} takes no block; its settings are `key=value` on the line", Code(.node))]
    #[diagnostic(
        code(baudelaire::config::unexpected_block),
        help("write them as attributes: {}", Code(.example))
    )]
    UnexpectedBlock { node: String, example: String },

    #[error("`profiles` cannot be nested inside a profile")]
    #[diagnostic(code(baudelaire::config::nested_profiles))]
    NestedProfiles,

    #[error("node missing children block")]
    #[diagnostic(
        code(baudelaire::config::missing_children),
        help("add a `{{ ... }}` block with the node's child entries")
    )]
    MissingChildren,

    #[error("config file not found at {}", Code(.path))]
    #[diagnostic(
        code(baudelaire::config::not_found),
        help("run `baudelaire init` to scaffold a new project, or pass `--config <path>`")
    )]
    NotFound { path: String },

    #[error("unknown typst feature {}", Code(.name))]
    #[diagnostic(code(baudelaire::config::unknown_feature))]
    UnknownFeature {
        name: String,
        #[help]
        valid: String,
    },

    #[error("profile {} not found", Code(.name))]
    #[diagnostic(code(baudelaire::config::missing_profile))]
    MissingProfile {
        name: String,
        #[help]
        help: String,
    },

    #[error("environment variable {} is not set", Code(.name))]
    #[diagnostic(
        code(baudelaire::config::missing_env),
        help(
            "set `{}` or provide a default with `${{{}:-default}}`",
            Text(.name),
            Text(.name)
        )
    )]
    MissingEnv { name: String },

    #[error(transparent)]
    #[diagnostic(transparent)]
    Permalink(#[from] crate::config::PermalinkError),
}
