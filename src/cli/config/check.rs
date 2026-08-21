//! `baudelaire config check`: validate a config without building it.

use std::io::Read as _;
use std::path::{Path, PathBuf};

use clap::Args;

use super::super::{Cx, Run, help};
use crate::config::Config;
use crate::error::{BaudelaireErrorKind, ConfigError, Result};

#[derive(Args, Debug, Clone)]
#[command(after_help = CheckArgs::examples())]
pub struct CheckArgs {
    /// Config files to check; `-` reads standard input. Defaults to the one
    /// `--config` names.
    pub paths: Vec<PathBuf>,

    /// Check the text alone: no theme resolved, no project around it. What a
    /// documentation snippet or a fragment wants.
    #[arg(long)]
    pub isolated: bool,

    /// Report each fault as one `file:line:column: message` line, which is what
    /// a tool reading this output expects.
    #[arg(long)]
    pub compact: bool,
}

impl CheckArgs {
    /// Appended to `config check --help`.
    fn examples() -> String {
        help::Table::examples(&[
            ("baudelaire config check", "This project's config"),
            (
                "baudelaire -p prod config check",
                "...with the `prod` profile applied in full",
            ),
            (
                "baudelaire config check --isolated snippet.kdl",
                "A file that is not a project",
            ),
        ])
        .to_string()
    }

    /// The files to check: what was named, or the project's own config.
    fn paths(&self, cx: &Cx) -> Vec<PathBuf> {
        if self.paths.is_empty() {
            return vec![cx.cli.global.config.clone()];
        }
        self.paths.clone()
    }

    /// Standard input for `-`, the file otherwise.
    pub(super) fn read(path: &Path) -> Result<String> {
        if path == Path::new("-") {
            let mut text = String::new();
            std::io::stdin()
                .read_to_string(&mut text)
                .map_err(|e| crate::error::FsError::new(crate::error::fs::Op::Read, path, e))?;
            return Ok(text);
        }
        crate::fs::read_to_string(path).map_err(|e| match e {
            BaudelaireErrorKind::Fs(fs) if fs.kind() == std::io::ErrorKind::NotFound => {
                ConfigError::not_found(&path.display().to_string()).into()
            }
            other => other,
        })
    }

    /// Check one config the way a build would, so what fails here is what would
    /// fail there.
    fn checked(&self, text: &str, path: &Path, cx: &Cx) -> Result<Config> {
        let named = |e: BaudelaireErrorKind| match e {
            BaudelaireErrorKind::Config(config) => {
                BaudelaireErrorKind::Config(Box::new(config.named(path)))
            }
            other => other,
        };
        let config = if self.isolated {
            Config::parse(text).map_err(named)?
        } else {
            Config::load(text, cx.root.path(), cx.cli.global.theme.as_deref()).map_err(named)?
        };
        match &cx.cli.global.profile {
            Some(profile) => config.with_profile(profile).map_err(named),
            None => Ok(config),
        }
    }
}

impl Run for CheckArgs {
    fn run(&self, cx: &Cx) -> Result<()> {
        let paths = self.paths(cx);
        cx.ui.banner(format_args!(
            "checking {}",
            crate::ui::Text(&Listed(&paths).to_string())
        ));
        let mut faulted = false;
        for path in &paths {
            let text = match Self::read(path) {
                Ok(text) => text,
                Err(fault) if self.compact => {
                    Compact::new(path, None, &fault).report();
                    faulted = true;
                    continue;
                }
                Err(fault) => return Err(fault),
            };
            let config = match self.checked(&text, path, cx) {
                Ok(config) => config,
                Err(fault) if self.compact => {
                    Compact::new(path, Some(&text), &fault).report();
                    faulted = true;
                    continue;
                }
                Err(fault) => return Err(fault),
            };
            cx.ui.done(format_args!("{}", path.display()));
            for line in Checked(&config).lines() {
                cx.ui.detail(line);
            }
        }
        if faulted {
            return Err(ConfigError::invalid(&Listed(&paths).to_string()).into());
        }
        Ok(())
    }
}

/// A fault as the one line a tool reading this output expects: where it is, and
/// what is wrong, and nothing else.
struct Compact<'a> {
    path: &'a Path,
    /// The text the run actually checked, `None` where it could not be read.
    /// Carried rather than re-read: `-` is standard input and is gone.
    text: Option<&'a str>,
    fault: &'a BaudelaireErrorKind,
}

impl<'a> Compact<'a> {
    fn new(path: &'a Path, text: Option<&'a str>, fault: &'a BaudelaireErrorKind) -> Self {
        Self { path, text, fault }
    }

    fn report(&self) {
        eprintln!("{self}");
    }

    /// Where the first label points, one-based, or the top of the file when the
    /// fault carries no source of its own.
    fn at(&self) -> (usize, usize) {
        let offset = miette::Diagnostic::labels(self.fault)
            .and_then(|mut labels| labels.next())
            .map_or(0, |label| label.offset());
        let text = self.text.unwrap_or_default();
        let before = &text[..offset.min(text.len())];
        let line = before.matches('\n').count() + 1;
        let column = before
            .rsplit('\n')
            .next()
            .map_or(1, |last| last.chars().count() + 1);
        (line, column)
    }
}

impl std::fmt::Display for Compact<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (line, column) = self.at();
        write!(
            f,
            "{}:{line}:{column}: {}",
            self.path.display(),
            crate::ui::Styled::new(self.fault, false)
        )
    }
}

/// What checking one config also passed through, for the reader who wants to
/// know that the profile they rely on was covered.
struct Checked<'a>(&'a Config);

impl Checked<'_> {
    fn lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if let Some(theme) = &self.0.theme {
            lines.push(format!("theme {theme}"));
        }
        let profiles: Vec<&str> = self
            .0
            .profiles
            .iter()
            .map(|(name, _)| name.as_str())
            .collect();
        if !profiles.is_empty() {
            lines.push(format!("profiles {}", profiles.join(", ")));
        }
        lines
    }
}

/// A list of paths as one phrase, `-` named for what it is.
struct Listed<'a>(&'a [PathBuf]);

impl Listed<'_> {
    /// What `-` is called where a filename would read as one.
    const STDIN: &'static str = "standard input";
}

impl std::fmt::Display for Listed<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<String> = self
            .0
            .iter()
            .map(|path| match path.to_str() {
                Some("-") => Self::STDIN.to_owned(),
                _ => path.display().to_string(),
            })
            .collect();
        f.write_str(&names.join(", "))
    }
}
