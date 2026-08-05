//! Command-line interface: per-subcommand args, dispatch, and the wiring of
//! terminal output ([`crate::ui`]) and debug logging (`tracing`).

#[cfg(feature = "announce")]
pub mod announce;
pub mod build;
pub mod check;
pub mod clean;
pub mod completions;
pub mod deploy;
mod help;
pub mod init;
pub mod man;
pub mod mirror;
pub mod new;
pub mod prompt;
pub mod reference;
pub mod remote;
pub mod scaffold;
pub mod serve;
#[cfg(feature = "themes")]
pub mod theme;

use std::path::{Path, PathBuf};

use clap::builder::styling::{AnsiColor, Styles};
use clap::{Args, Parser, Subcommand};

use crate::config::Config;
use crate::error::{BaudelaireErrorKind, ConfigError, FsError, Op, Result, StrictWarnings};
use crate::ui::{Level, Ui};
// Counted output lives in the theme verbs alone, which a slim build does not
// carry.
use crate::version::Version;

#[cfg(feature = "announce")]
pub use announce::AnnounceArgs;
pub use build::BuildArgs;
pub use check::CheckArgs;
pub use clean::CleanArgs;
pub use completions::{CompletionsArgs, Shell};
pub use deploy::DeployArgs;
pub use init::InitArgs;
pub use man::ManArgs;
pub use mirror::MirrorArgs;
pub use new::NewArgs;
pub use reference::ReferenceArgs;
pub use serve::ServeArgs;
#[cfg(feature = "themes")]
pub use theme::ThemeArgs;

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
    ///
    /// Checked here rather than where it is applied: the overlay is infallible
    /// by design, and the same value written in the config is refused by the
    /// same rule, so a preview deploy cannot smuggle in a base the config
    /// would have rejected.
    #[arg(long, help_heading = group::OUTPUT, value_parser = absolute_url)]
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
    /// Write the generated modules where an editor can resolve them.
    #[command(visible_aliases = ["packages", "pkg"])]
    Mirror(MirrorArgs),
    /// List the themes this binary ships, or write one into the project.
    #[cfg(feature = "themes")]
    #[command(visible_alias = "th")]
    Theme(ThemeArgs),
}

