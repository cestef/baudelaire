use miette::Diagnostic;
use thiserror::Error;

#[derive(Error, Debug)]
#[error("{kind}")]
pub struct ServeError {
    kind: ServeErrorKind,
}

impl ServeError {
    pub fn bind(addr: &str, error: Box<dyn std::error::Error + Send + Sync>) -> Self {
        Self {
            kind: ServeErrorKind::Bind {
                addr: addr.to_owned(),
                error,
            },
        }
    }

    pub fn watcher_init(error: notify::Error) -> Self {
        Self {
            kind: ServeErrorKind::WatcherInit(error),
        }
    }

    pub fn watch(dir: &std::path::Path, source: notify::Error) -> Self {
        Self {
            kind: ServeErrorKind::Watch {
                dir: dir.display().to_string(),
                source,
            },
        }
    }
}

impl miette::Diagnostic for ServeError {
    fn code(&self) -> Option<Box<dyn std::fmt::Display + '_>> {
        self.kind.code()
    }

    fn help(&self) -> Option<Box<dyn std::fmt::Display + '_>> {
        self.kind.help()
    }
}

#[derive(Error, Diagnostic, Debug)]
pub enum ServeErrorKind {
    #[error("failed to bind `{addr}`")]
    #[diagnostic(
        code(baudelaire::serve::bind),
        help("is another process using this port? try --port <n>")
    )]
    Bind {
        addr: String,
        #[source]
        error: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("failed to start file watcher")]
    #[diagnostic(code(baudelaire::serve::watcher_init))]
    WatcherInit(#[source] notify::Error),

    #[error("failed to watch `{dir}`")]
    #[diagnostic(code(baudelaire::serve::watch))]
    Watch {
        dir: String,
        #[source]
        source: notify::Error,
    },
}

impl From<ServeError> for crate::error::BaudelaireErrorKind {
    fn from(e: ServeError) -> Self {
        Self::Serve(Box::new(e))
    }
}
