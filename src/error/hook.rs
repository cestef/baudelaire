//! Errors from external command hooks.

use std::fmt;
use std::process::ExitStatus;

use miette::Diagnostic;
use thiserror::Error;

use crate::ui::Code;

/// When in the build lifecycle a hook runs, named for error messages.
#[derive(Debug, Clone, Copy)]
pub enum Phase {
    Before,
    After,
}

impl fmt::Display for Phase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Before => "before",
            Self::After => "after",
        })
    }
}

#[derive(Debug, Error, Diagnostic)]
pub enum HookError {
    #[error("could not run {phase}-build hook {}", Code(.command))]
    #[diagnostic(code(baudelaire::hook::spawn), help("is the command on your PATH?"))]
    Spawn {
        phase: Phase,
        command: String,
        #[source]
        source: std::io::Error,
    },

    #[error("{phase}-build hook {} failed ({status})", Code(.command))]
    #[diagnostic(code(baudelaire::hook::status), help("see the command's output above"))]
    Failed {
        phase: Phase,
        command: String,
        status: ExitStatus,
    },
}

impl HookError {
    pub fn spawn(phase: Phase, command: impl Into<String>, source: std::io::Error) -> Self {
        Self::Spawn {
            phase,
            command: command.into(),
            source,
        }
    }

    pub fn failed(phase: Phase, command: impl Into<String>, status: ExitStatus) -> Self {
        Self::Failed {
            phase,
            command: command.into(),
            status,
        }
    }
}
