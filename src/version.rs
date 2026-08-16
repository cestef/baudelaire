//! What this binary *is*: its version, where it was built from, and which
//! optional capabilities were compiled into it.

use std::fmt::Write as _;
use std::sync::LazyLock;

use owo_colors::{OwoColorize, Stream::Stdout};

pub struct Version;

impl Version {
    /// The released version, as `Cargo.toml` states it.
    pub const SEMVER: &'static str = env!("CARGO_PKG_VERSION");
    /// The commit this was built from, `-dirty` when the tree had uncommitted
    /// changes, or `unknown` outside a repository.
    pub const COMMIT: &'static str = env!("BAUDELAIRE_GIT_COMMIT");
    pub const RUSTC: &'static str = env!("BAUDELAIRE_RUSTC");
    pub const TARGET: &'static str = env!("BAUDELAIRE_TARGET");
    pub const PROFILE: &'static str = env!("BAUDELAIRE_PROFILE");

    /// Every optional capability and whether this binary has it, and the single
    /// source of what a build can do.
    ///
    /// A feature that gates no capability of its own is left out, so that
    /// turning it on does not report a published build as `custom`:
    /// `vendored-openssl` decides only how OpenSSL is linked, and `sidecars` is
    /// implied by the artifact features that already name it.
    pub const FEATURES: &'static [(&'static str, bool)] = &[
        ("announce", cfg!(feature = "announce")),
        ("cards", cfg!(feature = "cards")),
        ("css", cfg!(feature = "css")),
        ("embedded-fonts", cfg!(feature = "embedded-fonts")),
        ("epub", cfg!(feature = "epub")),
        ("images", cfg!(feature = "images")),
        ("js", cfg!(feature = "js")),
        ("markdown", cfg!(feature = "markdown")),
        ("pdf", cfg!(feature = "pdf")),
        ("sass", cfg!(feature = "sass")),
        ("ssh", cfg!(feature = "ssh")),
        ("tailwind", cfg!(feature = "tailwind")),
        ("themes", cfg!(feature = "themes")),
    ];

    /// The released flavor this binary matches, named the way `install.sh`
    /// spells it, or `custom` when no published tarball matches it.
    pub fn flavor() -> &'static str {
        let on = Self::FEATURES.iter().filter(|(_, on)| *on).count();
        match on {
            0 => "slim",
            n if n == Self::FEATURES.len() => "full",
            _ => "custom",
        }
    }

    /// The optional capabilities this binary has, and the ones it lacks, each
    /// as one line, spelling an empty side `none` rather than leaving it blank.
    fn split() -> (String, String) {
        let pick = |want: bool| -> String {
            let names: Vec<&str> = Self::FEATURES
                .iter()
                .filter(|(_, on)| *on == want)
                .map(|(name, _)| *name)
                .collect();
            if names.is_empty() {
                "none".to_owned()
            } else {
                names.join(" ")
            }
        };
        (pick(true), pick(false))
    }

    /// The full report, for `--version`, rendered once because clap wants a
    /// `'static` string.
    pub fn long() -> &'static str {
        static LONG: LazyLock<String> = LazyLock::new(Version::render);
        LONG.as_str()
    }

    /// Build the full report, without a program name: clap prints one in front
    /// of whatever this returns.
    ///
    /// Labels are padded before they are styled, since a width applied to a
    /// styled label counts its escape codes and misaligns the column.
    fn render() -> String {
        let (have, missing) = Self::split();
        let mut out = format!(
            "{}\n",
            Self::SEMVER.if_supports_color(Stdout, |t| t.bold().to_string()),
        );
        let mut rows = vec![
            ("commit", Self::COMMIT.to_owned()),
            ("rustc", format!("{} ({})", Self::RUSTC, Self::PROFILE)),
            ("target", Self::TARGET.to_owned()),
            ("flavor", Self::flavor().to_owned()),
            ("features", have),
        ];
        if missing != "none" {
            rows.push(("without", missing));
        }
        let width = rows.iter().map(|(label, _)| label.len()).max().unwrap_or(0);
        for (label, value) in rows {
            let label = format!("{label:<width$}");
            let _ = writeln!(
                out,
                "  {}  {}",
                label.if_supports_color(Stdout, |t| t.cyan().to_string()),
                value.if_supports_color(Stdout, |t| t.dimmed().to_string()),
            );
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::Version;

    #[test]
    fn flavor_names_a_released_build_or_admits_it_cannot() {
        assert!(matches!(Version::flavor(), "full" | "slim" | "custom"));
        let on = Version::FEATURES.iter().filter(|(_, on)| *on).count();
        match Version::flavor() {
            "full" => assert_eq!(on, Version::FEATURES.len()),
            "slim" => assert_eq!(on, 0),
            _ => assert!(on > 0 && on < Version::FEATURES.len()),
        }
    }

    #[test]
    fn every_feature_is_named_once() {
        for (i, (name, _)) in Version::FEATURES.iter().enumerate() {
            assert!(
                !Version::FEATURES[i + 1..].iter().any(|(o, _)| o == name),
                "`{name}` is listed twice"
            );
        }
    }

    #[test]
    fn the_long_form_reports_the_build() {
        let out = Version::long();
        assert!(out.contains(Version::SEMVER), "{out}");
        for label in ["commit", "rustc", "target", "flavor", "features"] {
            assert!(out.contains(label), "no `{label}` row: {out}");
        }
    }
}
