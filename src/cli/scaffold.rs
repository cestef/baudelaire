use std::fmt::Write as _;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::Command;

use owo_colors::OwoColorize;

use crate::cli::prompt::{Input, Prompt};
use crate::cli::{NewArgs, Root};
use crate::codegen::Value;
use crate::config::{Config, SortKey};
use crate::content::{Collection, Frontmatter, Page, Slug};
use crate::error::Result;
use crate::error::warning::{PermalinkTaken, ScaffoldExists, VcsFailed, VcsMissing};
use crate::fs;
use crate::ui::{Paths, Ui};
use crate::world::Project;

/// Declarative scaffold: dirs + files to create under a root.
struct Scaffold<'a> {
    root: &'a Path,
    dirs: Vec<PathBuf>,
    files: Vec<(PathBuf, String)>,
}

impl<'a> Scaffold<'a> {
    fn new(root: &'a Path) -> Self {
        Self {
            root,
            dirs: Vec::new(),
            files: Vec::new(),
        }
    }

    fn dir(mut self, rel: impl Into<PathBuf>) -> Self {
        self.dirs.push(rel.into());
        self
    }

    fn file(mut self, rel: impl Into<PathBuf>, contents: impl Into<String>) -> Self {
        self.files.push((rel.into(), contents.into()));
        self
    }

    fn apply(self, ui: &Ui) -> Result<()> {
        for dir in &self.dirs {
            let path = self.root.join(dir);
            if !path.exists() {
                fs::create_dir_all(&path)?;
                ui.detail(format_args!(
                    "{} {}",
                    "+".green(),
                    Paths(&dir.display().to_string())
                ));
            }
        }
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
        let rel = path.strip_prefix(&self.content).unwrap_or(path);
        let mut components = rel.components();
        match (components.next(), components.next()) {
            (Some(dir), Some(_)) => Some(dir.as_os_str().to_str()?.to_owned()),
            _ => None,
        }
    }

    /// Resolve the template file for a collection, defaulting to `layout.typ`.
    fn template_for(&self, collection: Option<&str>) -> String {
        collection
            .and_then(|c| self.collection(c))
            .and_then(|c| c.template.clone())
            .unwrap_or_else(|| "layout.typ".into())
    }
}

/// scaffold a new project where `dir` is the explicit positional argument, if any
/// when `None` & interactive, the user is prompted for a site name which
/// doubles as the target directory. `yes` skips prompts; `vcs` pins the vcs
pub(crate) fn init(
    ui: &Ui,
    dir: Option<&Path>,
    root: &Root,
    yes: bool,
    vcs: Option<Vcs>,
) -> Result<()> {
    let interactive = !yes && std::io::stdin().is_terminal();
    let (target, details) = Details::gather(dir, root, interactive)?;
    let repo = Repo::wanted(yes, interactive, vcs)?;
    if interactive {
        ui.blank();
    }

    Scaffold::new(&target)
        .dir("content")
        .dir("assets")
        .dir("templates")
        .file("config.kdl", details.config())
        .file("content/index.typ", templates::INDEX)
        .file("content/posts/hello.typ", templates::HELLO)
        .file("templates/layout.typ", templates::LAYOUT)
        .file("assets/style.css", templates::STYLE)
        .apply(ui)?;

    if let Some(vcs) = repo {
        Repo::new(&target, vcs).setup(ui)?;
    }

    ui.blank();
    ui.done_plain(format_args!(
        "project ready in {}",
        Paths(&target.display().to_string())
    ));
    ui.detail(format_args!(
        "run {} to build, {} for a live preview",
        "baudelaire build".cyan(),
        "baudelaire serve".cyan()
    ));
    Ok(())
}

/// Project metadata for a fresh scaffold: prompted interactively, or defaulted
/// from the target directory name and git config.
struct Details {
    site: String,
    author: String,
    url: String,
}

impl Details {
    /// explicit `dir`: derive site name from last component, skip site name prompt
    /// no `dir` + interactive: prompt for site name; that name becomes the target directory
    /// no `dir` + non-interactive (--yes / CI): scaffold into `.`, derive name from cwd
    fn gather(dir: Option<&Path>, root: &Root, interactive: bool) -> Result<(PathBuf, Self)> {
        let author = Self::git_author().unwrap_or_default();

        match dir {
            Some(d) => {
                let site = Self::dir_name(d, root);
                let (author, url) = if interactive {
                    (
                        Input::new("Author").default(&author).ask()?,
                        Input::new("Base URL")
                            .default("https://example.com")
                            .ask()?,
                    )
                } else {
                    (author, "https://example.com".into())
                };
                Ok((d.to_path_buf(), Self { site, author, url }))
            }
            None => {
                if interactive {
                    let site = Input::new("Site name").default("my-site").ask()?;
                    let author = Input::new("Author").default(&author).ask()?;
                    let url = Input::new("Base URL")
                        .default("https://example.com")
                        .ask()?;
                    let target = PathBuf::from(&site);
                    Ok((target, Self { site, author, url }))
                } else {
                    // non-interactive / CI: preserve old behavior & scaffold into the current directory & derive the site name from it
                    let dot = Path::new(".");
                    let site = Self::dir_name(dot, root);
                    Ok((
                        dot.to_path_buf(),
                        Self {
                            site,
                            author,
                            url: "https://example.com".into(),
                        },
                    ))
                }
            }
        }
    }

