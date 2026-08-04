use std::fmt::Write as _;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::Command;

use owo_colors::OwoColorize;

use crate::cli::prompt::{Input, Prompt};
use crate::cli::scaffold::templates::{Extra, Quoted, Template, Vars};
use crate::cli::{InitArgs, NewArgs, Root};
use crate::codegen::Value;
use crate::config::{Config, SortKey};
use crate::content::{Collection, Frontmatter, Page, Slug};
use crate::error::Result;
use crate::error::warning::{MirrorSkipped, PermalinkTaken, ScaffoldExists, VcsFailed, VcsMissing};
use crate::fs;
use crate::mirror::{Mirror, Settings};
use crate::ui::{Paths, Ui};
use crate::world::Project;

/// Declarative scaffold: the files to create under a root, written in one pass
/// so nothing is laid down until every one of them is known.
struct Scaffold<'a> {
    root: &'a Path,
    files: Vec<(PathBuf, String)>,
}

impl<'a> Scaffold<'a> {
    /// The ignore file every scaffold writes, and its contents. Unconditional,
    /// because the two directories it names (`public/`, `.baudelaire/`) are
    /// build output either way: it used to ride along with `--vcs`, so a
    /// scaffold that ran `git init` itself, afterwards, committed both.
    const IGNORE: &'static str = ".gitignore";
    const IGNORED: &'static str = include_str!("scaffold/gitignore");

    fn new(root: &'a Path) -> Self {
        Self {
            root,
            files: Vec::new(),
        }
    }

    /// Add the ignore file both supported version-control systems read.
    fn ignore(self) -> Self {
        self.file(Self::IGNORE, Self::IGNORED)
    }

    fn file(mut self, rel: impl Into<PathBuf>, contents: impl Into<String>) -> Self {
        self.files.push((rel.into(), contents.into()));
        self
    }

    fn apply(self, ui: &Ui) -> Result<()> {
        for (rel, contents) in &self.files {
            let full = self.root.join(rel);
            // never clobber: `init` into an existing project must not overwrite
            // its config or templates. existing files are skipped with a warning.
            if full.exists() {
                ui.warn(ScaffoldExists { path: rel.clone() });
                continue;
            }
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&full, contents)?;
            ui.detail(format_args!(
                "{} {}",
                "+".green(),
                Paths(&rel.display().to_string())
            ));
        }
        Ok(())
    }
}

impl Config {
    /// The collection a content path falls into by convention: the top-level
    /// directory under the content root. `None` for a file directly under it (a
    /// root page, which belongs to no collection). Mirrors discovery's
    /// convention so `new` infers the same collection the build later will.
    fn collection_for(&self, path: &Path) -> Option<String> {
        let rel = path.strip_prefix(&self.paths.content).unwrap_or(path);
        let mut components = rel.components();
        match (components.next(), components.next()) {
            (Some(dir), Some(_)) => Some(dir.as_os_str().to_str()?.to_owned()),
            _ => None,
        }
    }

    /// The basename `new` treats as a page bundle's index, both when writing one
    /// (`--bundle`) and when reading a title back off one. The configured
    /// [`crate::config::ContentConfig::index`], or the same `index` the build
    /// falls back to, in one place rather than at each of the two call sites.
    pub(super) fn bundle_index(&self) -> &str {
        self.content.index.as_deref().unwrap_or("index")
    }

    /// Resolve the template file for a collection, defaulting to `layout.typ`.
    fn template_for(&self, collection: Option<&str>) -> String {
        collection
            .and_then(|c| self.collection(c))
            .and_then(|c| c.template.clone())
            .unwrap_or_else(|| "layout.typ".into())
    }
}

/// Scaffold a new project: pick the starter shape, fill its placeholders from
/// the flags (prompting for what they left out, when there is a terminal to
/// prompt at), write the files the flags did not exclude, and optionally
/// initialize a repository.
pub(crate) fn init(ui: &Ui, root: &Root, args: &InitArgs, config: &Path) -> Result<()> {
    // Every selection is resolved before anything is prompted for or written,
    // so a mistyped name fails on the spot rather than half a scaffold in.
    let template = Template::find(&args.template)?;
    let extras = Extra::resolve(&args.with)?;
    let config = templates::File::config_at(config)?;
    let interactive = !args.yes && std::io::stdin().is_terminal();
    let (target, details) = Details::gather(args, root, interactive)?;
    let repo = Repo::wanted(interactive, args.vcs)?;
    if interactive {
        ui.blank();
    }

    let files = template.files(&details.vars());
    // What the shape already configures is not appended a second time: `--with
    // search` on a shape whose config already sets `formats`, `fields` and a
    // palette used to bolt a barer `search { formats "json" }` on beneath it.
    let extras = Extra::wanted(&extras, &files, ui);

    let mut scaffold = Scaffold::new(&target).ignore();
    for file in files {
        if (args.theme.is_some() && file.themed()) || (args.no_sample && file.sample()) {
            continue;
        }
        let (rel, body) = match file.is_config() {
            true => (
                config.clone(),
                Details::config(&file.body, args.theme.as_deref(), &extras),
            ),
            false => (file.rel, file.body),
        };
        scaffold = scaffold.file(rel, body);
    }
    scaffold.apply(ui)?;

    if let Some(vcs) = repo {
        Repo::new(&target, vcs).setup(ui);
    }

    let settings = packages(ui, &target);

    ui.blank();
    ui.done_plain(format_args!(
        "{} project ready in {}",
        template.name,
        Paths(&target.display().to_string())
    ));
    ui.detail(format_args!("{}", template.about));
    ui.detail(format_args!(
        "run {} to build, {} for a live preview",
        "baudelaire build".cyan(),
        "baudelaire serve".cyan()
    ));
    // Last, because it is the one thing here that a reader has to act on: a
    // scaffold that mirrors the modules and never says they need pointing at
    // leaves every import in the templates it just wrote marked unresolved.
    if let Some(settings) = settings {
        settings.render(ui);
    }
    Ok(())
}

