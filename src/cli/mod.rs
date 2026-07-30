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
use crate::error::warning::CleanRefused;
use crate::error::{BaudelaireErrorKind, ConfigError, FsError, Op, Result};
use crate::ui::{Level, Ui};

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
    version,
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
}

/// Config overrides that only make sense for a build (`build`, `serve`,
/// `check`). Flattened per-command rather than made global so scaffolding
/// commands don't advertise irrelevant `--drafts`/`--no-cache` flags.
#[derive(Args, Debug, Clone, Default)]
pub struct BuildOverrides {
    #[command(flatten)]
    pub common: CommonOverrides,

    /// Override the output directory.
    #[arg(short, long, help_heading = group::OUTPUT)]
    pub out: Option<PathBuf>,

    /// Skip the cache (full rebuild).
    #[arg(long, help_heading = group::BUILD)]
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

    /// Build draft pages.
    #[arg(long, help_heading = group::BUILD)]
    pub drafts: bool,

    /// Build future-dated pages.
    #[arg(long, help_heading = group::BUILD)]
    pub future: bool,

    /// Error on broken internal links (default true; pass `false` to warn).
    #[arg(long, num_args = 0..=1, default_missing_value = "true", help_heading = group::BUILD)]
    pub strict_links: Option<bool>,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Command {
    /// Build the site (default when no subcommand given).
    Build(BuildArgs),
    /// Serve the site with a dev server and live rebuild.
    Serve(ServeArgs),
    /// Compile and check links without writing output.
    Check(CheckArgs),
    /// Scaffold a new content file.
    New(NewArgs),
    /// Announce the site's metadata to the configured destination (atproto/standard.site).
    #[cfg(feature = "announce")]
    Announce(AnnounceArgs),
    /// Deploy the built files to the configured destination (S3/R2 or SSH).
    Deploy(DeployArgs),
    /// Remove build output and local build state.
    Clean(CleanArgs),
    /// Scaffold a new project (config.kdl + dirs).
    Init(InitArgs),
}

/// Arguments for `baudelaire build`.
#[derive(Args, Debug, Clone, Default)]
pub struct BuildArgs {
    #[command(flatten)]
    pub overrides: BuildOverrides,
}

/// Arguments for `baudelaire check`.
#[derive(Args, Debug, Clone)]
pub struct CheckArgs {
    #[command(flatten)]
    pub overrides: CommonOverrides,

    /// Also verify outbound `http(s)` links over the network.
    #[arg(long, help_heading = group::BUILD)]
    pub external: bool,
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

    /// Open browser on start.
    #[arg(long, num_args = 0..=1, default_missing_value = "true", help_heading = group::SERVER)]
    pub open: Option<bool>,

    /// Disable file watching and live rebuild.
    #[arg(long, help_heading = group::SERVER)]
    pub no_watch: bool,
}

/// Arguments for `baudelaire announce`.
#[cfg(feature = "announce")]
#[derive(Args, Debug, Clone)]
pub struct AnnounceArgs {
    /// Secret (app password / token) for the destination; `-` reads it from
    /// stdin. Prefer stdin, the environment variable, or the interactive prompt:
    /// a literal flag can leak into shell history.
    #[arg(long)]
    pub password: Option<String>,
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
        self.password.clone()
    }
}

/// Arguments for `baudelaire deploy`.
#[derive(Args, Debug, Clone)]
pub struct DeployArgs {
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
#[derive(Args, Debug, Clone)]
pub struct CleanArgs {
    /// Remove the build output directory.
    #[arg(long, help_heading = group::TARGETS)]
    pub dist: bool,
    /// Remove the incremental build cache.
    #[arg(long, help_heading = group::TARGETS)]
    pub cache: bool,
    /// Remove local announce state.
    #[arg(long, help_heading = group::TARGETS)]
    pub announce: bool,
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
        selected: |a| a.dist,
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
    /// No explicit target means sweep everything.
    fn all(&self) -> bool {
        CLEAN_TARGETS.iter().all(|t| !(t.selected)(self))
    }

