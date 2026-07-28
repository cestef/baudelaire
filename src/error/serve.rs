//! Errors that stop the dev server coming up: the socket it could not take, and
//! the watches it could not establish. Once it is serving, a failure is a
//! warning instead (see [`super::warning`]), because the server stays up.

use miette::Diagnostic;
use thiserror::Error;

/// A failure while starting `baudelaire serve`.
#[derive(Error, Diagnostic, Debug)]
pub enum ServeError {
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

impl ServeError {
    pub fn bind(addr: &str, error: Box<dyn std::error::Error + Send + Sync>) -> Self {
        Self::Bind {
            addr: addr.to_owned(),
            error,
        }
    }

    pub fn watcher_init(error: notify::Error) -> Self {
        Self::WatcherInit(error)
    }

    pub fn watch(dir: &std::path::Path, source: notify::Error) -> Self {
        Self::Watch {
            dir: dir.display().to_string(),
            source,
        }
    }
}

impl From<ServeError> for crate::error::BaudelaireErrorKind {
    fn from(e: ServeError) -> Self {
        Self::Serve(Box::new(e))
    }
}
