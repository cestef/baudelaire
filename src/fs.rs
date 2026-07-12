//! Filesystem facade over `std::fs` that attaches path + operation context to
//! every error (see [`crate::error::FsError`]). Prefer these over `std::fs`
//! wherever a failure should tell the user *which* file and *what* operation.

use std::path::{Path, PathBuf};

use crate::error::fs::FsError;
use crate::error::{Op, Result};

/// Read a file to a string.
pub fn read_to_string(path: impl AsRef<Path>) -> Result<String> {
    let path = path.as_ref();
    std::fs::read_to_string(path).map_err(|e| FsError::new(Op::Read, path, e).into())
}

/// Read a file to bytes.
pub fn read(path: impl AsRef<Path>) -> Result<Vec<u8>> {
    let path = path.as_ref();
    std::fs::read(path).map_err(|e| FsError::new(Op::Read, path, e).into())
}

/// Write bytes to a file, creating it if needed.
pub fn write(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> Result<()> {
    let path = path.as_ref();
    std::fs::write(path, contents).map_err(|e| FsError::new(Op::Write, path, e).into())
}

/// Write bytes to a file, creating any missing parent directories first — the
/// one shared "emit an output file" path.
pub fn write_all(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }
    write(path, contents)
}

/// Resolve a path to its canonical, absolute form.
pub fn canonicalize(path: impl AsRef<Path>) -> Result<PathBuf> {
    let path = path.as_ref();
    std::fs::canonicalize(path).map_err(|e| FsError::new(Op::Canonicalize, path, e).into())
}

/// Canonicalize best-effort: the canonical path when resolvable, else the
/// lexical path unchanged. For sites where an unresolvable path must not fail
/// (dependency capture, display, watch filters).
pub fn canonical(path: impl AsRef<Path>) -> PathBuf {
    let path = path.as_ref();
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Remove a file.
pub fn remove_file(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    std::fs::remove_file(path).map_err(|e| FsError::new(Op::Remove, path, e).into())
}

/// Rename a file, naming both source and destination on failure.
pub fn rename(from: impl AsRef<Path>, to: impl AsRef<Path>) -> Result<()> {
    let (from, to) = (from.as_ref(), to.as_ref());
    std::fs::rename(from, to).map_err(|e| FsError::between(Op::Rename, from, to, e).into())
}

/// Recursively create a directory and all its parents.
pub fn create_dir_all(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    std::fs::create_dir_all(path).map_err(|e| FsError::new(Op::CreateDir, path, e).into())
}

/// List a directory's immediate entries as paths. The open *and* each entry are
/// context-wrapped, so a mid-iteration failure still names the directory.
pub fn read_dir(path: impl AsRef<Path>) -> Result<Vec<PathBuf>> {
    let path = path.as_ref();
    let read = std::fs::read_dir(path).map_err(|e| FsError::new(Op::ReadDir, path, e))?;
    let mut entries = Vec::new();
    for entry in read {
        let entry = entry.map_err(|e| FsError::new(Op::ReadDir, path, e))?;
        entries.push(entry.path());
    }
    Ok(entries)
}

/// Copy a file, reporting the source path on failure.
pub fn copy(from: impl AsRef<Path>, to: impl AsRef<Path>) -> Result<()> {
    let from = from.as_ref();
    std::fs::copy(from, to)
        .map(|_| ())
        .map_err(|e| FsError::new(Op::Copy, from, e).into())
}

/// Recursively remove a directory and its contents.
pub fn remove_dir_all(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    std::fs::remove_dir_all(path).map_err(|e| FsError::new(Op::Remove, path, e).into())
}

#[cfg(test)]
mod tests {
    #[test]
    fn error_names_the_path_and_operation() {
        let err = super::read_to_string("does/not/exist.typ").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("failed to read"), "{msg}");
        assert!(msg.contains("does/not/exist.typ"), "{msg}");
    }
}
