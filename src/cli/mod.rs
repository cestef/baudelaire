//! Command-line interface: per-subcommand args, dispatch, and the wiring of
//! terminal output ([`crate::ui`]) and debug logging (`tracing`).

pub mod prompt;
pub mod remote;
pub mod scaffold;
pub mod serve;

use std::path::{Path, PathBuf};

use clap::builder::styling::{AnsiColor, Styles};
use clap::{Args, Parser, Subcommand};

use crate::config::Config;
use crate::error::cli::{Generated, UnknownKey};
use crate::error::warning::{CleanDefaults, CleanRefused, Uninferred};
use crate::error::{
    BaudelaireErrorKind, ConfigError, FsError, Op, Result, ScaffoldError, StrictWarnings,
};
use crate::ui::{Level, Ui, markup};
use crate::version::Version;

/// Help colouring, matched to the terminal UI palette: cyan for structure
/// (section headers, usage), green for the literals you type (commands and
/// flags), and dimmed `<VALUE>` placeholders, so a glance separates the words
/// to type from the slots to fill.
const HELP_STYLES: Styles = Styles::styled()
    .header(AnsiColor::Cyan.on_default().bold())
    .usage(AnsiColor::Cyan.on_default().bold())
    .literal(AnsiColor::Green.on_default().bold())
    .placeholder(AnsiColor::White.on_default().dimmed())
    .valid(AnsiColor::Green.on_default())
    .invalid(AnsiColor::Yellow.on_default())
    .error(AnsiColor::Red.on_default().bold());

/// Help-heading names, so the shared global flags cluster by concern instead of
/// piling into one long `Options` list. Single source, referenced by every
/// grouped `#[arg(help_heading = ..)]`.
mod group {
    pub const PROJECT: &str = "Project";
    pub const OUTPUT: &str = "Output";
    pub const BUILD: &str = "Build";
    pub const LOGGING: &str = "Logging";
    pub const SERVER: &str = "Server";
    pub const TARGETS: &str = "Targets";
    pub const CONTENT: &str = "Content";
}

/// The usage examples appended to the top-level help, as `(command, what it
/// does)`. A table rather than a sequence of calls, so the description column is
/// computed from the rows instead of hand-tuned to the longest one.
const EXAMPLES: &[(&str, &str)] = &[
    ("baudelaire", "Build the site from ./config.kdl"),
    (
        "baudelaire serve --open",
        "Start the dev server, open a browser",
    ),
    (
        "baudelaire new posts/hello",
        "Scaffold content/posts/hello.typ",
    ),
    (
        "baudelaire --profile prod build",
        "Build with the prod profile",
    ),
    ("baudelaire clean --cache", "Drop the incremental cache"),
];

/// The absolute project root: the directory `--root` selected (into which the
/// process changes so relative config paths resolve under it) or the launch
/// directory. Captured once and threaded, so nothing re-derives the root from
/// the process cwd.
pub(crate) struct Root(PathBuf);

impl Root {
    /// Enter and capture the project root. A `--root` argument changes the
    /// process cwd (the single side effect that makes every relative path in
    /// the config resolve under the chosen directory), then the absolute root
    /// is read once, here, and passed by value everywhere else.
    fn enter(dir: Option<&Path>) -> Result<Self> {
        if let Some(dir) = dir {
            std::env::set_current_dir(dir).map_err(|e| FsError::new(Op::Enter, dir, e))?;
        }
        let cwd =
            std::env::current_dir().map_err(|e| FsError::new(Op::Enter, Path::new("."), e))?;
        Ok(Self(cwd))
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }

    /// Resolve a path against the root (an absolute path is returned as-is).
    pub(crate) fn join(&self, path: impl AsRef<Path>) -> PathBuf {
        self.0.join(path)
    }

    #[cfg(test)]
    pub(crate) fn at(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }
}

/// Baudelaire: a Typst-native static site generator.
#[derive(Parser, Debug)]
#[command(
    name = "baudelaire",
    // `-V` is the one-line form a script greps; `--version` is the full report,
    // which is also what answers "does this binary even have the feature my
    // config is asking for".
    version = Version::SEMVER,
    long_version = Version::long(),
    about,
    long_about = "Baudelaire compiles a Typst content tree into a static site: incremental \
                  builds, a live-reload dev server, feeds, search, taxonomies, and more, all \
                  driven by Typst templates rather than HTML string templating.",
    styles = HELP_STYLES,
    after_help = Cli::examples(),
    subcommand_value_name = "COMMAND",
)]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalArgs,

    #[command(subcommand)]
    pub command: Option<Command>,
}

/// Arguments shared across *every* subcommand: project location and logging.
/// The build-shaping overrides live in [`BuildOverrides`], flattened only into
/// the commands that actually build, so `new`/`init`/`clean` help stays clean.
#[derive(Args, Debug, Clone)]
pub struct GlobalArgs {
    /// Path to config.kdl.
    #[arg(short, long, global = true, default_value = "config.kdl", help_heading = group::PROJECT)]
    pub config: PathBuf,

    /// Project root directory.
    #[arg(short, long, global = true, help_heading = group::PROJECT)]
    pub root: Option<PathBuf>,

    /// Named profile to apply (e.g. `dev`, `prod`).
    #[arg(short, long, global = true, help_heading = group::PROJECT)]
    pub profile: Option<String>,

    /// Verbose output: per-page progress plus debug logs (-vv for trace logs).
    #[arg(short, long, global = true, action = clap::ArgAction::Count, help_heading = group::LOGGING)]
    pub verbose: u8,

    /// Quiet output.
    #[arg(short, long, global = true, conflicts_with = "verbose", help_heading = group::LOGGING)]
    pub quiet: bool,

    /// Fail the run if anything warned.
    #[arg(long, global = true, help_heading = group::LOGGING)]
    pub strict: bool,

    /// Write a machine-readable summary of the run to stdout.
    #[arg(long, global = true, help_heading = group::LOGGING)]
    pub json: bool,
}

/// Config overrides that only make sense for a build: `build` and `serve`, and
/// equally `deploy`/`announce`, which build the site before publishing it and
/// so want the same levers (a preview `--base-url`, a staging `--drafts`, a
/// `--no-cache` clean artifact). Flattened per-command rather than made global
/// so scaffolding commands don't advertise irrelevant `--drafts`/`--no-cache`
/// flags.
#[derive(Args, Debug, Clone, Default)]
pub struct BuildOverrides {
    #[command(flatten)]
    pub common: CommonOverrides,

