//! Scaffolding one content page: its frontmatter, its order, and opening it.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use owo_colors::OwoColorize;

use super::Scaffold;
use crate::cli::NewArgs;
use crate::codegen::Value;
use crate::config::{Config, SortKey};
use crate::content::{Collection, Frontmatter, Page, Slug};
use crate::error::Result;
use crate::error::warning::PermalinkTaken;
use crate::ui::{Paths, Ui};
use crate::world::Project;

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
    /// The layout to bind, absent when the config resolves none: the page then
    /// carries no `template` key rather than one naming a file nothing wrote.
    template: Option<String>,
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
        let template = config.scaffold_template(collection.as_deref());
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
        if let Some(template) = &self.template {
            fields.push(("template", Value::str(template)));
        }

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
pub(super) struct Editor;
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
