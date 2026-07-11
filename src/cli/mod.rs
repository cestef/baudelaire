//! Command-line interface: per-subcommand args, colored output, dispatch.

pub mod output;
pub mod prompt;
pub mod scaffold;
pub mod serve;

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::cli::output::{Level, Report};
use crate::config::Config;
use crate::error::Result;

/// Baudelaire - a Typst-native static site generator.
#[derive(Parser, Debug)]
#[command(name = "baudelaire", version, about)]
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
    #[arg(short, long, global = true, default_value = "config.kdl")]
    pub config: PathBuf,

    /// Project root directory.
    #[arg(short, long, global = true)]
    pub root: Option<PathBuf>,

    /// Named profile to apply (e.g. `dev`, `prod`).
    #[arg(short, long, global = true)]
    pub profile: Option<String>,

    /// Override the output directory.
    #[arg(short, long, global = true)]
    pub out: Option<PathBuf>,

    /// Override the base URL.
    #[arg(long, global = true)]
    pub base_url: Option<String>,

    /// Build draft pages.
    #[arg(long, global = true)]
    pub drafts: bool,

    /// Build future-dated pages.
    #[arg(long, global = true)]
    pub future: bool,

    /// Skip the cache (full rebuild).
    #[arg(long, global = true)]
    pub no_cache: bool,

    /// Error on broken internal links (default true; pass `false` to warn).
    #[arg(long, global = true, num_args = 0..=1, default_missing_value = "true")]
    pub strict_links: Option<bool>,

    /// Verbose output (-v info, -vv trace).
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Quiet output.
    #[arg(short, long, global = true, conflicts_with = "verbose")]
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
    /// Remove the dist and cache directories.
    Clean,
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
    #[arg(long)]
    pub port: Option<u16>,

    /// Address to bind (overrides config).
    #[arg(long)]
    pub bind: Option<String>,

    /// Open browser on start.
    #[arg(long, num_args = 0..=1, default_missing_value = "true")]
    pub open: Option<bool>,

    /// Disable file watching and live rebuild.
    #[arg(long)]
    pub no_watch: bool,
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

    fn level(&self) -> Level {
        let g = &self.global;
        if g.quiet {
            Level::Quiet
        } else {
            match g.verbose {
                0 => Level::Default,
                1 => Level::Verbose,
                _ => Level::Trace,
            }
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

    /// Change into `--root` if given, so config and every relative path resolve
    /// under it. Applied once before dispatch.
    fn enter(&self) -> Result<()> {
        if let Some(root) = &self.root {
            std::env::set_current_dir(root)
                .map_err(|e| crate::error::FsError::new(crate::error::Op::Enter, root, e))?;
        }
        Ok(())
    }
}

/// Dispatch a parsed CLI to the matching engine entrypoint.
pub fn run(cli: Cli) -> Result<()> {
    let mut report = Report::with_level(cli.level());
    cli.global.enter()?;
    let command = cli.command.clone().unwrap_or(Command::Build(BuildArgs {}));
    match command {
        Command::Build(_) => {
            let config = cli.load_config()?;
            crate::engine::Engine::new(config, crate::engine::Mode::Build)?.build(&mut report)?;
        }
        Command::Check => {
            let config = cli.load_config()?;
            crate::engine::Engine::new(config, crate::engine::Mode::Check)?.check(&mut report)?;
        }
        Command::Serve(args) => {
            let mut config = cli.load_config()?;
            args.apply(&mut config);
            crate::cli::serve::run(&mut report, &config)?;
        }
        Command::New(args) => {
            let config = cli.load_config()?;
            scaffold::new_page(&mut report, &args.target(&config), &config)?;
        }
        Command::Clean => {
            let config = cli.load_config()?;
            clean(&mut report, &config)?;
        }
        Command::Init(args) => {
            // A positional directory scaffolds there (`init my-site`); with none,
            // the current directory (already `--root`-adjusted) is used.
            let root = args.dir.clone().unwrap_or_else(|| PathBuf::from("."));
            scaffold::init(&mut report, &root, args.yes, args.vcs)?;
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

fn clean(report: &mut Report, config: &Config) -> Result<()> {
    report.milestone("cleaning")?;
    // The scratch root covers the cache; the configured cache dir is also
    // removed in case it was relocated outside `.baudelaire`.
    let targets = [
        config.dist.clone(),
        PathBuf::from(Config::SCRATCH),
        config.cache.dir.clone(),
    ];
    for dir in &targets {
        if dir.exists() {
            report.muted(format_args!("removing {}", dir.display()))?;
            crate::fs::remove_dir_all(dir)?;
        }
    }
    report.success("clean complete")?;
    Ok(())
}