    /// Override the output directory.
    #[arg(short, long, help_heading = group::OUTPUT)]
    pub out: Option<PathBuf>,

    /// Use the incremental cache (default; `--no-cache` forces a full rebuild).
    #[arg(long, overrides_with = "no_cache", help_heading = group::BUILD)]
    pub cache: bool,
    #[arg(long, overrides_with = "cache", hide = true)]
    pub no_cache: bool,
}

/// The overrides that apply to any command reading the config, including the
/// ones that write nothing. `--out` and `--no-cache` are *not* here: `check`
/// writes no file and loads no cache, so advertising them there offered
/// settings that did nothing.
#[derive(Args, Debug, Clone, Default)]
pub struct CommonOverrides {
    /// Override the base URL.
    #[arg(long, help_heading = group::OUTPUT)]
    pub base_url: Option<String>,

    /// Build draft pages (`--no-drafts` excludes them, whatever the config says).
    #[arg(long, overrides_with = "no_drafts", help_heading = group::BUILD)]
    pub drafts: bool,
    #[arg(long, overrides_with = "drafts", hide = true)]
    pub no_drafts: bool,

    /// Build future-dated pages (`--no-future` excludes them).
    #[arg(long, overrides_with = "no_future", help_heading = group::BUILD)]
    pub future: bool,
    #[arg(long, overrides_with = "future", hide = true)]
    pub no_future: bool,

    /// Error on broken internal links (default; `--no-strict-links` warns instead).
    #[arg(long, overrides_with = "no_strict_links", help_heading = group::BUILD)]
    pub strict_links: bool,
    #[arg(long, overrides_with = "strict_links", hide = true)]
    pub no_strict_links: bool,
}

/// Every command carries a short alias, and they are *visible* aliases: they
/// appear in `--help` beside the full name, because a shorthand nobody can find
/// is not a shorthand.
///
/// The letters are first-come by frequency, not by spelling, which is why
/// `check` takes `c` and `clean` takes `cl`: `check` runs in a loop while
/// writing, `clean` runs when something has gone wrong. A single letter for the
/// destructive one, next to the letter for the harmless one, is also how a
/// slipped keystroke deletes a build.
///
/// These are an API the moment they ship. A future command cannot claim a
/// letter already listed here, so the table is worth reading before adding one.
#[derive(Subcommand, Debug, Clone)]
pub enum Command {
    /// Build the site (default when no subcommand given).
    #[command(visible_alias = "b")]
    Build(BuildArgs),
    /// Serve the site with a dev server and live rebuild.
    #[command(visible_alias = "s")]
    Serve(ServeArgs),
    /// Compile and check links without writing output.
    #[command(visible_alias = "c")]
    Check(CheckArgs),
    /// Scaffold a new content file.
    #[command(visible_alias = "n")]
    New(NewArgs),
    /// Announce the site's metadata to the configured destination (atproto/standard.site).
    #[cfg(feature = "announce")]
    #[command(visible_alias = "a")]
    Announce(AnnounceArgs),
    /// Deploy the built files to the configured destination (S3/R2 or SSH).
    #[command(visible_alias = "d")]
    Deploy(DeployArgs),
    /// Remove build output and local build state.
    #[command(visible_alias = "cl")]
    Clean(CleanArgs),
    /// Scaffold a new project (config.kdl + dirs).
    #[command(visible_alias = "i")]
    Init(InitArgs),
    /// Print a shell completion script to stdout.
    #[command(visible_alias = "comp")]
    Completions(CompletionsArgs),
    /// Print this manual as a man page (roff) to stdout.
    Man(ManArgs),
    /// Print every key config.kdl accepts, with its value shape.
    #[command(visible_alias = "ref")]
    Reference(ReferenceArgs),
}

/// Arguments for `baudelaire build`.
#[derive(Args, Debug, Clone, Default)]
pub struct BuildArgs {
    #[command(flatten)]
    pub overrides: BuildOverrides,
}

/// Arguments for `baudelaire completions`.
#[derive(Args, Debug, Clone)]
#[command(after_help = Shell::help())]
pub struct CompletionsArgs {
    /// Shell to generate a completion script for.
    #[arg(value_enum)]
    pub shell: Shell,
}

/// A shell `completions` can generate for.
///
/// Its own enum rather than [`clap_complete::Shell`] because nushell's
/// generator ships in a separate crate and so cannot be a variant of that one.
/// This is the single table mapping the name a user types to the generator that
/// answers it, and [`Shell::script`] is the only place that mapping is written.
#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    Bash,
    Elvish,
    Fish,
    Nushell,
    #[value(name = "powershell")]
    PowerShell,
    Zsh,
}

impl Shell {
    /// Where this shell wants the script.
    ///
    /// An exhaustive match rather than a lookup table, so adding a shell fails
    /// to compile until its install line is written; the `--help` text is
    /// rendered from these, and the value list clap prints comes from the same
    /// enum, so the two cannot drift.
    const fn install(self) -> &'static str {
        match self {
            Self::Bash => {
                "baudelaire completions bash > ~/.local/share/bash-completion/completions/baudelaire"
            }
            Self::Elvish => "baudelaire completions elvish > ~/.config/elvish/lib/baudelaire.elv",
            Self::Fish => {
                "baudelaire completions fish > ~/.config/fish/completions/baudelaire.fish"
            }
            Self::Nushell => {
                "baudelaire completions nushell > ~/.config/nushell/completions/baudelaire.nu"
            }
            Self::PowerShell => {
                "baudelaire completions powershell | Out-String | Invoke-Expression"
            }
            Self::Zsh => {
                "baudelaire completions zsh > ~/.local/share/zsh/site-functions/_baudelaire"
            }
        }
    }

    /// The per-shell install lines, appended to `completions --help`.
    ///
    /// Laid out like [`Cli::examples`], for the same reason: the shell column is
    /// measured from the rows rather than hand-tuned to the longest one.
    fn help() -> String {
        use clap::ValueEnum;
        use owo_colors::{OwoColorize, Stream::Stdout};
        use std::fmt::Write;

        let rows: Vec<_> = Self::value_variants()
            .iter()
            .filter_map(|shell| {
                let name = shell.to_possible_value()?;
                Some((name.get_name().to_owned(), shell.install()))
            })
            .collect();
        let column = rows.iter().map(|(n, _)| n.len()).max().unwrap_or(0) + 2;
        let mut out = format!(
            "{}\n",
            "Examples:".if_supports_color(Stdout, |t| t.cyan().bold().to_string())
        );
        for (name, line) in &rows {
            let pad = " ".repeat(column - name.len());
            let colored = line.if_supports_color(Stdout, |t| t.green().bold().to_string());
            let _ = writeln!(out, "  {name}{pad}{colored}");
        }
        let _ = write!(
            out,
            "\nThe directory has to exist, and the shell has to be told to read it;\n\
             {} cover both.",
            "your shell's completion docs".if_supports_color(Stdout, |t| t.dimmed().to_string())
        );
        out
    }

    /// Render the completion script for `command` under `name`.
    fn script(self, command: &mut clap::Command, name: String) -> Vec<u8> {
        use clap_complete::Shell as Builtin;

        let mut out = Vec::new();
        match self {
            Self::Nushell => {
                clap_complete::generate(clap_complete_nushell::Nushell, command, name, &mut out);
            }
            Self::Bash => clap_complete::generate(Builtin::Bash, command, name, &mut out),
            Self::Elvish => clap_complete::generate(Builtin::Elvish, command, name, &mut out),
            Self::Fish => clap_complete::generate(Builtin::Fish, command, name, &mut out),
            Self::PowerShell => {
                clap_complete::generate(Builtin::PowerShell, command, name, &mut out);
            }
            Self::Zsh => clap_complete::generate(Builtin::Zsh, command, name, &mut out),
        }
        out
    }
}