/// Mirror the generated modules for editor tooling, so the imports the
/// scaffolded templates and scripts carry resolve from the first minute.
///
/// A warning rather than an error: this is tooling convenience, and a platform
/// with no data directory (or one that is read-only) is no reason to fail a
/// scaffold that otherwise succeeded. The site builds either way, since a build
/// serves these modules from memory and never reads what this writes.
fn packages(ui: &Ui, target: &Path) -> Option<Settings> {
    let config = Config {
        root: target.to_path_buf(),
        ..Config::default()
    };
    match Mirror::new(&config, None, false).install() {
        Ok(install) => Some(install.render(ui)),
        Err(error) => {
            ui.warn(MirrorSkipped {
                reason: error.to_string(),
            });
            None
        }
    }
}

/// Project metadata for a fresh scaffold: prompted interactively, or defaulted
/// from the target directory name and git config.
struct Details {
    site: String,
    author: String,
    url: String,
    lang: String,
}

impl Details {
    /// The site name a run falls back to when nothing names one: the prompt's
    /// default answer and the name for a directory that has none (`/`, or a
    /// path ending in `..`).
    const UNNAMED: &'static str = "my-site";

    /// Where to scaffold, and what to fill the placeholders with.
    ///
    /// A flag always wins. What a flag did not supply is prompted for when
    /// there is a terminal to prompt at, and otherwise defaulted, so `--yes` in
    /// CI is fully scriptable rather than silently accepting `example.com`.
    ///
    /// The target directory has three cases: an explicit `dir` names it, an
    /// interactive run without one takes the site name for it, and a
    /// non-interactive run without one scaffolds into `.`.
    fn gather(args: &InitArgs, root: &Root, interactive: bool) -> Result<(PathBuf, Self)> {
        let git = Self::git_author().unwrap_or_default();
        // Only prompt for what was not given, and only when someone is there.
        let ask = |label: &str, default: &str, given: Option<&String>| -> Result<String> {
            match given {
                Some(v) => Ok(v.clone()),
                None if interactive => Input::new(label).default(default).ask(),
                None => Ok(default.to_owned()),
            }
        };

        let (target, site) = match &args.dir {
            Some(d) => (d.clone(), Self::dir_name(d, root)),
            None if interactive && args.title.is_none() => {
                let site = Input::new("Site name").default(Self::UNNAMED).ask()?;
                (PathBuf::from(&site), site)
            }
            // No directory, but a title (or no terminal): scaffold into `.`, the
            // shape `baudelaire init --yes` has always had in CI.
            None => (PathBuf::from("."), Self::dir_name(Path::new("."), root)),
        };
        let site = match &args.title {
            Some(t) => t.clone(),
            None => site,
        };
        let author = ask("Author", &git, args.author.as_ref())?;
        let url = ask("Base URL", "https://example.com", args.url.as_ref())?;

        Ok((
            target,
            Self {
                site,
                author,
                url,
                lang: args.lang.clone(),
            },
        ))
    }

    /// The placeholder values every scaffolded file is rendered against.
    fn vars(&self) -> Vars<'_> {
        Vars::new([
            ("site", self.site.as_str()),
            ("author", self.author.as_str()),
            ("url", self.url.as_str()),
            ("lang", self.lang.as_str()),
        ])
    }

    /// The scaffolded `config.kdl`: the template's own, plus whatever the flags
    /// bolt on. Both additions are appended rather than spliced, which a KDL
    /// config tolerates because a repeated section fills in place.
    fn config(rendered: &str, theme: Option<&str>, extras: &[&'static Extra]) -> String {
        let mut out = rendered.to_owned();
        if let Some(theme) = theme {
            let _ = write!(
                out,
                "\n// Templates, assets and config defaults come from this package.\ntheme \"{}\"\n",
                Quoted(theme)
            );
        }
        for extra in extras {
            out.push_str(extra.fragment);
        }
        out
    }

    /// A sensible default site name from the target directory's last component.
    fn dir_name(target: &Path, root: &Root) -> String {
        root.join(target)
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|n| !n.is_empty())
            .map_or_else(|| Self::UNNAMED.to_owned(), str::to_owned)
    }

    /// The user's name from git config, if configured.
    fn git_author() -> Option<String> {
        let output = Command::new("git")
            .args(["config", "user.name"])
            .output()
            .ok()?;
        let name = String::from_utf8(output.stdout).ok()?.trim().to_owned();
        (!name.is_empty()).then_some(name)
    }
}

