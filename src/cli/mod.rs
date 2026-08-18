//! Command-line interface: per-subcommand args, dispatch, and the wiring of
//! terminal output ([`crate::ui`]) and debug logging (`tracing`).

#[cfg(feature = "announce")]
pub mod announce;
pub mod build;
pub mod check;
pub mod clean;
pub mod completions;
pub mod config;
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
use crate::version::Version;

#[cfg(feature = "announce")]
pub use announce::AnnounceArgs;
pub use build::BuildArgs;
pub use check::CheckArgs;
pub use clean::CleanArgs;
pub use completions::{CompletionsArgs, Shell};
pub use config::ConfigArgs;
pub use deploy::DeployArgs;
pub use init::InitArgs;
pub use man::ManArgs;
pub use mirror::MirrorArgs;
pub use new::NewArgs;
pub use reference::ReferenceArgs;
pub use remote::PublishArgs;
pub use serve::ServeArgs;
#[cfg(feature = "themes")]
pub use theme::ThemeArgs;

/// Help colouring, matched to the terminal UI palette.
const HELP_STYLES: Styles = Styles::styled()
    .header(AnsiColor::Cyan.on_default().bold())
    .usage(AnsiColor::Cyan.on_default().bold())
    .literal(AnsiColor::Green.on_default().bold())
    .placeholder(AnsiColor::White.on_default().dimmed())
    .valid(AnsiColor::Green.on_default())
    .invalid(AnsiColor::Yellow.on_default())
    .error(AnsiColor::Red.on_default().bold());

/// Help-heading names, referenced by every grouped `#[arg(help_heading = ..)]`.
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
/// does)`.
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

/// What the process exits with, as `(code, when)`.
const EXIT_CODES: &[(&str, &str)] = &[
    ("0", "Everything asked for was done"),
    (
        "1",
        "The run failed, or --strict was passed and something warned",
    ),
];

/// The environment a run reads, as `(name, what it decides)`; a flag always
/// beats the variable beside it.
const ENVIRONMENT: &[(&str, &str)] = &[
    ("RUST_LOG", "Debug-log filter, for a run that passed no -v"),
    ("NO_COLOR", "Set to anything, colour is off"),
    ("CLICOLOR_FORCE", "Set to anything but 0, colour is on"),
    ("VISUAL, EDITOR", "What new --edit opens the page in"),
    ("${VAR}", "Expanded in every config.kdl string value"),
];

/// The absolute project root: the directory `--root` selected, or the launch
/// directory.
pub(crate) struct Root(PathBuf);

impl Root {
    /// Enter and capture the project root, changing the process cwd so that
    /// every relative path in the config resolves under it.
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
    version = Version::SEMVER,
    long_version = Version::long(),
    about,
    long_about = "Baudelaire compiles a Typst content tree into a static site: incremental \
                  builds, a live-reload dev server, feeds, search, taxonomies, and more, all \
                  driven by Typst templates rather than HTML string templating.",
    styles = HELP_STYLES,
    after_help = Cli::help(),
    subcommand_value_name = "COMMAND",
)]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalArgs,

    #[command(subcommand)]
    pub command: Option<Command>,
}

/// Arguments shared across *every* subcommand: project location and logging.
#[derive(Args, Debug, Clone)]
pub struct GlobalArgs {
    /// Path to config.kdl.
    #[arg(short, long, global = true, default_value = Config::FILE, help_heading = group::PROJECT)]
    pub config: PathBuf,

    /// Project root directory.
    #[arg(short, long, global = true, help_heading = group::PROJECT)]
    pub root: Option<PathBuf>,

    /// Named profile to apply (e.g. `dev`, `prod`).
    #[arg(short, long, global = true, help_heading = group::PROJECT)]
    pub profile: Option<String>,

    /// Build with this theme, whatever `theme` in the config says.
    ///
    /// Here beside `--profile` and not in [`CommonOverrides`] because it does
    /// not edit a config that has been read: a theme supplies the `theme.kdl`
    /// floor that the site's own keys are layered over, so it has to be known
    /// during the load rather than applied to its result. The same reason a
    /// theme cannot be named in a `profiles { }` entry.
    #[arg(long, global = true, value_name = "THEME", help_heading = group::PROJECT)]
    pub theme: Option<String>,

    /// Verbose output: per-page progress plus debug logs (-vv for trace logs).
    #[arg(short, long, global = true, action = clap::ArgAction::Count, help_heading = group::LOGGING)]
    pub verbose: u8,