/// Arguments for `baudelaire reference`.
#[derive(Args, Debug, Clone)]
#[command(after_help = ReferenceArgs::examples())]
pub struct ReferenceArgs {
    /// A dotted key path to narrow to, e.g. `assets.images`.
    pub key: Option<String>,
}

/// Arguments for `baudelaire man`. Empty, and kept as a struct rather than
/// collapsed into a unit variant so the command matches every other one: a
/// subcommand is a `Command` variant, an args struct, one [`Run`] impl and one
/// delegating arm, with nowhere for a special case to grow.
#[derive(Args, Debug, Clone)]
pub struct ManArgs {}

/// Arguments for `baudelaire check`.
#[derive(Args, Debug, Clone)]
pub struct CheckArgs {
    #[command(flatten)]
    pub overrides: CommonOverrides,

    /// Also verify outbound `http(s)` links over the network
    /// (`--no-external` skips them even when the config asks).
    #[arg(long, overrides_with = "no_external", help_heading = group::BUILD)]
    pub external: bool,
    #[arg(long, overrides_with = "external", hide = true)]
    pub no_external: bool,
}

/// Arguments for `baudelaire serve`.
#[derive(Args, Debug, Clone)]
pub struct ServeArgs {
    #[command(flatten)]
    pub overrides: BuildOverrides,

    /// Port to listen on (overrides config).
    #[arg(long, help_heading = group::SERVER)]
    pub port: Option<u16>,

    /// Address to bind (overrides config).
    #[arg(long, help_heading = group::SERVER)]
    pub bind: Option<String>,

    /// Open a browser on start (default; `--no-open` suppresses it).
    #[arg(long, overrides_with = "no_open", help_heading = group::SERVER)]
    pub open: bool,
    #[arg(long, overrides_with = "open", hide = true)]
    pub no_open: bool,

    /// Watch for changes and rebuild (default; `--no-watch` serves statically).
    #[arg(long, overrides_with = "no_watch", help_heading = group::SERVER)]
    pub watch: bool,
    #[arg(long, overrides_with = "watch", hide = true)]
    pub no_watch: bool,

    /// Stamp each element with the source it came from, so alt-clicking the
    /// preview opens that line in `serve { editor }` (`html { spans }`).
    #[arg(long, overrides_with = "no_spans", help_heading = group::SERVER)]
    pub spans: bool,
    #[arg(long, overrides_with = "spans", hide = true)]
    pub no_spans: bool,
}

/// Arguments for `baudelaire announce`.
#[cfg(feature = "announce")]
#[derive(Args, Debug, Clone)]
pub struct AnnounceArgs {
    #[command(flatten)]
    pub overrides: BuildOverrides,

    /// Secret (app password / token) for the destination; `-` reads it from
    /// stdin. Prefer stdin, the environment variable, or the interactive prompt:
    /// a literal flag can leak into shell history.
    // Spelled the same as `deploy`'s: one concept, one name. `--password` stays
    // as an alias, since that is what the atproto side calls an app password.
    #[arg(long, alias = "password")]
    pub secret: Option<String>,
    /// Skip the confirmation prompt.
    #[arg(short = 'y', long)]
    pub yes: bool,
    /// Report what would change without writing to any destination.
    #[arg(long)]
    pub dry_run: bool,
}

#[cfg(feature = "announce")]
impl remote::Flags for AnnounceArgs {
    fn dry_run(&self) -> bool {
        self.dry_run
    }
    fn yes(&self) -> bool {
        self.yes
    }
    fn secret(&self) -> Option<String> {
        self.secret.clone()
    }
}

/// Arguments for `baudelaire deploy`.
#[derive(Args, Debug, Clone)]
pub struct DeployArgs {
    #[command(flatten)]
    pub overrides: BuildOverrides,

    /// Secret for the destination (S3 secret access key, or SSH password/key
    /// passphrase); `-` reads it from stdin. Prefer stdin, the backend's
    /// environment variable, or the interactive prompt: a literal flag can leak
    /// into shell history.
    #[arg(long)]
    pub secret: Option<String>,
    /// Skip the confirmation prompt.
    #[arg(short = 'y', long)]
    pub yes: bool,
    /// Report what would change without writing to any destination.
    #[arg(long)]
    pub dry_run: bool,
}

impl remote::Flags for DeployArgs {
    fn dry_run(&self) -> bool {
        self.dry_run
    }
    fn yes(&self) -> bool {
        self.yes
    }
    fn secret(&self) -> Option<String> {
        self.secret.clone()
    }
}

/// Arguments for `baudelaire clean`. With no target flag every directory is
/// swept; naming targets narrows it to those, so `clean --cache` forces a
/// rebuild without discarding announce state.
#[derive(Args, Debug, Clone, Default)]
pub struct CleanArgs {
    /// Remove everything: the output directory and all local build state.
    #[arg(long, help_heading = group::TARGETS)]
    pub all: bool,
    /// Remove the build output directory. Named for what it removes, not for
    /// the config key that locates it, which for this repo's own docs site is
    /// `docs/public`; `--dist` still works.
    #[arg(long, alias = "dist", help_heading = group::TARGETS)]
    pub output: bool,
    /// Remove the incremental build cache.
    #[arg(long, help_heading = group::TARGETS)]
    pub cache: bool,
    /// Remove local announce state.
    #[arg(long, help_heading = group::TARGETS)]
    pub announce: bool,