/// A new content page `new` will scaffold: its target path plus the structure
/// inferred for it: title from the filename, the ordering field from the
/// collection (a `date` for a dated collection, the next `order` for an ordered
/// one), the template, and the permalink it will occupy (with any existing
/// occupant). The operation is a type, not a free function: [`plan`](Self::plan)
/// reads the config and existing content to infer, then [`create`](Self::create)
/// writes. Only standard frontmatter fields are written; content is the author's.
// The type is the plan for a new page; the field is the `draft` frontmatter key
// that plan writes. Same word, two different things, and the key is not ours to
// rename.
#[allow(clippy::struct_field_names)]
pub(crate) struct Draft {
    /// The file to write; a bundle resolves to `<dir>/index.typ`.
    path: PathBuf,
    title: String,
    template: String,
    date: Option<time::Date>,
    order: Option<i64>,
    draft: bool,
    permalink: String,
    /// The source of an existing page already producing `permalink`, if any.
    collision: Option<String>,
    /// Whether to open the created file in `$EDITOR`.
    edit: bool,
}

impl Draft {
    /// Infer everything for the page named by `args`, reading the collection
    /// config and the existing content. Errors if the target already exists.
    ///
    /// `project` is what makes the existing content readable, and is `None`
    /// when it could not be opened at all: the ordering and collision hints go
    /// with it, the page is still written.
    pub(crate) fn plan(args: &NewArgs, config: &Config, project: Option<&Project>) -> Result<Self> {
        let path = args.target(config);
        if path.exists() {
            return Err(crate::error::ScaffoldError::already_exists(&path).into());
        }
        let collection = config.collection_for(&path);
        let template = config.template_for(collection.as_deref());
        // The display name behind the slug: a bundle takes its directory's name.
        let raw = Self::raw_name(&path, config);
        let slug = Slug::parse(&raw).map_or_else(|| raw.clone(), Slug::into_string);
        let title = args.title.clone().unwrap_or_else(|| Self::titleize(&raw));

        // The collection's sort decides which ordering field the page wants: a
        // frozen `date` for a dated collection, the next `order` for an ordered
        // one. An unconfigured collection sorts by `order` (the default).
        let sort = collection
            .as_deref()
            .map(|c| config.collection(c).map(|cc| cc.sort).unwrap_or_default());
        // Discover once, reused for the next order and the collision check.
        // A discovery failure (e.g. a broken sibling page) must not block `new`.
        let discovered = project
            .and_then(|project| crate::content::discover(config, project).ok())
            .unwrap_or_default();

        let date = match &args.date {
            Some(input) => Some(Self::parse_date(input)?),
            None if sort == Some(SortKey::Date) => Some(time::OffsetDateTime::now_utc().date()),
            None => None,
        };
        let order = match (&collection, sort) {
            (Some(c), Some(SortKey::Order)) => Some(Self::next_order(c, &discovered)),
            _ => None,
        };

        let frontmatter = Frontmatter {
            title: Some(title.clone()),
            date,
            order,
            ..Frontmatter::default()
        };
        // A root page (no collection) maps `index` to `/` and every other slug
        // to `/{slug}/`; `permalink_of` owns that fallback, exactly as the build.
        let permalink =
            Page::permalink_of(collection.as_deref(), &frontmatter, &slug, &path, config);

        let output = config.destination(&permalink);
        let collision = discovered
            .iter()
            .flat_map(|c| c.pages.iter())
            .find(|p| p.output == output && p.source != path)
            .map(|p| p.source.display().to_string());

        Ok(Self {
            path,
            title,
            template,
            date,
            order,
            draft: args.is_draft(),
            permalink,
            collision,
            edit: args.edit,
        })
    }

    /// Write the planned page: warn if its permalink is already taken, create
    /// the file (and any parent dirs), report the path and the URL it lands at,
    /// and open it in `$EDITOR` when asked.
    pub(crate) fn create(self, ui: &Ui) -> Result<()> {
        if let Some(origin) = &self.collision {
            ui.warn(PermalinkTaken {
                url: self.permalink.clone(),
                origin: origin.clone(),
            });
        }
        let name = self
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("untitled.typ");
        Scaffold::new(self.path.parent().unwrap_or_else(|| Path::new(".")))
            .file(name, self.body())
            .apply(ui)?;
        ui.done(format_args!(
            "created {} {} {}",
            Paths(&self.path.display().to_string()),
            "→".dimmed(),
            self.permalink.cyan()
        ));
        if self.edit {
            Editor::open(&self.path, ui);
        }
        Ok(())
    }

    /// The name behind the page's slug: the file stem, or (for a bundle
    /// `index`) the directory it lives in, so `posts/hello/index.typ` is titled
    /// "Hello", not "Index".
    fn raw_name(path: &Path, config: &Config) -> String {
        let index = config.bundle_index();
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or(index);
        if stem == index {
            path.parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or(stem)
                .to_owned()
        } else {
            stem.to_owned()
        }
    }