    /// Quiet output: warnings and the final result (-qq drops the result too).
    ///
    /// Counted, like `-v`, and for the same reason: one level was not enough to
    /// separate "do not narrate the build" from "say nothing unless something
    /// is wrong". `-qq` leaves diagnostics and the exit code, which is what a
    /// cron job wants and what `-q` alone could not express.
    #[arg(short, long, global = true, action = clap::ArgAction::Count, conflicts_with = "verbose", help_heading = group::LOGGING)]
    pub quiet: u8,

    /// Fail the run if anything warned.
    #[arg(long, global = true, help_heading = group::LOGGING)]
    pub strict: bool,

    /// Write a machine-readable summary of the run to stdout.
    #[arg(long, global = true, help_heading = group::LOGGING)]
    pub json: bool,

    /// When to colour output.
    ///
    /// Layers over the automatic detection rather than replacing it: `auto`
    /// leaves `NO_COLOR`, `CLICOLOR_FORCE` and the terminal check to decide,
    /// and naming `always` or `never` overrules all three.
    #[arg(long, global = true, value_name = "WHEN", value_enum, default_value = "auto", help_heading = group::LOGGING)]
    pub color: Color,
}

/// Config overrides that only make sense for a command that builds: `build`,
/// `serve`, and the publishing commands that build first.
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
/// ones that write nothing.
#[derive(Args, Debug, Clone, Default)]
pub struct CommonOverrides {
    /// Override the base URL.
    ///
    /// Checked here rather than where it is applied: the overlay is infallible
    /// by design, and the same value written in the config is refused by the
    /// same rule, so a preview deploy cannot smuggle in a base the config
    /// would have rejected.
    #[arg(long, help_heading = group::OUTPUT, value_parser = CommonOverrides::absolute)]
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

    /// Error on a `.typ` link to a missing page or heading (default;
    /// `--no-strict-links` warns instead).
    ///
    /// Says `.typ` because that is the whole of it: a link the build can judge
    /// is one naming a page this site renders. An `http(s)` URL is checked only
    /// by `check --external`, and a link to a static file is never checked at
    /// all, so "broken internal links" promised two things this flag does not
    /// decide.
    #[arg(long, overrides_with = "no_strict_links", help_heading = group::BUILD)]
    pub strict_links: bool,
    #[arg(long, overrides_with = "strict_links", hide = true)]
    pub no_strict_links: bool,
}

/// The subcommands, each with a visible short alias.
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
    /// Validate a config without building it.
    #[command(visible_alias = "cfg")]
    Config(ConfigArgs),
    /// Write the generated modules where an editor can resolve them.
    #[command(visible_aliases = ["packages", "pkg"])]
    Mirror(MirrorArgs),
    /// List the themes this binary ships, or write one into the project.
    #[cfg(feature = "themes")]
    #[command(visible_alias = "th")]
    Theme(ThemeArgs),
}

impl Cli {
    /// The blocks under the top-level help: [`EXAMPLES`], [`EXIT_CODES`] and
    /// [`ENVIRONMENT`], through the one help-block layout.
    fn help() -> String {
        format!(
            "{}\n{}\n{}",
            help::Table::examples(EXAMPLES),
            help::Table::codes(EXIT_CODES),
            help::Table::environment(ENVIRONMENT).footer(format!(
                "Run {} for command-specific options.",
                help::Literal("baudelaire <command> --help")
            )),
        )
    }

    /// The parsed config: read from `--config`, then narrowed by the active
    /// profile. Build-time overrides ([`BuildOverrides`]) are the caller's to
    /// apply per-command, not this.
    pub fn config(&self) -> Result<Config> {
        let text = self.read()?;
        let named = |e: BaudelaireErrorKind| match e {
            BaudelaireErrorKind::Config(config) => {
                BaudelaireErrorKind::Config(Box::new(config.named(&self.global.config)))
            }
            other => other,
        };
        let mut config =
            Config::load(&text, Path::new("."), self.global.theme.as_deref()).map_err(named)?;
        if let Some(profile) = &self.global.profile {
            config = config.with_profile(profile).map_err(named)?;
        }
        Ok(config)
    }

    /// Read the config file, mapping only a genuinely missing file to a
    /// [`ConfigError::not_found`]; every other failure keeps its [`FsError`].
    fn read(&self) -> Result<String> {
        let path = &self.global.config;
        crate::fs::read_to_string(path).map_err(|e| match e {
            BaudelaireErrorKind::Fs(fs) if fs.kind() == std::io::ErrorKind::NotFound => {
                ConfigError::not_found(&path.display().to_string()).into()
            }
            other => other,
        })
    }

