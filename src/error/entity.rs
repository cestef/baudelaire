//! Failures from the entity registries: a reference nobody answers, a name that
//! reaches two entities, a roster that does not carry what it says it does.

use std::fmt;

use miette::{Diagnostic, LabeledSpan, NamedSource, Severity, SourceCode, SourceSpan};
use thiserror::Error;

use crate::config::dispatch::Keys;
use crate::content::entities::{Entity, Snippet};
use crate::content::frontmatter::check::Fault;
use crate::ui::{Code, Text, markup};

#[derive(Error, Diagnostic, Debug)]
pub enum EntityError {
    /// A term whose registry holds no entity of that name.
    #[error(transparent)]
    #[diagnostic(transparent)]
    Unresolved(Unresolved),

    #[error(
        "{} reaches both {} and {} in the {} registry",
        Code(.alias),
        Code(.first),
        Code(.second),
        Code(.registry)
    )]
    #[diagnostic(
        code(baudelaire::entity::alias),
        help(
            "an alias is a second name for one entity: drop it from one of them, or give the two of them one id"
        )
    )]
    Alias {
        registry: String,
        alias: String,
        first: String,
        second: String,
        #[source_code]
        src: Option<NamedSource<String>>,
        #[label("declared here")]
        span: Option<SourceSpan>,
    },

    #[error(
        "{} in the {} registry has no {}, which its entities must carry",
        Code(.id),
        Code(.registry),
        Code(.key)
    )]
    #[diagnostic(code(baudelaire::entity::missing_field), help("{help}"))]
    MissingField {
        registry: String,
        id: String,
        key: String,
        help: String,
        #[source_code]
        src: Option<NamedSource<String>>,
        // `want` is one of this crate's own literals, never authored text: a
        // label is rendered raw rather than as markup.
        #[label("should carry {want}")]
        span: Option<SourceSpan>,
        want: String,
    },

    #[error(
        "{} of {} in the {} registry must be {}, but is {}",
        Code(.key),
        Code(.id),
        Code(.registry),
        Text(.want),
        Text(.got)
    )]
    #[diagnostic(code(baudelaire::entity::field), help("{help}"))]
    Field {
        registry: String,
        id: String,
        key: String,
        got: String,
        help: String,
        #[source_code]
        src: Option<NamedSource<String>>,
        #[label("must be {want}")]
        span: Option<SourceSpan>,
        want: String,
    },

    #[error(
        "the {} taxonomy resolves its terms in the {} registry, which nothing declares",
        Code(.taxonomy),
        Code(.registry)
    )]
    #[diagnostic(code(baudelaire::entity::no_registry), help("{help}"))]
    NoRegistry {
        taxonomy: String,
        registry: String,
        help: String,
    },
}

impl EntityError {
    /// One name reaching two entities, underlined at the second of them.
    pub fn alias(
        registry: &str,
        alias: &str,
        first: &str,
        second: &str,
        snippet: Option<Snippet>,
    ) -> Self {
        let (src, span) = Snippet::parts(snippet);
        Self::Alias {
            registry: registry.to_owned(),
            alias: alias.to_owned(),
            first: first.to_owned(),
            second: second.to_owned(),
            src,
            span,
        }
    }

    /// An entity that fails the fields its registry declares.
    pub(crate) fn field(
        registry: &str,
        entity: &Entity,
        fault: &Fault,
        snippet: Option<Snippet>,
    ) -> Self {
        let (src, span) = Snippet::parts(snippet);
        let (registry, id, key) = (registry.to_owned(), entity.id().to_owned(), fault.key());
        let (source, at) = (entity.from().source(), entity.from().at());
        match fault {
            Fault::Missing { want, .. } => Self::MissingField {
                help: markup!(
                    "add `{} {}` where `{}` declares it, in {}, or declare the field `optional=#true`",
                    &key,
                    &want.example(),
                    source,
                    &at
                ),
                want: want.article(),
                registry,
                id,
                key,
                src,
                span,
            },
            Fault::Mismatch { want, got, .. } => Self::Field {
                help: markup!("declared by `{}`, in {}", source, &at),
                want: want.article(),
                got: got.clone(),
                registry,
                id,
                key,
                src,
                span,
            },
        }
    }

    pub fn no_registry(taxonomy: &str, registry: &str, known: &[&str]) -> Self {
        Self::NoRegistry {
            help: if known.is_empty() {
                markup!(
                    "declare it: `content {{ entities {{ {} {{ .. }} }} }}`",
                    registry
                )
            } else {
                Keys::of(known).help(registry, "registries")
            },
            taxonomy: taxonomy.to_owned(),
            registry: registry.to_owned(),
        }
    }
}

/// A term that names nothing in the registry its taxonomy resolves against.
///
/// Its own type rather than a plain variant: the registry decides whether this
/// stops the build (`unknown "error"`) or is merely reported (`unknown
/// "warn"`), and a severity that varies is a field rather than an attribute.
#[derive(Debug)]
pub struct Unresolved {
    registry: String,
    taxonomy: String,
    term: String,
    help: String,
    /// The page that wrote the term.
    src: Option<NamedSource<String>>,
    /// Where in it, when the frontmatter could be read into.
    span: Option<SourceSpan>,
    severity: Severity,
}

impl Unresolved {
    /// A term nothing in its registry answers to, underlined where the page
    /// wrote it, with the near id if there is one.
    pub fn new(
        registry: &str,
        taxonomy: &str,
        term: &str,
        known: &[&str],
        snippet: Option<Snippet>,
    ) -> Self {
        let (src, span) = Snippet::parts(snippet);
        Self {
            help: if known.is_empty() {
                markup!(
                    "the `{}` registry holds no entities yet: declare this one, or set `unknown \"synthesize\"` to take the term as written",
                    registry
                )
            } else {
                Keys::of(known).help(term, "ids")
            },
            registry: registry.to_owned(),
            taxonomy: taxonomy.to_owned(),
            term: term.to_owned(),
            src,
            span,
            severity: Severity::Error,
        }
    }

    /// The same diagnostic, reported rather than raised: what
    /// `unknown "warn"` asks for.
    pub fn lenient(mut self) -> Self {
        self.severity = Severity::Warning;
        self
    }
}

impl fmt::Display for Unresolved {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} in {} names nothing in the {} registry",
            Code(&self.term),
            Code(&self.taxonomy),
            Code(&self.registry)
        )
    }
}

impl std::error::Error for Unresolved {}

impl Diagnostic for Unresolved {
    fn code(&self) -> Option<Box<dyn fmt::Display + '_>> {
        Some(Box::new("baudelaire::entity::unresolved"))
    }

    fn severity(&self) -> Option<Severity> {
        Some(self.severity)
    }

    fn help(&self) -> Option<Box<dyn fmt::Display + '_>> {
        Some(Box::new(&self.help))
    }

    fn source_code(&self) -> Option<&dyn SourceCode> {
        self.span
            .and_then(|_| self.src.as_ref().map(|src| src as &dyn SourceCode))
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        let span = self.span?;
        Some(Box::new(std::iter::once(LabeledSpan::new_with_span(
            Some("no entity answers to this".to_owned()),
            span,
        ))))
    }
}

impl From<Unresolved> for EntityError {
    fn from(e: Unresolved) -> Self {
        Self::Unresolved(e)
    }
}

impl From<EntityError> for crate::error::BaudelaireErrorKind {
    fn from(e: EntityError) -> Self {
        Self::Entity(Box::new(e))
    }
}