    /// De-slugify a filename into a title: split on `-`/`_`/spaces and
    /// capitalize each word (`my-first-post` -> `My First Post`).
    fn titleize(name: &str) -> String {
        name.split(['-', '_', ' '])
            .filter(|w| !w.is_empty())
            .map(|w| {
                let mut chars = w.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The next `order` for a collection: one past the highest already used, or
    /// 1 for the first page, so a new chapter appends to the end.
    fn next_order(collection: &str, discovered: &[Collection]) -> i64 {
        discovered
            .iter()
            .filter(|c| c.id == collection)
            .flat_map(|c| c.pages.iter())
            .filter_map(|p| p.frontmatter.order)
            .max()
            // Saturating: `order` comes from authored frontmatter, and
            // `overflow-checks` is off in release, so `i64::MAX` wrapped to
            // `i64::MIN` and the new page sorted first instead of last.
            .map_or(1, |highest| highest.saturating_add(1))
    }

    fn parse_date(input: &str) -> Result<time::Date> {
        let bad = || crate::error::ScaffoldError::bad_date(input);
        let parts: Vec<&str> = input.split('-').collect();
        let [year, month, day] = parts.as_slice() else {
            return Err(bad().into());
        };
        let year: i32 = year.parse().map_err(|_| bad())?;
        let month: u8 = month.parse().map_err(|_| bad())?;
        let month = time::Month::try_from(month).map_err(|_| bad())?;
        let day: u8 = day.parse().map_err(|_| bad())?;
        time::Date::from_calendar_date(year, month, day).map_err(|_| bad().into())
    }

    /// The scaffolded `.typ`: a computed `#let frontmatter = (..)` export plus a
    /// body stub. Values go through [`Value`] so strings are escaped.
    fn body(&self) -> String {
        let mut fields: Vec<(&str, Value)> = vec![("title", Value::str(&self.title))];
        if let Some(d) = self.date {
            fields.push((
                "date",
                Value::Raw(format!(
                    "datetime(year: {}, month: {}, day: {})",
                    d.year(),
                    u8::from(d.month()),
                    d.day()
                )),
            ));
        }
        if let Some(order) = self.order {
            fields.push(("order", Value::Int(order)));
        }
        fields.push(("draft", Value::Bool(self.draft)));
        fields.push(("template", Value::str(&self.template)));

        let mut out = String::from("#let frontmatter = (\n");
        for (key, value) in &fields {
            let _ = writeln!(out, "  {key}: {},", crate::codegen::Typst(value));
        }
        out.push_str(")\n\nYour content here.\n");
        out
    }
}

/// The user's configured text editor. Namespaces the "open a file in `$EDITOR`"
/// action, in the unit-struct style of the rest of the codebase.
struct Editor;

impl Editor {
    /// Open `path` in `$VISUAL`/`$EDITOR`, best-effort: a missing or failing
    /// editor is a note, never a failed command.
    fn open(path: &Path, ui: &Ui) {
        let editor = std::env::var("VISUAL")
            .or_else(|_| std::env::var("EDITOR"))
            .ok()
            .filter(|e| !e.is_empty());
        match editor {
            Some(editor) => {
                if let Err(e) = Command::new(&editor).arg(path).status() {
                    ui.detail(format_args!("could not launch `{editor}`: {e}"));
                }
            }
            None => ui.detail(format_args!(
                "set {} to open new files here",
                "$EDITOR".cyan()
            )),
        }
    }
}

/// A version-control system baudelaire can initialize for a new project. Both
/// use the same `.gitignore` (jujutsu honors it too).
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Vcs {
    Git,
    #[value(alias = "jj")]
    Jujutsu,
}

/// What setting up one [`Vcs`] takes: the program to run and its arguments, the
/// marker directory whose presence means a repository already exists, and the
/// name to report. One row per variant, so a new VCS is one row and nothing
/// about it can be declared in one match and forgotten in another.
struct Tool {
    command: &'static str,
    args: &'static [&'static str],
    marker: &'static str,
    label: &'static str,
}

impl Vcs {
    /// The tool that initializes a repository. Jujutsu colocates a `.git`, so it
    /// stays interoperable with git tooling.
    const fn tool(self) -> Tool {
        match self {
            Self::Git => Tool {
                command: "git",
                args: &["init", "-q"],
                marker: ".git",
                label: "git",
            },
            Self::Jujutsu => Tool {
                command: "jj",
                args: &["git", "init", "--colocate"],
                marker: ".jj",
                label: "jujutsu",
            },
        }
    }
}

/// Optional version-control setup for a freshly scaffolded project. Opt-in,
/// because not every scaffold wants a repository; the `.gitignore` both systems
/// read is written unconditionally by [`Scaffold::ignore`], since it describes
/// the build output rather than the repository.
struct Repo<'a> {
    root: &'a Path,
    vcs: Vcs,
}

impl<'a> Repo<'a> {
    fn new(root: &'a Path, vcs: Vcs) -> Self {
        Self { root, vcs }
    }

    /// Which VCS to set up, if any. An explicit `--vcs` wins; otherwise ask,
    /// but only when the session is `interactive` (decided once, in `init`);
    /// piped or CI input sets up nothing, so a scaffold never blocks nor
    /// creates a repo unbidden.
    ///
    /// `--yes` means only "do not prompt". It used to also create a git
    /// repository, so the one flag every script reaches for to silence the
    /// prompts wrote a repository nobody asked for, and its help line ("skip the
    /// prompt and set up version control") was doing two jobs at once. Naming
    /// `--vcs` is now the only way to ask.
    fn wanted(interactive: bool, explicit: Option<Vcs>) -> Result<Option<Vcs>> {
        if explicit.is_some() {
            return Ok(explicit);
        }
        if !interactive {
            return Ok(None);
        }
        // git is the default (empty answer); every option lists its aliases.
        Prompt::new("set up version control?")
            .option(&["git", "g", "y", "yes"], Some(Vcs::Git))
            .default()
            .option(&["jj", "jujutsu", "j"], Some(Vcs::Jujutsu))
            .option(&["no", "n"], None)
            .ask()
    }