impl Cli {
    /// [`EXAMPLES`] rendered for the bottom of the top-level help. owo-colors
    /// gates the colour on the stdout stream itself (`if_supports_color`), so
    /// escapes never leak when piped or under `NO_COLOR`: the same policy
    /// [`crate::ui`] uses.
    fn examples() -> String {
        help::Examples {
            rows: EXAMPLES,
            footer: Some(&format!(
                "Run {} for command-specific options.",
                help::Literal("baudelaire <command> --help")
            )),
        }
        .to_string()
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
///   so `drafts #true` in config had no CLI route back to a production
///   build, and `serve { watch #false }` no way to turn watching on.
/// - One that accumulated with `|=` (`--external`), which is the same one-way
///   street with the reasoning buried in a code comment.
#[derive(Debug, Clone, Copy, Default)]
struct Toggle(Option<bool>);

impl Toggle {
    /// Resolve a pair. Each flag `overrides_with` the other, so clap lets the
    /// last one on the command line win and at most one arrives set.
    fn of(on: bool, off: bool) -> Self {
        Self(on.then_some(true).or_else(|| off.then_some(false)))
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

/// A `--base-url` that is an absolute base, by the same rule the config's own
/// `url` answers to.
fn absolute_url(value: &str) -> std::result::Result<String, String> {
    match crate::config::BaseUrl::absolute(value) {
        true => Ok(value.to_owned()),
        false => {
            Err("not an absolute URL: write the scheme too, e.g. `https://example.com`".into())
        }
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
            config.paths.dist.clone_from(out);
        }
        Toggle::of(self.cache, self.no_cache).apply(&mut config.cache.incremental);
    }
}

impl Overrides for CommonOverrides {
    fn apply(&self, config: &mut Config) {
        if let Some(url) = &self.base_url {
            config.url = Some(url.clone());
        }
        Toggle::of(self.drafts, self.no_drafts).apply(&mut config.content.drafts.build);
        Toggle::of(self.future, self.no_future).apply(&mut config.content.future);
        Toggle::of(self.strict_links, self.no_strict_links).apply(&mut config.links.strict);
    }
}

/// Run a parsed CLI: install the debug-log subscriber, dispatch, and flush any
/// collected warnings, on success and failure alike, so a failed run still
/// shows what it warned about before dying.
// The process entry point owns the parsed CLI for the whole run and drops it at
// the end; borrowing here would only push that ownership back into `main`.
#[allow(clippy::needless_pass_by_value)]
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
    // stderr. Not at all for a command whose own document owns stdout: see
    // [`Command::owns_stdout`].
    if cli.global.json && !cli.command.as_ref().is_some_and(Command::owns_stdout) {
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
    /// Whether this command's own document *is* the stdout payload.
    ///
    /// THE list of them, read by [`run`] to decide whether a `--json` report may
    /// be appended. `completions`, `man` and `reference` describe the CLI rather
    /// than a site: they read no config, build nothing, and write one document
    /// to stdout meant to be redirected into a completion directory, a `man1/`
    /// or a file. A summary object printed after it corrupts that file, and
    /// `baudelaire completions bash --json > _bd` produced a script the shell
    /// chokes on.
    ///
    /// Skipped rather than refused as a usage error, because `--json` is global:
    /// a wrapper that sets it once for every invocation would otherwise fail on
    /// the three commands that have no summary to report in the first place.
    /// The run still warns on stderr, and `--strict` still decides the exit
    /// code, so nothing is swallowed but the report.
    fn owns_stdout(&self) -> bool {
        matches!(
            self,
            Self::Completions(_) | Self::Man(_) | Self::Reference(_)
        )
    }

    /// Delegate to the selected subcommand's [`Run`] impl.
    fn run(&self, cx: &Cx) -> Result<()> {
        match self {
            Self::Build(args) => args.run(cx),
            Self::Serve(args) => args.run(cx),
            Self::Check(args) => args.run(cx),
            Self::New(args) => args.run(cx),
            #[cfg(feature = "announce")]
            Self::Announce(args) => args.run(cx),
            Self::Deploy(args) => args.run(cx),
            Self::Clean(args) => args.run(cx),
            Self::Init(args) => args.run(cx),
            Self::Completions(args) => args.run(cx),
            Self::Man(args) => args.run(cx),
            Self::Reference(args) => args.run(cx),
            Self::Mirror(args) => args.run(cx),
            #[cfg(feature = "themes")]
            Self::Theme(args) => args.run(cx),
        }
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
        assert!(config.content.drafts.build);
        assert!(!config.cache.incremental);
    }

    /// The flag answers to the same rule the config's `url` does: a preview
    /// deploy must not be able to set a base the config would have refused.
    #[test]
    fn a_base_url_override_must_be_absolute() {
        use clap::Parser;
        assert!(
            Cli::try_parse_from(["baudelaire", "build", "--base-url", "preview.example"]).is_err()
        );
        assert!(
            Cli::try_parse_from([
                "baudelaire",
                "build",
                "--base-url",
                "https://preview.example"
            ])
            .is_ok()
        );
    }

    /// The three commands whose document *is* the stdout payload keep it to
    /// themselves; everything else still reports.
    ///
    /// `baudelaire completions bash --json > _bd` used to write a completion
    /// script with a JSON object on the end of it, which a shell refuses to
    /// source. The same appended object would corrupt a man page and the
    /// config reference.
    #[test]
    fn a_document_command_keeps_stdout_to_itself() {
        use clap::Parser;
        let owns = |argv: &[&str]| {
            Cli::parse_from(argv.iter().copied())
                .command
                .expect("a command")
                .owns_stdout()
        };
        assert!(owns(&["baudelaire", "completions", "bash"]));
        assert!(owns(&["baudelaire", "man"]));
        assert!(owns(&["baudelaire", "reference"]));
        assert!(owns(&["baudelaire", "reference", "assets.images"]));

        assert!(!owns(&["baudelaire", "build"]));
        assert!(!owns(&["baudelaire", "check"]));
        assert!(!owns(&["baudelaire", "clean"]));
        assert!(!owns(&["baudelaire", "new", "posts/a"]));
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
