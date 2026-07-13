//! Command-line interface: per-subcommand args, dispatch, and the wiring of
//! terminal output ([`crate::ui`]) and debug logging (`tracing`).

pub mod prompt;
pub mod publish;
pub mod scaffold;
pub mod serve;

use std::path::{Path, PathBuf};

use clap::builder::styling::{AnsiColor, Styles};
use clap::{Args, Parser, Subcommand};

use crate::config::Config;
use crate::error::{FsError, Op, Result};
use crate::ui::{Level, Ui};

/// Help colouring, matched to the terminal UI palette: cyan for structure
/// (section headers, usage), green for the literals you type (commands and
/// flags), and dimmed `<VALUE>` placeholders — so a glance separates the words
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
/// grouped `#[arg(help_heading = …)]`.
mod group {
    pub const PROJECT: &str = "Project";
    pub const OUTPUT: &str = "Output";
    pub const BUILD: &str = "Build";
    pub const LOGGING: &str = "Logging";
    pub const SERVER: &str = "Server";
    pub const TARGETS: &str = "Targets";
}

/// Usage examples appended to the top-level help. owo-colors gates the colour
/// on the stdout stream itself (`if_supports_color`), so escapes never leak when
/// piped or under `NO_COLOR` — the same policy [`crate::ui`] uses.
fn examples() -> String {
    use owo_colors::{OwoColorize, Stream::Stdout};
    use std::fmt::Write;
    // One example row: the command in green (the "literal you type" accent), then
    // its description at a fixed column. Padding is computed from the *visible*
    // length, so the ANSI escapes don't skew the alignment.
    let row = |out: &mut String, command: &str, desc: &str| {
        let colored = command.if_supports_color(Stdout, |t| t.green().bold().to_string());
        // Column wide enough for the longest command, with a two-space gutter.
        let pad = " ".repeat(33usize.saturating_sub(command.len()).max(2));
        let _ = writeln!(out, "  {colored}{pad}{desc}");
    };
    let mut s = format!(
        "{}\n",
        "Examples:".if_supports_color(Stdout, |t| t.cyan().bold().to_string())
    );
    row(&mut s, "baudelaire", "Build the site from ./config.kdl");
    row(&mut s, "baudelaire serve --open", "Start the dev server, open a browser");
    row(&mut s, "baudelaire new posts/hello", "Scaffold content/posts/hello.typ");
    row(&mut s, "baudelaire --profile prod build", "Build with the prod profile");
    row(&mut s, "baudelaire clean --cache", "Drop the incremental cache");
    let _ = write!(
        s,
        "\nRun {} for command-specific options.",
        "baudelaire <command> --help".if_supports_color(Stdout, |t| t.green().bold().to_string())
    );
    s
}

/// The absolute project root: the directory `--root` selected (into which the
/// process changes so relative config paths resolve under it) or the launch
/// directory. Captured once and threaded, so nothing re-derives the root from
/// the process cwd.
pub(crate) struct Root(PathBuf);

impl Root {
    /// Enter and capture the project root. A `--root` argument changes the
    /// process cwd — the single side effect that makes every relative path in
    /// the config resolve under the chosen directory — then the absolute root
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

/// Baudelaire — a Typst-native static site generator.
#[derive(Parser, Debug)]
#[command(
    name = "baudelaire",
    version,
    about,
    long_about = "Baudelaire compiles a Typst content tree into a static site — incremental \
                  builds, a live-reload dev server, feeds, search, taxonomies, and more, all \
                  driven by Typst templates rather than HTML string templating.",
    styles = HELP_STYLES,
    after_help = examples(),
    subcommand_value_name = "COMMAND",
)]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalArgs,

    #[command(subcommand)]
    pub command: Option<Command>,
}

/// Arguments shared across all subcommands.
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

    /// Override the output directory.
    #[arg(short, long, global = true, help_heading = group::OUTPUT)]
    pub out: Option<PathBuf>,

    /// Override the base URL.
    #[arg(long, global = true, help_heading = group::OUTPUT)]
    pub base_url: Option<String>,

    /// Build draft pages.
    #[arg(long, global = true, help_heading = group::BUILD)]
    pub drafts: bool,

    /// Build future-dated pages.
    #[arg(long, global = true, help_heading = group::BUILD)]
    pub future: bool,

    /// Skip the cache (full rebuild).
    #[arg(long, global = true, help_heading = group::BUILD)]
    pub no_cache: bool,

    /// Error on broken internal links (default true; pass `false` to warn).
    #[arg(long, global = true, num_args = 0..=1, default_missing_value = "true", help_heading = group::BUILD)]
    pub strict_links: Option<bool>,

    /// Verbose output: per-page progress plus debug logs (-vv for trace logs).
    #[arg(short, long, global = true, action = clap::ArgAction::Count, help_heading = group::LOGGING)]
    pub verbose: u8,

    /// Quiet output.
    #[arg(short, long, global = true, conflicts_with = "verbose", help_heading = group::LOGGING)]
    pub quiet: bool,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Command {
    /// Build the site (default when no subcommand given).
    Build(BuildArgs),
    /// Serve the site with a dev server and live rebuild.
    Serve(ServeArgs),
    /// Compile and check links without writing output.
    Check,
    /// Scaffold a new content file.
    New(NewArgs),
    /// Publish the built site to every configured destination.
    Publish(PublishArgs),
    /// Remove build output and local build state.
    Clean(CleanArgs),
    /// Scaffold a new project (config.kdl + dirs).
    Init(InitArgs),
}