    /// Skip the confirmation prompt.
    #[arg(short = 'y', long)]
    pub yes: bool,
    /// Report what would be removed without removing it.
    #[arg(long)]
    pub dry_run: bool,
}

/// One nameable `clean` target: the flag that selects it and the directories it
/// removes. THE single source of what `clean` can sweep: a new target is one
/// row here plus its flag on [`CleanArgs`]; `all` and the narrowed `targets`
/// both derive from this table.
struct CleanTarget {
    selected: fn(&CleanArgs) -> bool,
    dirs: fn(&Config) -> Vec<PathBuf>,
}

const CLEAN_TARGETS: &[CleanTarget] = &[
    CleanTarget {
        selected: |a| a.output,
        dirs: |c| vec![c.paths.dist.clone()],
    },
    CleanTarget {
        selected: |a| a.cache,
        dirs: |c| vec![c.cache.dir.clone()],
    },
    CleanTarget {
        selected: |a| a.announce,
        dirs: |_| vec![Config::scratch("announce")],
    },
];

impl CleanArgs {
    /// Whether this is the wholesale wipe. Naming no target stays the shorthand
    /// for it, and `--all` is the spelling a script can state, so "I meant
    /// everything" and "I forgot the flag" stop being the same invocation.
    fn all(&self) -> bool {
        self.all || CLEAN_TARGETS.iter().all(|t| !(t.selected)(self))
    }

    /// Remove this invocation's targets, skipping any that would take the
    /// project with them.
    ///
    /// The paths are printed before anything is removed, whether or not they
    /// are about to be confirmed: they come from config, so the directory named
    /// `dist` is only the one you expect if the config says what you think it
    /// does.
    fn sweep(
        &self,
        ui: &Ui,
        config: &Config,
        root: &Path,
        interaction: &dyn crate::remote::Interaction,
    ) -> Result<()> {
        let dirs: Vec<PathBuf> = self
            .targets(config)
            .into_iter()
            .filter(|dir| dir.exists())
            .collect();
        if dirs.is_empty() {
            ui.done("nothing to clean");
            return Ok(());
        }
        for dir in &dirs {
            ui.detail(format_args!("- {}", dir.display()));
        }
        if self.dry_run {
            ui.done(format_args!("{} to remove", Self::count(dirs.len())));
            return Ok(());
        }
        if !self.consented(interaction, dirs.len())? {
            ui.done("nothing to clean");
            return Ok(());
        }
        let mut removed = 0;
        for dir in dirs {
            if !Self::removable(&dir, root) {
                ui.warn(CleanRefused { dir });
                continue;
            }
            crate::fs::remove_dir_all(&dir)?;
            removed += 1;
        }
        match removed {
            0 => ui.done("nothing to clean"),
            _ => ui.done("clean"),
        }
        Ok(())
    }

    /// The config to sweep by, falling back to the built-in paths when the file
    /// exists and does not load.
    ///
    /// A config that is missing entirely stays an error: `clean` would
    /// otherwise sweep `public` and `.baudelaire` out of whatever directory it
    /// was run in, which is not a recovery. One that exists and does not parse
    /// is the case worth recovering from, since `clean` is what you reach for
    /// when the project is in a state you want gone.
    fn config(cx: &Cx) -> Result<Config> {
        match cx.announced("cleaning") {
            Ok(config) => Ok(config),
            Err(error) if cx.cli.global.config.exists() => {
                cx.ui.banner("cleaning");
                cx.ui.warn(CleanDefaults {
                    errors: vec![error],
                });
                Ok(Config {
                    root: cx.root.path().to_path_buf(),
                    ..Config::default()
                })
            }
            Err(error) => Err(error),
        }
    }

    /// Whether the sweep may go ahead.
    ///
    /// Only the wholesale wipe asks. It takes the output directory *and* every
    /// scrap of local state with it, announce state included, which is remote
    /// reconciliation state: wiping it changes what the next `announce` does to
    /// a live repository. A narrowed `clean --cache` costs a rebuild and needs
    /// no ceremony.
    fn consented(
        &self,
        interaction: &dyn crate::remote::Interaction,
        count: usize,
    ) -> Result<bool> {
        use crate::remote::Consent;
        if !self.all() {
            return Ok(true);
        }
        let action = format!("remove {}", Self::count(count));
        match interaction.consent(&action, self.yes)? {
            Consent::Granted => Ok(true),
            Consent::Refused => Ok(false),
            Consent::Unattended => Err(crate::error::Unattended { action }.into()),
        }
    }

    /// `1 directory` / `3 directories`, for a prompt that reads as a sentence.
    fn count(dirs: usize) -> String {
        match dirs {
            1 => "1 directory".to_owned(),
            n => format!("{n} directories"),
        }
    }

    /// Whether `dir` may be removed: it must not be the project root, nor an
    /// ancestor of it.
    ///
    /// The paths come from config with no containment check of their own, so
    /// `paths { dist "." }` deleted the whole project and `cache { dir "/" }`
    /// everything above it. A `dist` deliberately placed outside the project
    /// stays cleanable: only swallowing the project is refused.
    fn removable(dir: &Path, root: &Path) -> bool {
        !crate::fs::canonical(root).starts_with(crate::fs::canonical(dir))
    }

    /// The directories to remove for this invocation. A full sweep clears the
    /// output plus the whole scratch root in one step (covering the cache,
    /// announce state, and any future intermediate); a relocated cache dir lives
    /// outside that root, so it is named explicitly. A narrowed sweep removes
    /// only the [`CLEAN_TARGETS`] whose flags were set.
    fn targets(&self, config: &Config) -> Vec<PathBuf> {
        if self.all() {
            let mut dirs = vec![config.paths.dist.clone(), PathBuf::from(Config::SCRATCH)];
            if !config.cache.dir.starts_with(Config::SCRATCH) {
                dirs.push(config.cache.dir.clone());
            }
            return dirs;
        }
        CLEAN_TARGETS
            .iter()
            .filter(|t| (t.selected)(self))
            .flat_map(|t| (t.dirs)(config))
            .collect()
    }
}

/// Arguments for `baudelaire new`.
#[derive(Args, Debug, Clone)]
pub struct NewArgs {
    /// Path for the new content file (e.g. `posts/my-post` or
    /// `content/posts/my-post.typ`). A bare name lands under the content dir.
    pub path: PathBuf,