    /// The UI verbosity, from the two counted flags.
    ///
    /// `-vv` and beyond only deepen the `tracing` filter (see
    /// [`crate::ui::trace`]); the terminal report itself has one verbose level.
    fn level(&self) -> Level {
        let g = &self.global;
        match (g.quiet, g.verbose) {
            (0, 0) => Level::Default,
            (0, _) => Level::Verbose,
            (1, _) => Level::Quiet,
            (_, _) => Level::Silent,
        }
    }
}

/// What `--color` says about styled output.
#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Color {
    /// Colour when the stream is a terminal that wants it.
    #[default]
    Auto,
    Always,
    Never,
}

impl Color {
    /// Install this choice for the process, in both `anstream` and owo-colors:
    /// the two detect independently, so one flag would otherwise colour half
    /// the output.
    fn install(self) {
        use anstream::ColorChoice;
        match self {
            Self::Auto => {
                ColorChoice::Auto.write_global();
                owo_colors::unset_override();
            }
            Self::Always => {
                ColorChoice::Always.write_global();
                owo_colors::set_override(true);
            }
            Self::Never => {
                ColorChoice::Never.write_global();
                owo_colors::set_override(false);
            }
        }
    }

    /// The `--color` value on a raw command line, before clap has parsed it.
    ///
    /// A value it cannot read is `None`, leaving clap to report the usage
    /// error; the last spelling wins, as it does for every other flag.
    fn declared<'a>(args: impl IntoIterator<Item = &'a str>) -> Option<Self> {
        let mut args = args.into_iter();
        let mut chosen = None;
        while let Some(arg) = args.next() {
            let Some(tail) = arg.strip_prefix("--color") else {
                continue;
            };
            let value = if tail.is_empty() {
                args.next()
            } else {
                tail.strip_prefix('=')
            };
            if let Some(color) = value.and_then(Self::named) {
                chosen = Some(color);
            }
        }
        chosen
    }

    /// The variant `value` names, off clap's own value table, so the pre-scan
    /// and the parse cannot disagree.
    fn named(value: &str) -> Option<Self> {
        use clap::ValueEnum as _;
        Self::from_str(value, true).ok()
    }
}

/// What a `--x` / `--no-x` flag pair says about a boolean setting: turn it on,
/// turn it off, or (`None`) leave whatever the config decided.
#[derive(Debug, Clone, Copy, Default)]
struct Toggle(Option<bool>);

impl Toggle {
    /// Resolve a pair. Each flag `overrides_with` the other, so clap lets the
    /// last one on the command line win and at most one arrives set.
    fn of(on: bool, off: bool) -> Self {
        Self(on.then_some(true).or_else(|| off.then_some(false)))
    }

    /// Overlay onto a config field, leaving it untouched when neither flag was
    /// passed.
    fn apply(self, target: &mut bool) {
        if let Some(value) = self.0 {
            *target = value;
        }
    }

    /// The value, or `default` when neither flag was passed, for a toggle with
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
            config.paths.dist.clone_from(out);
        }
        Toggle::of(self.cache, self.no_cache).apply(&mut config.cache.incremental);
    }
}