/// Arguments for `baudelaire build`.
#[derive(Args, Debug, Clone)]
pub struct BuildArgs {}

/// Arguments for `baudelaire serve`.
#[derive(Args, Debug, Clone)]
pub struct ServeArgs {
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

/// Arguments for `baudelaire publish`.
#[derive(Args, Debug, Clone)]
pub struct PublishArgs {
    /// Secret (app password / token) for the destination; `-` reads it from
    /// stdin. Prefer stdin, the environment variable, or the interactive prompt —
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

/// Arguments for `baudelaire clean`. With no target flag every directory is
/// swept; naming targets narrows it to those, so `clean --cache` forces a
/// rebuild without discarding publish state.
#[derive(Args, Debug, Clone)]
pub struct CleanArgs {
    /// Remove the build output directory.
    #[arg(long, help_heading = group::TARGETS)]
    pub dist: bool,
    /// Remove the incremental build cache.
    #[arg(long, help_heading = group::TARGETS)]
    pub cache: bool,
    /// Remove local publish state.
    #[arg(long, help_heading = group::TARGETS)]
    pub publish: bool,
}

impl CleanArgs {
    /// No explicit target means sweep everything.
    fn all(&self) -> bool {
        !(self.dist || self.cache || self.publish)
    }

    /// The directories to remove for this invocation. A full sweep clears the
    /// output plus the whole scratch root in one step (covering the cache,
    /// publish state, and any future intermediate); a relocated cache dir lives
    /// outside that root, so it is named explicitly. A narrowed sweep removes
    /// only the subdirectories asked for.
    fn targets(&self, config: &Config) -> Vec<PathBuf> {
        if self.all() {
            let mut dirs = vec![config.dist.clone(), PathBuf::from(Config::SCRATCH)];
            if !config.cache.dir.starts_with(Config::SCRATCH) {
                dirs.push(config.cache.dir.clone());
            }
            return dirs;
        }
        let mut dirs = Vec::new();
        if self.dist {
            dirs.push(config.dist.clone());
        }
        if self.cache {
            dirs.push(config.cache.dir.clone());
        }
        if self.publish {
            dirs.push(Config::scratch("publish"));
        }
        dirs
    }
}

/// Arguments for `baudelaire new`.
#[derive(Args, Debug, Clone)]
pub struct NewArgs {
    /// Path for the new content file (e.g. `content/posts/my-post.typ`).
    pub path: PathBuf,
}

/// Arguments for `baudelaire init`.
#[derive(Args, Debug, Clone)]
pub struct InitArgs {
    /// Directory to scaffold into (default: current directory).
    pub dir: Option<PathBuf>,
    /// Skip the prompt and set up version control (default: git).
    #[arg(short = 'y', long)]
    pub yes: bool,
    /// Set up this version-control system (implies `--yes`): `git` or `jujutsu`.
    #[arg(long, value_enum)]
    pub vcs: Option<scaffold::Vcs>,
}

impl Cli {
    /// Load config from the configured path, apply profile + CLI overrides.
    pub fn load_config(&self) -> Result<Config> {
        let g = &self.global;
        // Only a genuinely missing file is a "config not found"; anything else
        // (permission denied, a directory, invalid UTF-8) keeps its precise
        // filesystem diagnostic instead of being misreported.
        let text = crate::fs::read_to_string(&g.config).map_err(|e| match e {
            crate::error::BaudelaireErrorKind::Fs(fs)
                if fs.kind() == std::io::ErrorKind::NotFound =>
            {
                crate::error::ConfigError::not_found(&g.config.display().to_string()).into()
            }
            other => other,
        })?;
        let mut config = Config::parse(&text)?;
        if let Some(profile) = &g.profile {
            config = config.with_profile(profile)?;
        }
        g.apply_overrides(&mut config);
        Ok(config)
    }