    /// Page title (default: derived from the filename).
    #[arg(long, help_heading = group::CONTENT)]
    pub title: Option<String>,

    /// Publication date `YYYY-MM-DD` (default: today, for dated collections).
    #[arg(long, help_heading = group::CONTENT)]
    pub date: Option<String>,

    /// Mark the page a draft (default; `--no-draft` publishes it immediately).
    #[arg(long, overrides_with = "no_draft", help_heading = group::CONTENT)]
    pub draft: bool,
    #[arg(long, overrides_with = "draft", hide = true)]
    pub no_draft: bool,

    /// Create a page bundle (`<name>/index.typ`) for colocated assets.
    #[arg(short = 'b', long, help_heading = group::CONTENT)]
    pub bundle: bool,

    /// Open the new file in `$EDITOR` after creating it.
    // `--edit`, not `--open`: `serve --open` opens a browser, and the two are
    // unrelated. The short form was already `-e`, so the flag's own two names
    // disagreed about what it was called.
    #[arg(short = 'e', long, alias = "open", help_heading = group::CONTENT)]
    pub edit: bool,
}

/// Arguments for `baudelaire init`.
#[derive(Args, Debug, Clone)]
pub struct InitArgs {
    /// Directory to scaffold into (default: current directory).
    pub dir: Option<PathBuf>,

    /// Starter shape. The accepted values and the default both come from the
    /// template registry, so the help cannot name a shape that is not there.
    #[arg(
        short = 't',
        long,
        default_value = scaffold::templates::Template::DEFAULT,
        help = scaffold::templates::Template::help(),
        help_heading = group::PROJECT,
    )]
    pub template: String,

    /// Site title (default: prompted, or the directory name).
    #[arg(long, help_heading = group::PROJECT)]
    pub title: Option<String>,

    /// Site author (default: prompted, or your git `user.name`).
    #[arg(long, help_heading = group::PROJECT)]
    pub author: Option<String>,

    /// Canonical base URL (default: prompted).
    #[arg(long, help_heading = group::PROJECT)]
    pub url: Option<String>,

    /// Default language code.
    #[arg(long, default_value = "en", help_heading = group::PROJECT)]
    pub lang: String,

    /// Take templates and assets from a theme package instead of scaffolding
    /// copies of them.
    #[arg(long, value_name = "SPEC", help_heading = group::PROJECT)]
    pub theme: Option<String>,

    /// Switch on optional features. Listed from the same table that resolves
    /// them, so a new feature is documented by existing.
    #[arg(
        long,
        value_name = "FEATURE",
        value_delimiter = ',',
        help = scaffold::templates::Extra::help(),
        help_heading = group::PROJECT,
    )]
    pub with: Vec<String>,

    /// Scaffold the shape without the example pages.
    #[arg(long, help_heading = group::PROJECT)]
    pub no_sample: bool,

    /// Take the default answer to every prompt instead of asking.
    #[arg(short = 'y', long)]
    pub yes: bool,
    // The accepted values are not spelled out: `value_enum` already lists them
    // from [`scaffold::Vcs`] itself, so a new variant documents itself.
    /// Set up this version-control system without asking.
    #[arg(long, value_enum)]
    pub vcs: Option<scaffold::Vcs>,
}

impl Cli {
    /// [`EXAMPLES`] rendered for the bottom of the top-level help. owo-colors
    /// gates the colour on the stdout stream itself (`if_supports_color`), so
    /// escapes never leak when piped or under `NO_COLOR`: the same policy
    /// [`crate::ui`] uses.
    fn examples() -> String {
        use owo_colors::{OwoColorize, Stream::Stdout};
        use std::fmt::Write;
        // The description column: past the longest command, with a two-space
        // gutter. Padding is measured on the *visible* length, so the ANSI
        // escapes cannot skew the alignment.
        let column = EXAMPLES.iter().map(|(c, _)| c.len()).max().unwrap_or(0) + 2;
        let mut out = format!(
            "{}\n",
            "Examples:".if_supports_color(Stdout, |t| t.cyan().bold().to_string())
        );
        for (command, desc) in EXAMPLES {
            // The command in green: the "literal you type" accent.
            let colored = command.if_supports_color(Stdout, |t| t.green().bold().to_string());
            let pad = " ".repeat(column - command.len());
            let _ = writeln!(out, "  {colored}{pad}{desc}");
        }
        let _ = write!(
            out,
            "\nRun {} for command-specific options.",
            "baudelaire <command> --help"
                .if_supports_color(Stdout, |t| t.green().bold().to_string())
        );
        out
    }

    /// The parsed config: read from `--config`, then narrowed by the active
    /// profile. Build-time overrides ([`BuildOverrides`]) are applied per-command
    /// by the caller, not here, so `new`/`clean` load the same untouched config.
    pub fn config(&self) -> Result<Config> {
        let text = self.read()?;
        // Name every config diagnostic after the file actually loaded, the one
        // place the path is known: otherwise no config error says which file it
        // came from, and `--config prod.kdl` reported `config.kdl`.
        let named = |e: BaudelaireErrorKind| match e {
            BaudelaireErrorKind::Config(config) => {
                BaudelaireErrorKind::Config(Box::new(config.named(&self.global.config)))
            }
            other => other,
        };
        // `Root::enter` has already made the project directory the cwd, so a
        // directory-theme resolves against it.
        let mut config = Config::load(&text, Path::new(".")).map_err(named)?;
        if let Some(profile) = &self.global.profile {
            config = config.with_profile(profile).map_err(named)?;
        }
        Ok(config)
    }

    /// Read the config file, mapping only a genuinely missing file to a
    /// [`ConfigError::not_found`]. Every other failure (permission denied, a
    /// directory, invalid UTF-8) keeps its precise [`FsError`] diagnostic rather
    /// than being flattened into "config not found".
    fn read(&self) -> Result<String> {
        let path = &self.global.config;
        crate::fs::read_to_string(path).map_err(|e| match e {
            BaudelaireErrorKind::Fs(fs) if fs.kind() == std::io::ErrorKind::NotFound => {
                ConfigError::not_found(&path.display().to_string()).into()
            }
            other => other,
        })
    }

    /// The UI verbosity. `-vv` and beyond only deepen the `tracing` filter
    /// (see [`crate::ui::trace`]): the terminal report itself has one
    /// verbose level.
    fn level(&self) -> Level {
        let g = &self.global;
        if g.quiet {
            Level::Quiet
        } else if g.verbose > 0 {
            Level::Verbose
        } else {
            Level::Default
        }
    }
}