    /// The scaffolded `config.kdl`, its placeholders filled in.
    fn config(&self) -> String {
        templates::render(
            templates::CONFIG,
            &[
                ("site", &self.site),
                ("author", &self.author),
                ("url", &self.url),
            ],
        )
    }

    /// A sensible default site name from the target directory's last component.
    fn dir_name(target: &Path, root: &Root) -> String {
        root.join(target)
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|n| !n.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| "my-site".into())
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
/// inferred for it — title from the filename, the ordering field from the
/// collection (a `date` for a dated collection, the next `order` for an ordered
/// one), the template, and the permalink it will occupy (with any existing
/// occupant). The operation is a type, not a free function: [`plan`](Self::plan)
/// reads the config and existing content to infer, then [`create`](Self::create)
/// writes. Only standard frontmatter fields are written; content is the author's.
pub(crate) struct Draft {
    /// The file to write; a bundle resolves to `<dir>/index.typ`.
    path: PathBuf,
    title: String,
    template: String,
    date: Option<time::Date>,
    order: Option<i64>,
    is_draft: bool,
    permalink: String,
    /// The source of an existing page already producing `permalink`, if any.
    collision: Option<String>,
    /// Whether to open the created file in `$EDITOR`.
    open: bool,
}

impl Draft {
    /// Infer everything for the page named by `args`, reading the collection
    /// config and the existing content. Errors if the target already exists.
    pub(crate) fn plan(args: &NewArgs, config: &Config, project: &Project) -> Result<Self> {
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
        // Discover once — reused for the next order and the collision check.
        // A discovery failure (e.g. a broken sibling page) must not block `new`.
        let discovered = crate::content::discover(config, project).unwrap_or_default();

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
        // to `/{slug}/` — `permalink_of` owns that fallback, exactly as the build.
        let permalink = Page::permalink_of(collection.as_deref(), &frontmatter, &slug, config);

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
            is_draft: args.draft.unwrap_or(true),
            permalink,
            collision,
            open: args.open,
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
        Scaffold::new(self.path.parent().unwrap_or(Path::new(".")))
            .file(name, self.body())
            .apply(ui)?;
        ui.done(format_args!(
            "created {} {} {}",
            Paths(&self.path.display().to_string()),
            "→".dimmed(),
            self.permalink.cyan()
        ));
        if self.open {
            Editor::open(&self.path, ui);
        }
        Ok(())
    }

    /// The name behind the page's slug: the file stem, or — for a bundle
    /// `index` — the directory it lives in, so `posts/hello/index.typ` is titled
    /// "Hello", not "Index".
    fn raw_name(path: &Path, config: &Config) -> String {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("index");
        let index = config.index.as_deref().unwrap_or("index");
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
    /// capitalize each word (`my-first-post` → `My First Post`).
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
    /// 1 for the first page — so a new chapter appends to the end.
    fn next_order(collection: &str, discovered: &[Collection]) -> i64 {
        discovered
            .iter()
            .filter(|c| c.id == collection)
            .flat_map(|c| c.pages.iter())
            .filter_map(|p| p.frontmatter.order)
            .max()
            .map_or(1, |highest| highest + 1)
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
        fields.push(("draft", Value::Bool(self.is_draft)));
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

impl Vcs {
    /// The command that initializes a repository, and the marker directory whose
    /// presence means one already exists. Jujutsu colocates a `.git`, so it stays
    /// interoperable with git tooling.
    fn init(self) -> (&'static [&'static str], &'static str) {
        match self {
            Self::Git => (&["git", "init", "-q"], ".git"),
            Self::Jujutsu => (&["jj", "git", "init", "--colocate"], ".jj"),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Git => "git",
            Self::Jujutsu => "jujutsu",
        }
    }
}

/// Optional version-control setup for a freshly scaffolded project: a
/// `.gitignore` plus a repository. Opt-in, because not every scaffold wants one.
struct Repo<'a> {
    root: &'a Path,
    vcs: Vcs,
}

impl<'a> Repo<'a> {
    const IGNORE: &'static str = include_str!("scaffold/gitignore");

    fn new(root: &'a Path, vcs: Vcs) -> Self {
        Self { root, vcs }
    }