    /// The UI verbosity. `-vv` and beyond only deepen the `tracing` filter
    /// (see [`crate::ui::trace`]) — the terminal report itself has one
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

impl GlobalArgs {
    fn apply_overrides(&self, config: &mut Config) {
        if let Some(out) = &self.out {
            config.dist = out.clone();
        }
        if let Some(url) = &self.base_url {
            config.url = Some(url.clone());
        }
        if self.drafts {
            config.draft.build = true;
        }
        if self.future {
            config.future = true;
        }
        if let Some(strict) = self.strict_links {
            config.links.strict = strict;
        }
        if self.no_cache {
            config.cache.incremental = false;
        }
    }
}

/// Run a parsed CLI: install the debug-log subscriber, dispatch, and flush any
/// collected warnings — on success and failure alike, so a failed run still
/// shows what it warned about before dying.
pub fn run(cli: Cli) -> Result<()> {
    crate::ui::trace::init(cli.global.verbose);
    let ui = Ui::new(cli.level());
    let result = dispatch(&cli, &ui);
    ui.flush();
    result
}

/// Dispatch to the matching engine entrypoint.
fn dispatch(cli: &Cli, ui: &Ui) -> Result<()> {
    let root = Root::enter(cli.global.root.as_deref())?;
    let command = cli.command.clone().unwrap_or(Command::Build(BuildArgs {}));
    match command {
        Command::Build(_) => {
            let config = cli.load_config()?;
            ui.banner(format_args!("building {}", config.label()));
            crate::engine::Engine::new(config, crate::engine::Mode::Build)?.build(ui)?;
        }
        Command::Check => {
            let config = cli.load_config()?;
            ui.banner(format_args!("checking {}", config.label()));
            crate::engine::Engine::new(config, crate::engine::Mode::Check)?.check(ui)?;
        }
        Command::Serve(args) => {
            let mut config = cli.load_config()?;
            args.apply(&mut config);
            ui.banner(format_args!("dev · {}", config.label()));
            // Re-reads config.kdl with the same profile + overrides, so the dev
            // server picks up config edits live.
            let reload = || -> Result<Config> {
                let mut config = cli.load_config()?;
                args.apply(&mut config);
                Ok(config)
            };
            crate::cli::serve::run(ui, config, &root, cli.global.config.clone(), reload)?;
        }
        Command::New(args) => {
            let config = cli.load_config()?;
            scaffold::new_page(ui, &args.target(&config), &config)?;
        }
        Command::Publish(args) => {
            let config = cli.load_config()?;
            ui.banner(format_args!("publishing {}", config.label()));
            publish::run(ui, &config, &args)?;
        }
        Command::Clean(args) => {
            let config = cli.load_config()?;
            ui.banner(format_args!("cleaning {}", config.label()));
            clean(ui, &config, &args)?;
        }
        Command::Init(args) => {
            ui.banner("init");
            // A positional directory scaffolds there (`init my-site`); with none,
            // the current directory (already `--root`-adjusted) is used.
            let target = args.dir.clone().unwrap_or_else(|| PathBuf::from("."));
            scaffold::init(ui, &target, &root, args.yes, args.vcs)?;
        }
    }
    Ok(())
}

impl NewArgs {
    /// The file `baudelaire new` should create: a relative path lands under
    /// the content directory (unless it already starts with it, so an explicit
    /// `content/posts/foo.typ` is not double-prefixed), and `.typ` is appended
    /// when the name does not already carry it.
    fn target(&self, config: &Config) -> PathBuf {
        let mut path = if self.path.is_absolute() || self.path.starts_with(&config.content) {
            self.path.clone()
        } else {
            config.content.join(&self.path)
        };
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

fn clean(ui: &Ui, config: &Config, args: &CleanArgs) -> Result<()> {
    let mut removed = 0;
    for dir in args.targets(config) {
        if dir.exists() {
            ui.detail(format_args!("- {}", dir.display()));
            crate::fs::remove_dir_all(&dir)?;
            removed += 1;
        }
    }
    match removed {
        0 => ui.done("nothing to clean"),
        _ => ui.done("clean"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(dist: bool, cache: bool, publish: bool) -> CleanArgs {
        CleanArgs {
            dist,
            cache,
            publish,
        }
    }

    #[test]
    fn full_sweep_clears_output_and_scratch_root() {
        let config = Config::default();
        let targets = args(false, false, false).targets(&config);
        assert!(targets.contains(&config.dist));
        assert!(targets.contains(&PathBuf::from(Config::SCRATCH)));
        // The default cache lives under the scratch root, so it is not named
        // separately — the root sweep already covers it.
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
            vec![Config::scratch("publish")]
        );
        assert_eq!(
            args(true, false, false).targets(&config),
            vec![config.dist.clone()]
        );
    }
}