impl CommonOverrides {
    /// The value parser for every flag that takes a site base, `init --url`
    /// included: a URL that is absolute, by the same rule the config's own
    /// `url` answers to.
    pub(super) fn absolute(value: &str) -> std::result::Result<String, String> {
        if crate::config::BaseUrl::absolute(value) {
            Ok(value.to_owned())
        } else {
            Err("not an absolute URL: write the scheme too, e.g. `https://example.com`".into())
        }
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

impl Cli {
    /// Parse the command line with the colour choice already in force, in place
    /// of `Cli::parse`: clap writes `--help` during the parse, so a `--color`
    /// installed afterwards would miss it.
    #[must_use]
    pub fn parsed() -> Self {
        let args: Vec<String> = std::env::args().collect();
        Color::declared(args.iter().map(String::as_str))
            .unwrap_or_default()
            .install();
        Self::parse()
    }

    /// Run this parsed CLI: install the debug-log subscriber, dispatch, and
    /// flush any collected warnings, on success and failure alike.
    ///
    /// Installs the colour choice again, for a `Cli` built without
    /// [`Cli::parsed`]; the call is idempotent.
    // The entry point owns the parsed CLI for the whole run.
    #[allow(clippy::needless_pass_by_value)]
    pub fn run(self) -> Result<()> {
        self.global.color.install();
        crate::ui::trace::Logs::new(self.global.verbose).install();
        let ui = Ui::new(self.level());
        let result = self.dispatch(&ui);
        let warned = ui.warnings();
        let outcome = result.and_then(|()| {
            if self.global.strict && warned > 0 {
                Err(StrictWarnings { count: warned }.into())
            } else {
                Ok(())
            }
        });
        if self.global.json && !self.command.as_ref().is_some_and(Command::owns_stdout) {
            if let Err(error) = &outcome {
                ui.failed(error);
            }
            ui.summary(outcome.is_ok()).emit();
        }
        ui.flush();
        outcome
    }

    /// Dispatch to the matching subcommand, defaulting to `build`.
    fn dispatch(&self, ui: &Ui) -> Result<()> {
        let root = Root::enter(self.global.root.as_deref())?;
        let command = self
            .command
            .clone()
            .unwrap_or_else(|| Command::Build(BuildArgs::default()));
        command.run(&Cx {
            cli: self,
            ui,
            root: &root,
        })
    }
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
        config.root = self.root.path().to_path_buf();
        self.ui.banner(format_args!("{verb} {}", config.label()));
        Ok(config)
    }

    /// [`Cx::announced`] plus the build-shaping overrides: the front matter of
    /// the build-shaped commands.
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
    /// Whether this command's own document *is* the stdout payload, in which
    /// case a `--json` summary is skipped rather than appended: it would
    /// corrupt a redirected completion script, man page or reference.
    fn owns_stdout(&self) -> bool {
        matches!(
            self,
            Self::Completions(_) | Self::Man(_) | Self::Reference(_)
        ) || matches!(
            self,
            Self::Config(args) if args.owns_stdout()
        )
    }

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
            Self::Config(args) => args.run(cx),
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
        assert!(CleanArgs::removable(Path::new("/srv/www"), root));
    }

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

    #[test]
    fn the_scaffolded_url_answers_to_the_same_rule() {
        use clap::Parser;
        assert!(Cli::try_parse_from(["baudelaire", "init", "--url", "example.com"]).is_err());
        assert!(
            Cli::try_parse_from(["baudelaire", "init", "--url", "https://example.com"]).is_ok()
        );
    }

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

    #[test]
    fn the_two_counted_flags_name_four_levels() {
        use clap::Parser;
        let level = |argv: &[&str]| Cli::parse_from(argv.iter().copied()).level();
        assert_eq!(level(&["baudelaire", "build"]), Level::Default);
        assert_eq!(level(&["baudelaire", "-v", "build"]), Level::Verbose);
        assert_eq!(level(&["baudelaire", "-vv", "build"]), Level::Verbose);
        assert_eq!(level(&["baudelaire", "-q", "build"]), Level::Quiet);
        assert_eq!(level(&["baudelaire", "-qq", "build"]), Level::Silent);
        assert_eq!(level(&["baudelaire", "-qqq", "build"]), Level::Silent);
        assert!(Cli::try_parse_from(["baudelaire", "-q", "-v", "build"]).is_err());
    }

    #[test]
    fn a_colour_choice_is_read_before_clap_sees_it() {
        assert_eq!(Color::declared(["baudelaire", "build"]), None);
        assert_eq!(
            Color::declared(["baudelaire", "--color=never", "build"]),
            Some(Color::Never)
        );
        assert_eq!(
            Color::declared(["baudelaire", "--color", "always", "build"]),
            Some(Color::Always)
        );
        assert_eq!(
            Color::declared(["baudelaire", "--color=always", "--color=never"]),
            Some(Color::Never)
        );
        assert_eq!(Color::declared(["baudelaire", "--color=maybe"]), None);
        assert_eq!(Color::declared(["baudelaire", "--colorful"]), None);
    }

    /// Installs process-global state; the suite runs one process per test
    /// (nextest).
    #[test]
    fn an_explicit_choice_overrules_the_environment() {
        use anstream::ColorChoice;
        Color::Never.install();
        assert_eq!(ColorChoice::global(), ColorChoice::Never);
        Color::Always.install();
        assert_eq!(ColorChoice::global(), ColorChoice::Always);
        Color::Auto.install();
        assert_eq!(ColorChoice::global(), ColorChoice::Auto);
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
            vec![Config::scratch(crate::config::Scratch::Announce)]
        );
        assert_eq!(
            args(true, false, false).targets(&config),
            vec![config.paths.dist.clone()]
        );
    }
}