    /// Initialize the repository, skipping the step if one already exists. A
    /// missing or failing tool is a warning, not an error: the project is
    /// scaffolded either way.
    fn setup(&self, ui: &Ui) {
        let tool = self.vcs.tool();
        if self.root.join(tool.marker).exists() {
            return;
        }
        // Capture the tool's output rather than inherit it: jj in particular
        // prints an "Initialized repo" line and a hint that would clutter the
        // scaffold log. Surface it only if the command actually failed.
        match Command::new(tool.command)
            .args(tool.args)
            .current_dir(self.root)
            .output()
        {
            Ok(out) if out.status.success() => {
                ui.detail(format_args!("{} {} repository", "+".green(), tool.label));
            }
            Ok(out) => {
                let detail = String::from_utf8_lossy(&out.stderr);
                let detail = detail.trim();
                ui.warn(VcsFailed {
                    tool: tool.command,
                    detail: (!detail.is_empty()).then(|| detail.to_owned()),
                });
            }
            Err(_) => ui.warn(VcsMissing { tool: tool.command }),
        }
    }
}

/// Scaffold templates, embedded from `scaffold/` at build time. Editing those
/// files (not string literals here) changes what `init`/`new` produce.
pub(super) mod templates {
    use std::fmt::{self, Write as _};
    use std::path::{Path, PathBuf};

    use include_dir::{Dir, include_dir};
    use itertools::Itertools as _;

    /// One starter project shape.
    ///
    /// The directory *is* the manifest: every file under it is written at the
    /// same relative path, so adding a file to a template is adding a file, not
    /// a file plus a table entry.
    pub struct Template {
        /// What `--template` accepts, and what the summary reports.
        pub name: &'static str,
        /// One line for `--help` and for the unknown-name suggestion.
        pub about: &'static str,
        files: Dir<'static>,
    }

    /// The registered starter templates.
    ///
    /// Single source of truth: the flag's default, its help listing, the
    /// resolution of a given name and the "did you mean" on a typo all read this
    /// one table, so a new template is one entry and cannot drift out of the
    /// help text.
    pub const TEMPLATES: &[Template] = &[
        Template {
            name: "blog",
            about: "dated posts, tags, pagination and feeds",
            files: include_dir!("$CARGO_MANIFEST_DIR/src/cli/scaffold/blog"),
        },
        Template {
            name: "docs",
            about: "ordered sections, sidebar nav and client-side search",
            files: include_dir!("$CARGO_MANIFEST_DIR/src/cli/scaffold/docs"),
        },
        Template {
            name: "book",
            about: "ordered chapters, also exported as one HTML file",
            files: include_dir!("$CARGO_MANIFEST_DIR/src/cli/scaffold/book"),
        },
        Template {
            name: "minimal",
            about: "one page and one template, nothing else",
            files: include_dir!("$CARGO_MANIFEST_DIR/src/cli/scaffold/minimal"),
        },
    ];

    /// One optional feature `--with` switches on.
    pub struct Extra {
        /// What `--with` accepts.
        pub name: &'static str,
        /// The KDL appended verbatim to the rendered config. A duplicate
        /// top-level block is not a conflict: [`crate::config`] dispatches every
        /// node in order and a nested section fills in place, so a second
        /// `generate { .. }` merges into the first rather than replacing it.
        pub fragment: &'static str,
        /// Whether the starter shape already asked for this. A shape that
        /// configures the feature itself (the `docs` one configures search, with
        /// fields and a palette) would otherwise be handed a second, barer block
        /// saying the same thing.
        pub present: fn(&crate::config::Config) -> bool,
    }

    /// The registered optional features.
    ///
    /// Single source of truth for what `--with` accepts, exactly as
    /// [`TEMPLATES`] is for `--template`.
    pub const EXTRAS: &[Extra] = &[
        Extra {
            name: "spa",
            fragment: "\n// Client-side navigation between the built pages.\nnavigation {\n  spa { }\n}\n",
            present: |config| config.navigation.spa.enabled,
        },
        Extra {
            name: "standalone",
            fragment: "\n// Also emit the whole site as one self-contained HTML file.\nnavigation {\n  standalone { }\n}\n",
            present: |config| config.navigation.standalone.enabled,
        },
        Extra {
            name: "speculation",
            fragment: "\n// Browser-native prefetch hints for same-site links.\nnavigation {\n  speculation { }\n}\n",
            present: |config| config.navigation.speculation.enabled,
        },
        Extra {
            name: "search",
            fragment: "\n// Client-side search index.\ngenerate {\n  search { formats \"json\" }\n}\n",
            present: |config| !config.generate.search.formats.is_empty(),
        },
        Extra {
            name: "pdf",
            fragment: "\n// A PDF of every page, from `templates/print.typ`.\ngenerate {\n  pdf { pages { template \"print.typ\" } }\n}\n",
            present: |config| config.generate.pdf.enabled(),
        },
    ];

    impl Template {
        /// The shape `init` scaffolds when `--template` is not given: the first
        /// registered one, so the flag's default cannot name a missing table row.
        pub const DEFAULT: &'static str = TEMPLATES[0].name;

