//! Path-aware filesystem errors.
//!
//! `std::io::Error` alone says *what* went wrong but not *which file* or *which
//! operation*. [`FsError`] carries the operation and path so a failure reads
//! `failed to read `content/post.typ`: No such file or directory` with a hint
//! keyed off the OS error kind. Produced by the [`crate::fs`] facade.

use std::fmt;
use std::io;
use std::path::PathBuf;

use miette::Diagnostic;
use thiserror::Error;

/// A filesystem operation, named for error messages.
#[derive(Debug, Clone, Copy)]
pub enum Op {
    Read,
    Write,
    CreateDir,
    ReadDir,
    Copy,
    Remove,
    Canonicalize,
    Enter,
}

impl fmt::Display for Op {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::CreateDir => "create directory",
            Self::ReadDir => "read directory",
            Self::Copy => "copy",
            Self::Remove => "remove",
            Self::Canonicalize => "resolve",
            Self::Enter => "enter directory",
        })
    }
}

/// A filesystem operation that failed on a specific path.
#[derive(Error, Debug)]
#[error("failed to {op} `{}`", path.display())]
pub struct FsError {
    op: Op,
    path: PathBuf,
    #[source]
    source: io::Error,
}

impl FsError {
    pub fn new(op: Op, path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self {
            op,
            path: path.into(),
            source,
        }
    }
}

impl Diagnostic for FsError {
    fn code(&self) -> Option<Box<dyn fmt::Display + '_>> {
        Some(Box::new(format!("baudelaire::fs::{}", self.op).replace(' ', "_")))
    }

    /// A remedy keyed off the OS error kind — far more actionable than the raw
    /// `io::Error` message.
    fn help(&self) -> Option<Box<dyn fmt::Display + '_>> {
        let hint = match self.source.kind() {
            io::ErrorKind::NotFound => {
                "the path does not exist — check the spelling and that it was created"
            }
            io::ErrorKind::PermissionDenied => {
                "permission denied — check the file's ownership and mode"
            }
            io::ErrorKind::AlreadyExists => "the path already exists — remove it or pick another",
            _ => return None,
        };
        Some(Box::new(hint))
    }
}