    /// Which VCS to set up, if any. An explicit `--vcs` wins; `yes` takes the
    /// default (git) without asking; otherwise ask, but only when the session is
    /// `interactive` (decided once, in `init`) — piped or CI input defaults to
    /// none, so a scaffold never blocks nor creates a repo unbidden.
    fn wanted(yes: bool, interactive: bool, explicit: Option<Vcs>) -> Result<Option<Vcs>> {
        if explicit.is_some() {
            return Ok(explicit);
        }
        if yes {
            return Ok(Some(Vcs::Git));
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

    /// Write `.gitignore` and initialize the repository, skipping either step if
    /// it already exists. A missing or failing tool is a warning, not an error:
    /// the project is scaffolded either way.
    fn setup(&self, ui: &Ui) -> Result<()> {
        let ignore = self.root.join(".gitignore");
        if !ignore.exists() {
            fs::write(&ignore, Self::IGNORE)?;
            ui.detail(format_args!("{} {}", "+".green(), Paths(".gitignore")));
        }
        let (argv, marker) = self.vcs.init();
        if self.root.join(marker).exists() {
            return Ok(());
        }
        let (cmd, args) = argv.split_first().expect("non-empty argv");
        // Capture the tool's output rather than inherit it — jj in particular
        // prints an "Initialized repo" line and a hint that would clutter the
        // scaffold log. Surface it only if the command actually failed.
        match Command::new(cmd).args(args).current_dir(self.root).output() {
            Ok(out) if out.status.success() => {
                ui.detail(format_args!(
                    "{} {} repository",
                    "+".green(),
                    self.vcs.label()
                ));
            }
            Ok(out) => {
                let detail = String::from_utf8_lossy(&out.stderr);
                let detail = detail.trim();
                ui.warn(VcsFailed {
                    tool: cmd,
                    detail: (!detail.is_empty()).then(|| detail.to_owned()),
                });
            }
            Err(_) => ui.warn(VcsMissing { tool: cmd }),
        }
        Ok(())
    }
}

/// Scaffold templates, embedded from `scaffold/` at build time. Editing those
/// files — not string literals here — changes what `init`/`new` produce.
mod templates {
    /// Site config; `{{site}}`, `{{author}}`, `{{url}}` are filled by [`render`].
    pub const CONFIG: &str = include_str!("scaffold/config.kdl");
    pub const INDEX: &str = include_str!("scaffold/index.typ");
    pub const HELLO: &str = include_str!("scaffold/hello.typ");
    pub const LAYOUT: &str = include_str!("scaffold/layout.typ");
    pub const STYLE: &str = include_str!("scaffold/style.css");

    /// Substitute `{{key}}` placeholders in a template, in a single left-to-
    /// right pass: a substituted value is never rescanned, so a site name
    /// containing `{{author}}` stays literal. Values are escaped for the
    /// double-quoted string context they land in (KDL and typst share `\`
    /// and `"` escapes), so a quote in a site name yields valid config.
    /// Unknown placeholders are left untouched.
    pub fn render(template: &str, vars: &[(&str, &str)]) -> String {
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
            match vars.iter().find(|(k, _)| *k == key) {
                Some((_, value)) => out.push_str(&escape(value)),
                None => {
                    out.push_str("{{");
                    out.push_str(key);
                    out.push_str("}}");
                }
            }
            rest = &after[close + 2..];
        }
        out.push_str(rest);
        out
    }

    /// Escape a value for interpolation into a double-quoted string literal.
    fn escape(value: &str) -> String {
        value.replace('\\', "\\\\").replace('"', "\\\"")
    }

    #[cfg(test)]
    mod tests {
        use super::render;

        #[test]
        fn fills_known_placeholders() {
            let out = render(
                "site \"{{site}}\" by {{author}}",
                &[("site", "S"), ("author", "A")],
            );
            assert_eq!(out, "site \"S\" by A");
        }

        #[test]
        fn escapes_quotes_and_backslashes_in_values() {
            let out = render("site \"{{site}}\"", &[("site", "My \"Quoted\\\" Site")]);
            assert_eq!(out, "site \"My \\\"Quoted\\\\\\\" Site\"");
        }

        #[test]
        fn substituted_values_are_never_rescanned() {
            let out = render(
                "{{site}} by {{author}}",
                &[("site", "{{author}}"), ("author", "Me")],
            );
            assert_eq!(out, "{{author}} by Me");
        }

        #[test]
        fn unknown_placeholders_are_left_alone() {
            assert_eq!(
                render("keep {{unknown}}", &[("site", "S")]),
                "keep {{unknown}}"
            );
        }

        #[test]
        fn unterminated_braces_pass_through() {
            assert_eq!(
                render("dangling {{site", &[("site", "S")]),
                "dangling {{site"
            );
        }
    }
}