        /// The template `name` selects, or an error naming the valid ones.
        pub fn find(name: &str) -> crate::error::Result<&'static Self> {
            TEMPLATES.iter().find(|t| t.name == name).ok_or_else(|| {
                let names = Self::names();
                crate::error::ScaffoldError::unknown_template(
                    name,
                    crate::config::dispatch::Keys::of(&names).help(name, "templates"),
                )
                .into()
            })
        }

        fn names() -> Vec<&'static str> {
            TEMPLATES.iter().map(|t| t.name).collect()
        }

        /// The `--template` help line, listing the table's own rows so a new
        /// shape cannot ship undocumented.
        pub fn help() -> String {
            format!("Starter shape: {}", Self::names().iter().format(", "))
        }

        /// The template's files, placeholders already substituted. Ordered so
        /// the scaffold summary is stable.
        pub fn files(&self, vars: &Vars<'_>) -> Vec<File> {
            let mut out = Vec::new();
            Self::walk(&self.files, vars, &mut out);
            out.sort_by(|a, b| a.rel.cmp(&b.rel));
            out
        }

        fn walk(dir: &Dir<'_>, vars: &Vars<'_>, out: &mut Vec<File>) {
            for file in dir.files() {
                // Every scaffolded file is text we authored, so a non-UTF-8 one
                // is a bug here rather than something to degrade over.
                let body = file.contents_utf8().expect("scaffold files are UTF-8");
                out.push(File {
                    rel: file.path().to_path_buf(),
                    body: vars.render(body),
                });
            }
            for sub in dir.dirs() {
                Self::walk(sub, vars, out);
            }
        }
    }

    impl Extra {
        /// The extras `names` selects, or an error naming the valid ones. A typo
        /// is refused rather than dropped: an unmatched `--with` used to
        /// scaffold a site missing the very feature it asked for, silently.
        pub fn resolve(names: &[String]) -> crate::error::Result<Vec<&'static Self>> {
            names.iter().map(|name| Self::find(name)).collect()
        }

        /// The extras that still have something to add, given what the starter
        /// shape's own config already declares. A shape that configures the
        /// feature is left alone and reported, rather than handed a second block
        /// that says less than the one already there.
        ///
        /// A config that does not parse yields every extra: appending is then
        /// the honest thing to do, and the parse error is not this function's to
        /// report (a test parses every shipped shape).
        pub fn wanted(
            extras: &[&'static Self],
            files: &[File],
            ui: &crate::ui::Ui,
        ) -> Vec<&'static Self> {
            let Some(config) = files
                .iter()
                .find(|f| f.is_config())
                .and_then(|f| crate::config::Config::parse(&f.body).ok())
            else {
                return extras.to_vec();
            };
            extras
                .iter()
                .filter(|extra| {
                    let present = (extra.present)(&config);
                    if present {
                        ui.detail(format_args!(
                            "{} is already part of this shape",
                            extra.name
                        ));
                    }
                    !present
                })
                .copied()
                .collect()
        }

        fn find(name: &str) -> crate::error::Result<&'static Self> {
            EXTRAS.iter().find(|e| e.name == name).ok_or_else(|| {
                let names = Self::names();
                crate::error::ScaffoldError::unknown_extra(
                    name,
                    crate::config::dispatch::Keys::of(&names).help(name, "features"),
                )
                .into()
            })
        }

        fn names() -> Vec<&'static str> {
            EXTRAS.iter().map(|e| e.name).collect()
        }

        /// The `--with` help line, listing the table's own rows.
        pub fn help() -> String {
            format!(
                "Switch on optional features: {}",
                Self::names().iter().format(", ")
            )
        }
    }

    /// One file a starter shape ships: where it lands under the project root,
    /// and its rendered contents.
    pub struct File {
        pub rel: PathBuf,
        pub body: String,
    }

    impl File {
        /// The fixed layout every starter shape shares. The flags that skip or
        /// rewrite a scaffolded file all decide from these, not from the
        /// configured `paths { }`: these are the paths the shipped `config.kdl`
        /// declares.
        const CONFIG: &'static str = "config.kdl";
        const HOME: &'static str = "content/index.typ";
        const CONTENT: &'static str = "content";
        /// The directories a theme package supplies in the project's stead.
        const THEMED: &'static [&'static str] = &["templates", "assets"];

        /// Whether this is the config `init` bolts its flags onto.
        pub fn is_config(&self) -> bool {
            self.rel == Path::new(Self::CONFIG)
        }

        /// Where the scaffolded config lands: whatever the global `--config`
        /// names, so a project initialized under one name is one every later
        /// command finds under the same flag. The flag used to be accepted and
        /// ignored, writing `config.kdl` and reporting success.
        ///
        /// Only a bare filename can serve: a `paths { }` entry resolves against
        /// the working directory rather than against the config file, so a
        /// config nested a directory down would name a content tree outside its
        /// own project.
        pub fn config_at(path: &Path) -> crate::error::Result<PathBuf> {
            if path.file_name().is_none_or(|name| name != path.as_os_str()) {
                return Err(crate::error::ScaffoldError::config_path(path).into());
            }
            Ok(path.to_path_buf())
        }

        /// Whether a `--theme` package already supplies this file, in which case
        /// scaffolding a copy would shadow it on the very first build.
        pub fn themed(&self) -> bool {
            Self::THEMED.iter().any(|dir| self.rel.starts_with(dir))
        }

        /// Whether this is a demo page `--no-sample` drops. The home page is not
        /// one: a site with no content at all does not build to anything.
        pub fn sample(&self) -> bool {
            self.rel.starts_with(Self::CONTENT) && self.rel != Path::new(Self::HOME)
        }
    }

    /// The placeholder values a template is rendered against.
    pub struct Vars<'a>(Vec<(&'a str, &'a str)>);

    impl<'a> Vars<'a> {
        pub fn new(pairs: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
            Self(pairs.into_iter().collect())
        }

        /// Substitute `{{key}}` placeholders in a template, in a single left-to-
        /// right pass: a substituted value is never rescanned, so a site name
        /// containing `{{author}}` stays literal. Values are escaped for the
        /// double-quoted string context they land in, so a quote in a site name
        /// yields valid config. Unknown placeholders are left untouched.
        pub fn render(&self, template: &str) -> String {
            let mut out = String::with_capacity(template.len());
            let mut rest = template;
            while let Some(open) = rest.find("{{") {
                out.push_str(&rest[..open]);
                let after = &rest[open + 2..];
                let Some(close) = after.find("}}") else {
                    // No closing braces anywhere ahead: emit the rest verbatim.
                    out.push_str(&rest[open..]);
                    return out;
                };
                let key = &after[..close];
                if let Some((_, value)) = self.0.iter().find(|(k, _)| *k == key) {
                    let _ = write!(out, "{}", Quoted(value));
                } else {
                    out.push_str("{{");
                    out.push_str(key);
                    out.push_str("}}");
                }
                rest = &after[close + 2..];
            }
            out.push_str(rest);
            out
        }
    }

    /// A value escaped for the double-quoted string literal it is interpolated
    /// into. KDL and typst share `\` and `"` escapes, so one adapter serves both
    /// the rendered templates and the config fragments `init` appends.
    pub struct Quoted<'a>(pub &'a str);

    impl fmt::Display for Quoted<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            for c in self.0.chars() {
                if matches!(c, '"' | '\\') {
                    f.write_char('\\')?;
                }
                f.write_char(c)?;
            }
            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{EXTRAS, Extra, File, TEMPLATES, Template, Vars};

        fn vars() -> Vars<'static> {
            Vars::new([
                ("site", "My Site"),
                ("author", "Me"),
                ("url", "https://example.com"),
                ("lang", "en"),
            ])
        }

        /// A starter config is what a new site begins from, so every one has to
        /// stay valid against the dispatch tables. Nothing else reads them,
        /// which is exactly how a key rename ships a broken `init`.
        #[test]
        fn every_scaffolded_config_parses() {
            for template in TEMPLATES {
                let files = template.files(&vars());
                let text = &files
                    .iter()
                    .find(|f| f.is_config())
                    .unwrap_or_else(|| panic!("`{}` has no config.kdl", template.name))
                    .body;
                let config = crate::config::Config::parse(text)
                    .unwrap_or_else(|e| panic!("`{}` config: {e}", template.name));
                // every declared profile has to apply, not merely parse
                for (name, _) in config.profiles.clone() {
                    config
                        .clone()
                        .with_profile(&name)
                        .unwrap_or_else(|e| panic!("`{}` profile `{name}`: {e}", template.name));
                }
            }
        }

        /// `--config` names the file `init` writes, and only a bare name can:
        /// a nested one would leave every `paths { }` entry pointing outside
        /// the project it was scaffolded into.
        #[test]
        fn the_scaffolded_config_takes_a_name_not_a_path() {
            use std::path::Path;
            assert_eq!(
                File::config_at(Path::new("site.kdl")).unwrap(),
                Path::new("site.kdl")
            );
            assert!(File::config_at(Path::new("conf/site.kdl")).is_err());
            assert!(File::config_at(Path::new("/etc/site.kdl")).is_err());
            assert!(File::config_at(Path::new("../site.kdl")).is_err());
        }

        /// Every template ships a home page and a template to render it with,
        /// the two files without which the scaffold does not build.
        #[test]
        fn every_template_is_complete() {
            for template in TEMPLATES {
                let files = template.files(&vars());
                assert!(
                    files.iter().any(File::is_config),
                    "`{}` has no config",
                    template.name
                );
                assert!(
                    files
                        .iter()
                        .any(|f| f.rel == std::path::Path::new(File::HOME)),
                    "`{}` has no home page",
                    template.name
                );
                assert!(
                    files
                        .iter()
                        .any(|f| f.rel.starts_with("templates") && f.rel.extension().is_some()),
                    "`{}` ships no template",
                    template.name
                );
                assert!(
                    !files.iter().any(|f| f.body.contains("{{")),
                    "`{}` left a placeholder unfilled",
                    template.name
                );
            }
        }

        /// A `--with` fragment is appended to a config that may already carry
        /// the same section, so each has to survive the merge on its own.
        #[test]
        fn every_extra_parses_onto_every_template() {
            for template in TEMPLATES {
                let files = template.files(&vars());
                let base = &files.iter().find(|f| f.is_config()).expect("config").body;
                for extra in EXTRAS {
                    let text = format!("{base}{}", extra.fragment);
                    crate::config::Config::parse(&text).unwrap_or_else(|e| {
                        panic!("`{}` + --with {}: {e}", template.name, extra.name)
                    });
                }
            }
        }

        /// A feature the shape already configures is dropped rather than
        /// appended: the `docs` shape sets `search` with its fields and its
        /// palette, and a second, barer block underneath said less.
        #[test]
        fn an_extra_the_shape_already_configures_is_dropped() {
            let ui = crate::ui::Ui::new(crate::ui::Level::Silent);
            let docs = Template::find("docs").expect("docs shape");
            let files = docs.files(&vars());
            let asked = Extra::resolve(&["search".to_owned(), "pdf".to_owned()]).expect("features");
            let wanted = Extra::wanted(&asked, &files, &ui);
            assert_eq!(
                wanted.iter().map(|e| e.name).collect::<Vec<_>>(),
                vec!["pdf"]
            );
        }

        /// An unknown template names the real ones rather than failing bare.
        #[test]
        fn an_unknown_template_suggests_a_real_one() {
            let Err(err) = Template::find("blogg") else {
                panic!("`blogg` is not a template");
            };
            let rendered = format!("{err:?}");
            assert!(rendered.contains("blogg"), "{rendered}");
            assert!(
                rendered.contains("blog"),
                "suggests the real one: {rendered}"
            );
        }

        /// An unknown `--with` feature is refused the same way, rather than
        /// scaffolding a site quietly missing what it asked for.
        #[test]
        fn an_unknown_extra_suggests_a_real_one() {
            let Err(err) = Extra::resolve(&["serach".to_owned()]) else {
                panic!("`serach` is not a feature");
            };
            let rendered = format!("{err:?}");
            assert!(rendered.contains("serach"), "{rendered}");
            assert!(
                rendered.contains("search"),
                "suggests the real one: {rendered}"
            );
        }

        #[test]
        fn fills_known_placeholders() {
            let out = Vars::new([("site", "S"), ("author", "A")])
                .render("site \"{{site}}\" by {{author}}");
            assert_eq!(out, "site \"S\" by A");
        }

        #[test]
        fn escapes_quotes_and_backslashes_in_values() {
            let out = Vars::new([("site", "My \"Quoted\\\" Site")]).render("site \"{{site}}\"");
            assert_eq!(out, "site \"My \\\"Quoted\\\\\\\" Site\"");
        }

        #[test]
        fn substituted_values_are_never_rescanned() {
            let out = Vars::new([("site", "{{author}}"), ("author", "Me")])
                .render("{{site}} by {{author}}");
            assert_eq!(out, "{{author}} by Me");
        }

        #[test]
        fn unknown_placeholders_are_left_alone() {
            assert_eq!(
                Vars::new([("site", "S")]).render("keep {{unknown}}"),
                "keep {{unknown}}"
            );
        }

        #[test]
        fn unterminated_braces_pass_through() {
            assert_eq!(
                Vars::new([("site", "S")]).render("dangling {{site"),
                "dangling {{site"
            );
        }
    }
}