/// What a `--x` / `--no-x` flag pair says about a boolean setting: turn it on,
/// turn it off, or leave whatever the config decided.
///
/// Every overridable boolean in the CLI is spelled as such a pair, resolved
/// here. Three idioms used to coexist, and each was wrong in its own way:
///
/// - A tri-state `Option<bool>` taking an optional value (`--strict-links
///   [<bool>]`). It read the *next* argument as its value, so the natural
///   `baudelaire new --draft posts/foo` failed with "invalid value 'posts/foo'
///   for '--draft'", an error that never mentions the real cause.
/// - A plain `bool` that could only push a setting on (`--drafts`, `--future`),
///   so `draft { build #true }` in config had no CLI route back to a production
///   build, and `serve { watch #false }` no way to turn watching on.
/// - One that accumulated with `|=` (`--external`), which is the same one-way
///   street with the reasoning buried in a code comment.
#[derive(Debug, Clone, Copy, Default)]
struct Toggle(Option<bool>);

impl Toggle {
    /// Resolve a pair. Each flag `overrides_with` the other, so clap lets the
    /// last one on the command line win and at most one arrives set.
    fn of(on: bool, off: bool) -> Self {
        Self(on.then_some(true).or(off.then_some(false)))
    }

    /// Overlay onto a config field, leaving it untouched when neither flag was
    /// passed, which is what makes the config the default rather than the flag.
    fn apply(self, target: &mut bool) {
        if let Some(value) = self.0 {
            *target = value;
        }
    }

    /// The value, or `default` when neither flag was passed. For a toggle with
    /// no config field behind it.
    fn or(self, default: bool) -> bool {
        self.0.unwrap_or(default)
    }
}

/// A set of CLI flags that overlay the loaded config.
trait Overrides {
    fn apply(&self, config: &mut Config);
}

impl Overrides for BuildOverrides {
    fn apply(&self, config: &mut Config) {
        self.common.apply(config);
        if let Some(out) = &self.out {
            config.paths.dist = out.clone();
        }
        Toggle::of(self.cache, self.no_cache).apply(&mut config.cache.incremental);
    }
}

impl Overrides for CommonOverrides {
    fn apply(&self, config: &mut Config) {
        if let Some(url) = &self.base_url {
            config.url = Some(url.clone());
        }
        Toggle::of(self.drafts, self.no_drafts).apply(&mut config.content.draft.build);
        Toggle::of(self.future, self.no_future).apply(&mut config.content.future);
        Toggle::of(self.strict_links, self.no_strict_links).apply(&mut config.links.strict);
    }
}

/// Run a parsed CLI: install the debug-log subscriber, dispatch, and flush any
/// collected warnings, on success and failure alike, so a failed run still
/// shows what it warned about before dying.
pub fn run(cli: Cli) -> Result<()> {
    crate::ui::trace::init(cli.global.verbose);
    let ui = Ui::new(cli.level());
    let result = dispatch(&cli, &ui);
    // Counted before the flush empties them, and reported after, so `--strict`
    // fails *behind* the warnings that explain it rather than in front of them.
    let warned = ui.warnings();
    let outcome = result.and_then(|()| match cli.global.strict && warned > 0 {
        true => Err(StrictWarnings { count: warned }.into()),
        false => Ok(()),
    });
    // Built while the diagnostics are still collected, and written before the
    // flush, so the JSON object lands on stdout uninterleaved with the prose on
    // stderr.
    if cli.global.json {
        emit_json(&ui.summary(outcome.is_ok()));
    }
    ui.flush();
    outcome
}

/// Write the run's summary to stdout, the one thing that ever goes there.
///
/// A serialization failure is swallowed rather than replacing the run's real
/// outcome: the report describes what happened, and losing the description is
/// not the same as the thing having failed.
fn emit_json(report: &crate::ui::Report) {
    use std::io::Write;
    if let Ok(text) = serde_json::to_string(report) {
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{text}");
        let _ = out.flush();
    }
}

/// Dispatch to the matching subcommand. Each command owns its wiring in a
/// [`Run`] impl; this only picks the variant (defaulting to `build`) and hands
/// it the shared [`Cx`].
fn dispatch(cli: &Cli, ui: &Ui) -> Result<()> {
    let root = Root::enter(cli.global.root.as_deref())?;
    let command = cli
        .command
        .clone()
        .unwrap_or_else(|| Command::Build(BuildArgs::default()));
    command.run(&Cx {
        cli,
        ui,
        root: &root,
    })
}

/// The shared context a subcommand runs against: the parsed CLI (for config +
/// global flags), the terminal UI, and the entered project root.
struct Cx<'a> {
    cli: &'a Cli,
    ui: &'a Ui,
    root: &'a Root,
}

impl Cx<'_> {
    /// Load config and announce the run: the shared front matter of every
    /// command that operates on an existing project.
    fn announced(&self, verb: &str) -> Result<Config> {
        let mut config = self.cli.config()?;
        // `Root::enter` has already moved the cwd, so every relative config path
        // resolves under it; record it so nothing has to re-derive the root from
        // one of those paths.
        config.root = self.root.path().to_path_buf();
        self.ui.banner(format_args!("{verb} {}", config.label()));
        Ok(config)
    }

    /// [`Cx::announced`] plus the build-shaping overrides: the front matter of
    /// the build-shaped commands (`build`, `check`). Overrides never touch the
    /// site name, so applying them after the banner leaves its text unchanged.
    fn configured(&self, overrides: &impl Overrides, verb: &str) -> Result<Config> {
        let mut config = self.announced(verb)?;
        overrides.apply(&mut config);
        Ok(config)
    }
}

/// One CLI subcommand's behavior. Each args struct implements it, so a new
/// subcommand is a `Command` variant, an `impl Run`, and one delegating arm.
trait Run {
    fn run(&self, cx: &Cx) -> Result<()>;
}

impl Command {
    /// Delegate to the selected subcommand's [`Run`] impl.
    fn run(&self, cx: &Cx) -> Result<()> {
        match self {
            Command::Build(args) => args.run(cx),
            Command::Serve(args) => args.run(cx),
            Command::Check(args) => args.run(cx),
            Command::New(args) => args.run(cx),
            #[cfg(feature = "announce")]
            Command::Announce(args) => args.run(cx),
            Command::Deploy(args) => args.run(cx),
            Command::Clean(args) => args.run(cx),
            Command::Init(args) => args.run(cx),
            Command::Completions(args) => args.run(cx),
            Command::Man(args) => args.run(cx),
            Command::Reference(args) => args.run(cx),
        }
    }
}

