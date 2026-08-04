//! What this binary *is*: its version, where it was built from, and which
//! optional capabilities were compiled into it.
//!
//! Distinct from [`crate::world::BuildContext`], which is what a *site's* build
//! knows about itself and reaches pages through `sys.inputs.baudelaire`. This
//! is about the compiler's output, not the site's.

use std::fmt::Write as _;
use std::sync::LazyLock;

use owo_colors::{OwoColorize, Stream::Stdout};

/// The version, and the provenance behind it.
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

    /// Every optional capability and whether this binary has it.
    ///
    /// THE single source of what a build can do, which nothing else in the tree
    /// could answer: `GATES` in [`crate::engine`] is keyed on the *config
    /// settings* a missing feature degrades, so it names `css` twice and knows
    /// nothing of the features that gate no setting. A user whose `js` config
    /// silently does nothing could not tell a config mistake from a slim
    /// binary, because no command reported which one they were holding.
    ///
    /// `vendored-openssl` is absent on purpose: it decides how OpenSSL is
    /// *linked*, not what the binary can do, and the musl release turns it on
    /// over the full default set. Counting it would report that build as
    /// `custom` when it is exactly the published `full` one. `sidecars` is
    /// absent for the neighbouring reason: nobody selects it, every artifact
    /// drawn from a paged compile turns it on, and it gates no capability of its
    /// own that the artifact's feature does not already name.
    pub const FEATURES: &'static [(&'static str, bool)] = &[
        ("announce", cfg!(feature = "announce")),
        ("cards", cfg!(feature = "cards")),
        ("css", cfg!(feature = "css")),
        ("embedded-fonts", cfg!(feature = "embedded-fonts")),
        ("images", cfg!(feature = "images")),
        ("js", cfg!(feature = "js")),
        ("pdf", cfg!(feature = "pdf")),
        ("ssh", cfg!(feature = "ssh")),
        ("themes", cfg!(feature = "themes")),
    ];

    /// The released flavor this binary matches, named the way `install.sh`
    /// spells it so what `--version` reports is what you would ask for.
    ///
    /// `custom` is the honest answer for a build that is neither: it is what
    /// tells you no published tarball matches what you are running.
    pub fn flavor() -> &'static str {
        let on = Self::FEATURES.iter().filter(|(_, on)| *on).count();
        match on {
            0 => "slim",
            n if n == Self::FEATURES.len() => "full",
            _ => "custom",
        }
    }

    /// The optional capabilities this binary has, and the ones it lacks, each
    /// as one line. A slim build has none of the first, and an empty row would
    /// read as a rendering fault rather than as the answer.
    fn split() -> (String, String) {
        let pick = |want: bool| -> String {
            let names: Vec<&str> = Self::FEATURES
                .iter()
                .filter(|(_, on)| *on == want)
                .map(|(name, _)| *name)
                .collect();
            match names.is_empty() {
                true => "none".to_owned(),
                false => names.join(" "),
            }
        };
        (pick(true), pick(false))
    }

    /// The full report, for `--version`. Rendered once: clap wants a `'static`
    /// string, and whether stdout is a terminal cannot change mid-process.
    pub fn long() -> &'static str {
        static LONG: LazyLock<String> = LazyLock::new(Version::render);
        LONG.as_str()
    }

    /// Build the full report.
    ///
    /// Colour is gated on the stdout stream itself, the same policy
    /// [`crate::ui`] and the help footer follow, so escapes never survive a
    /// pipe or `NO_COLOR`.
    fn render() -> String {
        let (have, missing) = Self::split();
        // No program name: clap prints one in front of whatever this returns,
        // and two would read as a stutter.
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
        // Only when something is off: the row exists to answer "why is my
        // config doing nothing", which has an answer only when a feature is
        // missing, and a full build saying `without none` is noise.
        if missing != "none" {
            rows.push(("without", missing));
        }
        // Pad before styling: a width applied to a styled label would count its
        // escape codes and misalign the column, the same trap `Ui::arrow` names.
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

    /// The flavor is what a user would ask `install.sh` for, so it has to be
    /// one of the two published names or say plainly that it is neither.
    #[test]
    fn flavor_names_a_released_build_or_admits_it_cannot() {
        assert!(matches!(Version::flavor(), "full" | "slim" | "custom"));
        // Whichever this binary is, the table agrees with it.
        let on = Version::FEATURES.iter().filter(|(_, on)| *on).count();
        match Version::flavor() {
            "full" => assert_eq!(on, Version::FEATURES.len()),
            "slim" => assert_eq!(on, 0),
            _ => assert!(on > 0 && on < Version::FEATURES.len()),
        }
    }

    /// Every feature is listed once, since the report reads as an inventory:
    /// a name appearing twice would claim a capability twice.
    #[test]
    fn every_feature_is_named_once() {
        for (i, (name, _)) in Version::FEATURES.iter().enumerate() {
            assert!(
                !Version::FEATURES[i + 1..].iter().any(|(o, _)| o == name),
                "`{name}` is listed twice"
            );
        }
    }

    /// The report names what it knows: the version, and each row's label. Left
    /// unstyled here because tests capture a pipe, which is the point of
    /// gating colour on the stream.
    #[test]
    fn the_long_form_reports_the_build() {
        let out = Version::long();
        assert!(out.contains(Version::SEMVER), "{out}");
        for label in ["commit", "rustc", "target", "flavor", "features"] {
            assert!(out.contains(label), "no `{label}` row: {out}");
        }
    }
}