#[cfg(test)]
mod repo_tests {
    use super::{Repo, Vcs};

    /// A non-interactive scaffold sets up nothing unless `--vcs` names it.
    /// `--yes` used to mean git as well as "do not prompt", so the flag scripts
    /// reach for to silence the prompts left a repository behind.
    #[test]
    fn only_an_explicit_vcs_sets_one_up_without_a_prompt() {
        assert_eq!(Repo::wanted(false, None).unwrap(), None);
        assert_eq!(Repo::wanted(false, Some(Vcs::Git)).unwrap(), Some(Vcs::Git));
        assert_eq!(
            Repo::wanted(true, Some(Vcs::Jujutsu)).unwrap(),
            Some(Vcs::Jujutsu)
        );
    }
}

#[cfg(test)]
mod order_tests {
    use super::Draft;
    use crate::config::CollectionConfig;
    use crate::content::{Collection, Data, Frontmatter, Page, PageId, Siblings};
    use std::path::PathBuf;

    fn collection(orders: &[i64]) -> Collection {
        Collection {
            id: "posts".into(),
            config: CollectionConfig::default(),
            pages: orders
                .iter()
                .map(|order| Page {
                    id: PageId::new("posts", "a"),
                    source: PathBuf::from("content/posts/a.typ"),
                    frontmatter: Frontmatter {
                        order: Some(*order),
                        ..Frontmatter::default()
                    },
                    body: String::new(),
                    data: Data::Empty,
                    collection: "posts".into(),
                    permalink: "/p/".into(),
                    output: PathBuf::new(),
                    template: None,
                    lang: "en".into(),
                    siblings: Siblings::default(),
                    translations: Vec::new(),
                })
                .collect(),
        }
    }

    /// `order` comes from authored frontmatter and `overflow-checks` is off in
    /// release, so `i64::MAX + 1` wrapped to `i64::MIN` and the new page sorted
    /// first instead of last.
    #[test]
    fn next_order_saturates_instead_of_wrapping() {
        assert_eq!(Draft::next_order("posts", &[collection(&[])]), 1);
        assert_eq!(Draft::next_order("posts", &[collection(&[1, 3, 2])]), 4);
        assert_eq!(
            Draft::next_order("posts", &[collection(&[i64::MAX])]),
            i64::MAX
        );
    }
}