/// Both of these describe the CLI rather than a site, so they read no config,
/// touch no project, and write their one document to stdout, which `--json`
/// otherwise reserves. Nothing else is printed: the output is meant to be
/// redirected into a completion directory or `man1/`, and a banner in front of
/// it would corrupt the file.
impl Run for CompletionsArgs {
    fn run(&self, _cx: &Cx) -> Result<()> {
        use clap::CommandFactory;

        let mut command = Cli::command();
        let name = command.get_name().to_string();
        // Rendered into memory first so the whole script reaches stdout in one
        // write, and so a failed write is one error rather than a half-written
        // completion file that a shell would happily source.
        let script = self.shell.script(&mut command, name);
        Generated::Completions.emit(&script)?;
        Ok(())
    }
}

impl Run for ManArgs {
    fn run(&self, _cx: &Cx) -> Result<()> {
        use clap::CommandFactory;

        let mut page = Vec::new();
        Generated::Man.check(clap_mangen::Man::new(Cli::command()).render(&mut page))?;
        Generated::Man.emit(&page)?;
        Ok(())
    }
}

impl ReferenceArgs {
    /// Appended to `reference --help`. The narrowing argument is the part worth
    /// showing: the bare command prints a hundred and fifty keys.
    fn examples() -> String {
        use owo_colors::{OwoColorize, Stream::Stdout};

        let rows = [
            ("baudelaire reference", "Every key"),
            ("baudelaire reference assets", "Just the asset pipeline"),
            (
                "baudelaire reference deploy.s3",
                "Just the S3 deploy backend",
            ),
        ];
        let column = rows.iter().map(|(c, _)| c.len()).max().unwrap_or(0) + 2;
        let mut out = format!(
            "{}\n",
            "Examples:".if_supports_color(Stdout, |t| t.cyan().bold().to_string())
        );
        for (command, what) in rows {
            let pad = " ".repeat(column - command.len());
            let colored = command.if_supports_color(Stdout, |t| t.green().bold().to_string());
            let _ = std::fmt::Write::write_fmt(&mut out, format_args!("  {colored}{pad}{what}\n"));
        }
        out
    }
}

impl Run for ReferenceArgs {
    fn run(&self, _cx: &Cx) -> Result<()> {
        use crate::config::reference::{Reference, Terminal};

        let reference = match &self.key {
            None => Reference::new(),
            // The nearest real path, and *not* the whole list: there are over a
            // hundred and fifty of them, and a help that prints them all is a
            // wall rather than an answer. The command with no argument is the
            // list, so the help says so instead.
            Some(key) => Reference::at(key).ok_or_else(|| {
                let all = Reference::new();
                let paths = all.paths();
                let keys = crate::config::dispatch::Keys::of(&paths);
                UnknownKey {
                    key: key.clone(),
                    help: match keys.nearest(key) {
                        Some(near) => markup!("did you mean `{}`?", near),
                        None => markup!("run `{}` for every key", "baudelaire reference"),
                    },
                }
            })?,
        };
        Generated::Reference.emit(Terminal(&reference).to_string().as_bytes())?;
        Ok(())
    }
}

impl Run for BuildArgs {
    fn run(&self, cx: &Cx) -> Result<()> {
        let config = cx.configured(&self.overrides, "building")?;
        let stats = crate::engine::Engine::new(config, crate::engine::Mode::Build)?.build(cx.ui)?;
        cx.ui.built(stats.pages, stats.cached);
        Ok(())
    }
}

impl Run for CheckArgs {
    fn run(&self, cx: &Cx) -> Result<()> {
        let mut config = cx.configured(&self.overrides, "checking")?;
        Toggle::of(self.external, self.no_external).apply(&mut config.links.external);
        let stats = crate::engine::Engine::new(config, crate::engine::Mode::Check)?.check(cx.ui)?;
        cx.ui.built(stats.pages, stats.cached);
        Ok(())
    }
}

impl Run for ServeArgs {
    fn run(&self, cx: &Cx) -> Result<()> {
        let mut config = cx.cli.config()?;
        self.apply(&mut config);
        // Prefix the site name with the active profile, if one was requested.
        use owo_colors::OwoColorize;
        match cx.cli.global.profile.as_deref() {
            Some(profile) => cx.ui.banner(format_args!(
                "{} · {}",
                profile.cyan().bold(),
                config.label()
            )),
            None => cx.ui.banner(format_args!("{}", config.label())),
        }
        // Re-reads config.kdl with the same profile + overrides, so the dev
        // server picks up config edits live.
        let reload = || -> Result<Config> {
            let mut config = cx.cli.config()?;
            self.apply(&mut config);
            Ok(config)
        };
        crate::cli::serve::run(cx.ui, config, cx.root, cx.cli.global.config.clone(), reload)
    }
}

impl Run for NewArgs {
    fn run(&self, cx: &Cx) -> Result<()> {
        let config = cx.cli.config()?;
        // A project lets `new` read the existing content: next order in an
        // ordered collection, and permalink collisions. Both are conveniences,
        // so a content tree that cannot be opened costs the inference and warns
        // rather than refusing to write the file.
        let project = match crate::world::Project::new(&config, crate::engine::Mode::Build) {
            Ok(project) => Some(project),
            Err(error) => {
                cx.ui.warn(Uninferred {
                    errors: vec![error],
                });
                None
            }
        };
        scaffold::Draft::plan(self, &config, project.as_ref())?.create(cx.ui)
    }
}

#[cfg(feature = "announce")]
impl Run for AnnounceArgs {
    fn run(&self, cx: &Cx) -> Result<()> {
        let config = cx.configured(&self.overrides, "announcing")?;
        remote::run(cx.ui, &config, self, crate::announce::run)
    }
}

impl Run for DeployArgs {
    fn run(&self, cx: &Cx) -> Result<()> {
        let config = cx.configured(&self.overrides, "deploying")?;
        remote::run(cx.ui, &config, self, crate::deploy::run)
    }
}

impl Run for CleanArgs {
    fn run(&self, cx: &Cx) -> Result<()> {
        let config = Self::config(cx)?;
        self.sweep(cx.ui, &config, cx.root.path(), &prompt::Tty)
    }
}

impl Run for InitArgs {
    fn run(&self, cx: &Cx) -> Result<()> {
        // The two globals `init` cannot honour: it writes the config every other
        // command reads, so `--config` names the file to write rather than one
        // to read, and there is no profile to select in a project that does not
        // exist yet. Both used to be accepted and ignored.
        if cx.cli.global.profile.is_some() {
            return Err(ScaffoldError::Profile.into());
        }
        cx.ui.banner("init");
        scaffold::init(cx.ui, cx.root, self, &cx.cli.global.config)
    }
}