    /// Remove this invocation's targets, skipping any that would take the
    /// project with them.
    fn sweep(&self, ui: &Ui, config: &Config, root: &Path) -> Result<()> {
        let mut removed = 0;
        for dir in self.targets(config) {
            if !dir.exists() {
                continue;
            }
            if !Self::removable(&dir, root) {
                ui.warn(CleanRefused { dir });
                continue;
            }
            ui.detail(format_args!("- {}", dir.display()));
            crate::fs::remove_dir_all(&dir)?;
            removed += 1;
        }
        match removed {
            0 => ui.done("nothing to clean"),
            _ => ui.done("clean"),
        }
        Ok(())
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

    /// Mark the page a draft (default: true; `--draft false` publishes it).
    #[arg(long, num_args = 0..=1, default_missing_value = "true", help_heading = group::CONTENT)]
    pub draft: Option<bool>,

    /// Create a page bundle (`<name>/index.typ`) for colocated assets.
    #[arg(short = 'b', long, help_heading = group::CONTENT)]
    pub bundle: bool,

    /// Open the new file in `$EDITOR` after creating it.
    #[arg(short = 'e', long, help_heading = group::CONTENT)]
    pub open: bool,
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

    /// Skip the prompt and set up version control (default: git).
    #[arg(short = 'y', long)]
    pub yes: bool,
    // The accepted values are not spelled out: `value_enum` already lists them
    // from [`scaffold::Vcs`] itself, so a new variant documents itself.
    /// Set up this version-control system (implies `--yes`).
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
        if self.no_cache {
            config.cache.incremental = false;
        }
    }
}

impl Overrides for CommonOverrides {
    fn apply(&self, config: &mut Config) {
        if let Some(url) = &self.base_url {
            config.url = Some(url.clone());
        }
        if self.drafts {
            config.content.draft.build = true;
        }
        if self.future {
            config.content.future = true;
        }
        if let Some(strict) = self.strict_links {
            config.links.strict = strict;
        }
    }
}

/// Run a parsed CLI: install the debug-log subscriber, dispatch, and flush any
/// collected warnings, on success and failure alike, so a failed run still
/// shows what it warned about before dying.
pub fn run(cli: Cli) -> Result<()> {
    crate::ui::trace::init(cli.global.verbose);
    let ui = Ui::new(cli.level());
    let result = dispatch(&cli, &ui);
    ui.flush();
    result
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
        }
    }
}

impl Run for BuildArgs {
    fn run(&self, cx: &Cx) -> Result<()> {
        let config = cx.configured(&self.overrides, "building")?;
        crate::engine::Engine::new(config, crate::engine::Mode::Build)?.build(cx.ui)?;
        Ok(())
    }
}

impl Run for CheckArgs {
    fn run(&self, cx: &Cx) -> Result<()> {
        let mut config = cx.configured(&self.overrides, "checking")?;
        // The flag turns the config switch on for this run; it never turns a
        // configured one off, so `links { external #true }` in CI needs no flag.
        config.links.external |= self.external;
        crate::engine::Engine::new(config, crate::engine::Mode::Check)?.check(cx.ui)?;
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
        // ordered collection, and permalink collisions.
        let project = crate::world::Project::new(&config, crate::engine::Mode::Build)?;
        scaffold::Draft::plan(self, &config, &project)?.create(cx.ui)
    }
}

#[cfg(feature = "announce")]
impl Run for AnnounceArgs {
    fn run(&self, cx: &Cx) -> Result<()> {
        let config = cx.announced("announcing")?;
        remote::run(cx.ui, &config, self, crate::announce::run)
    }
}

impl Run for DeployArgs {
    fn run(&self, cx: &Cx) -> Result<()> {
        let config = cx.announced("deploying")?;
        remote::run(cx.ui, &config, self, crate::deploy::run)
    }
}

impl Run for CleanArgs {
    fn run(&self, cx: &Cx) -> Result<()> {
        let config = cx.announced("cleaning")?;
        self.sweep(cx.ui, &config, cx.root.path())
    }
}

impl Run for InitArgs {
    fn run(&self, cx: &Cx) -> Result<()> {
        cx.ui.banner("init");
        scaffold::init(cx.ui, cx.root, self)
    }
}

impl NewArgs {
    /// The file `baudelaire new` should create: a relative path lands under
    /// the content directory (unless it already starts with it, so an explicit
    /// `content/posts/foo.typ` is not double-prefixed), and `.typ` is appended
    /// when the name does not already carry it.
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
        if let Some(open) = self.open {
            config.serve.open = open;
        }
        if self.no_watch {
            config.serve.watch = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(dist: bool, cache: bool, announce: bool) -> CleanArgs {
        CleanArgs {
            dist,
            cache,
            announce,
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
