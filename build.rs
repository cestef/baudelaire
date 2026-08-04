//! Build provenance for `--version`: where this binary came from, which the
//! compiler is the only thing in a position to know.
//!
//! Everything probed here degrades to `unknown` rather than failing the build:
//! a published crate has no `.git`, a container may have no `git`, and neither
//! is a reason not to compile. What `--version` then reports is honest about
//! not knowing rather than wrong.
//!
//! Deliberately no build timestamp. A commit plus a toolchain plus a target
//! identifies a build exactly, and a clock reading identifies nothing you can
//! act on while costing the binary its bit-for-bit reproducibility.

use std::process::Command;

/// One probe of the build environment.
struct Probe;

impl Probe {
    /// Run `git` with `args`, or `None` when git is absent, fails, or this is
    /// not a repository (a crates.io build, a vendored source tree).
    fn git(args: &[&str]) -> Option<String> {
        let out = Command::new("git").args(args).output().ok()?;
        match out.status.success() {
            true => Some(String::from_utf8(out.stdout).ok()?.trim().to_owned()),
            false => None,
        }
    }

    /// The commit, marked `-dirty` when the tree it was built from had
    /// uncommitted changes. A bug report naming a clean commit can be
    /// reproduced from that commit; one naming a dirty tree cannot, and the
    /// suffix is what says so.
    fn commit() -> String {
        let Some(hash) = Self::git(&["rev-parse", "--short=12", "HEAD"]) else {
            return "unknown".to_owned();
        };
        match Self::git(&["status", "--porcelain"]).is_some_and(|s| !s.is_empty()) {
            true => format!("{hash}-dirty"),
            false => hash,
        }
    }

    /// The rustc that compiled this, without its `rustc ` prefix. Cargo sets
    /// `RUSTC` to the toolchain actually in use, which is what to ask.
    fn rustc() -> String {
        let bin = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_owned());
        let Some(out) = Command::new(bin)
            .arg("--version")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
        else {
            return "unknown".to_owned();
        };
        out.trim().trim_start_matches("rustc ").to_owned()
    }

    /// A cargo-set variable, or `unknown`.
    fn env(key: &str) -> String {
        std::env::var(key).unwrap_or_else(|_| "unknown".to_owned())
    }
}

fn main() {
    for (key, value) in [
        ("BAUDELAIRE_GIT_COMMIT", Probe::commit()),
        ("BAUDELAIRE_RUSTC", Probe::rustc()),
        ("BAUDELAIRE_TARGET", Probe::env("TARGET")),
        ("BAUDELAIRE_PROFILE", Probe::env("PROFILE")),
    ] {
        println!("cargo:rustc-env={key}={value}");
    }

    // Re-probe when the checked-out commit moves. Deliberately *not*
    // `rerun-if-changed` on the whole tree: a dirty working copy would then
    // rebuild the crate on every edit to report a `-dirty` suffix it already
    // reported, and this is provenance, not correctness.
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=build.rs");
    // The starter shapes and the bundled themes are embedded with `include_dir!`,
    // which records no dependency on the directories it walks. Without these, a
    // scaffold file *added* to one of those trees is not in the next binary, and
    // nothing says so: `init` writes the shape as it was two builds ago.
    println!("cargo:rerun-if-changed=src/cli/scaffold");
    println!("cargo:rerun-if-changed=themes");
}