impl NewArgs {
    /// The file `baudelaire new` should create: a relative path lands under
    /// the content directory (unless it already starts with it, so an explicit
    /// `content/posts/foo.typ` is not double-prefixed), and `.typ` is appended
    /// when the name does not already carry it.
    /// Whether the scaffolded page is a draft. Drafting is the default: a page
    /// being written is not one being published, and `--no-draft` says so.
    pub(crate) fn is_draft(&self) -> bool {
        Toggle::of(self.draft, self.no_draft).or(true)
    }

    pub(crate) fn target(&self, config: &Config) -> PathBuf {
        let mut path = if self.path.is_absolute() || self.path.starts_with(&config.paths.content) {
            self.path.clone()
        } else {
            config.paths.content.join(&self.path)
        };
        // A bundle is a directory holding an `index.typ` (the collection's
        // configured index name), so images and data can sit beside the page.
        if self.bundle {
            path.set_extension("");
            return path.join(format!("{}.typ", config.bundle_index()));
        }
        if path.extension().is_none_or(|e| e != "typ") {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("untitled");
            path.set_file_name(format!("{name}.typ"));
        }
        path
    }
}

impl ServeArgs {
    fn apply(&self, config: &mut Config) {
        self.overrides.apply(config);
        if let Some(port) = self.port {
            config.serve.port = port;
        }
        if let Some(bind) = &self.bind {
            config.serve.bind = bind.clone();
        }
        Toggle::of(self.open, self.no_open).apply(&mut config.serve.open);
        Toggle::of(self.watch, self.no_watch).apply(&mut config.serve.watch);
        // Into `html`, not `serve`: the stamps are markup, and the cache keys on
        // the config that shapes markup. A preview-only switch that the
        // fingerprint could not see would leave a later `build` reusing these
        // pages with their attributes still on.
        Toggle::of(self.spans, self.no_spans).apply(&mut config.html.spans);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(output: bool, cache: bool, announce: bool) -> CleanArgs {
        CleanArgs {
            output,
            cache,
            announce,
            ..CleanArgs::default()
        }
    }

    /// A target that would take the project with it is refused: `paths { dist
    /// "." }` used to delete the whole project, `cache { dir "/" }` everything
    /// above it.
    #[test]
    fn a_target_containing_the_project_is_not_removable() {
        let root = Path::new("/home/me/site");
        assert!(!CleanArgs::removable(Path::new("/home/me/site"), root));
        assert!(!CleanArgs::removable(Path::new("/home/me"), root));
        assert!(!CleanArgs::removable(Path::new("/"), root));
        assert!(CleanArgs::removable(
            Path::new("/home/me/site/public"),
            root
        ));
        // A dist deliberately placed outside the project stays cleanable.
        assert!(CleanArgs::removable(Path::new("/srv/www"), root));
    }

    /// `deploy` and `announce` build before they publish, so they take the same
    /// levers as `build`. Without them the only way to publish a preview was a
    /// named profile per permutation.
    #[test]
    fn a_publishing_command_takes_the_build_overrides() {
        use clap::Parser;
        let cli = Cli::parse_from([
            "baudelaire",
            "deploy",
            "--base-url",
            "https://preview.example/",
            "--drafts",
            "--no-cache",
        ]);
        let Some(Command::Deploy(args)) = cli.command else {
            panic!("expected deploy")
        };
        let mut config = Config::default();
        args.overrides.apply(&mut config);
        assert_eq!(config.url.as_deref(), Some("https://preview.example/"));
        assert!(config.content.draft.build);
        assert!(!config.cache.incremental);
    }

    /// A headless session: nobody to put the question to.
    struct Headless;

    impl crate::remote::Interaction for Headless {
        fn interactive(&self) -> bool {
            false
        }
        fn confirm(&self, _prompt: &str) -> Result<bool> {
            unreachable!("never asked without a terminal")
        }
        fn secret(&self, _label: &str) -> Result<Option<String>> {
            Ok(None)
        }
    }

    /// The wholesale wipe asks; a narrowed sweep does not. `clean` with no flag
    /// takes the output directory *and* announce state, which decides what the
    /// next `announce` does to a live repository, so an unattended run has to
    /// stop rather than answer for itself. `clean --cache` costs a rebuild.
    #[test]
    fn only_a_full_sweep_asks_for_consent() {
        assert!(args(false, true, false).consented(&Headless, 1).unwrap());
        assert!(matches!(
            CleanArgs::default().consented(&Headless, 3),
            Err(BaudelaireErrorKind::Unattended(_))
        ));
        let yes = CleanArgs {
            yes: true,
            ..CleanArgs::default()
        };
        assert!(yes.consented(&Headless, 3).unwrap());
    }

    /// `--all` is the sweep a script can state outright, so the most
    /// destructive invocation stops being the shortest one by accident.
    #[test]
    fn all_is_explicit_as_well_as_implicit() {
        assert!(CleanArgs::default().all());
        assert!(!args(false, true, false).all());
        let explicit = CleanArgs {
            all: true,
            cache: true,
            ..CleanArgs::default()
        };
        assert!(explicit.all());
    }

    #[test]
    fn full_sweep_clears_output_and_scratch_root() {
        let config = Config::default();
        let targets = args(false, false, false).targets(&config);
        assert!(targets.contains(&config.paths.dist));
        assert!(targets.contains(&PathBuf::from(Config::SCRATCH)));
        // The default cache lives under the scratch root, so it is not named
        // separately: the root sweep already covers it.
        assert!(!targets.contains(&config.cache.dir));
    }

    #[test]
    fn full_sweep_names_a_relocated_cache() {
        let mut config = Config::default();
        config.cache.dir = PathBuf::from("/var/tmp/bd-cache");
        assert!(
            args(false, false, false)
                .targets(&config)
                .contains(&config.cache.dir)
        );
    }

    #[test]
    fn narrowed_sweep_targets_only_the_named_dirs() {
        let config = Config::default();
        assert_eq!(
            args(false, true, false).targets(&config),
            vec![config.cache.dir.clone()]
        );
        assert_eq!(
            args(false, false, true).targets(&config),
            vec![Config::scratch("announce")]
        );
        assert_eq!(
            args(true, false, false).targets(&config),
            vec![config.paths.dist.clone()]
        );
    }
}
