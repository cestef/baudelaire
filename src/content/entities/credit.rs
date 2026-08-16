//! What a reference *claims* about an entity a page names, and how each output
//! surface spells it: one row per role and one column per surface, in
//! [`SPELLINGS`].

use crate::config::{Config, Named, Slots};
use crate::content::{Page, entities::Registries};

use super::{Entity, Registry};

/// What a page claims about an entity it names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Credit {
    Author,
    Editor,
    Translator,
    Contributor,
    Illustrator,
    Reviewer,
    Publisher,
}

impl Named for Credit {
    const NAMES: &'static [(&'static str, Self)] = &[
        ("author", Self::Author),
        ("editor", Self::Editor),
        ("translator", Self::Translator),
        ("contributor", Self::Contributor),
        ("illustrator", Self::Illustrator),
        ("reviewer", Self::Reviewer),
        ("publisher", Self::Publisher),
    ];
}

/// An output surface that can name who is behind a page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vocabulary {
    /// `<meta name="author">`, the oldest of them and the only one every reader
    /// agrees on.
    Meta,
    /// OpenGraph's `article:*`, which a link preview reads.
    OpenGraph,
    /// The schema.org island, the only vocabulary that can say every role.
    JsonLd,
    /// An Atom entry, whose two person constructs are fixed by RFC 4287.
    Atom,
    /// A bundled document's metadata: the PDF info dict, the card's byline.
    Document,
}

/// How each surface spells each role, and what it simply cannot say; a surface
/// handed a role it has no name for writes nothing rather than inventing a
/// field.
const SPELLINGS: &[(Credit, &[(Vocabulary, &str)])] = &[
    (
        Credit::Author,
        &[
            (Vocabulary::Meta, "author"),
            (Vocabulary::OpenGraph, "article:author"),
            (Vocabulary::JsonLd, "author"),
            (Vocabulary::Atom, "author"),
            (Vocabulary::Document, "author"),
        ],
    ),
    (Credit::Editor, &[(Vocabulary::JsonLd, "editor")]),
    (Credit::Translator, &[(Vocabulary::JsonLd, "translator")]),
    (
        Credit::Contributor,
        &[
            (Vocabulary::JsonLd, "contributor"),
            (Vocabulary::Atom, "contributor"),
        ],
    ),
    (Credit::Illustrator, &[(Vocabulary::JsonLd, "illustrator")]),
    (Credit::Reviewer, &[(Vocabulary::JsonLd, "reviewedBy")]),
    (Credit::Publisher, &[(Vocabulary::JsonLd, "publisher")]),
];

impl Credit {
    /// How `vocabulary` spells this role, if it can say it at all.
    pub fn spelling(self, vocabulary: Vocabulary) -> Option<&'static str> {
        SPELLINGS
            .iter()
            .find(|(credit, _)| *credit == self)
            .and_then(|(_, columns)| {
                columns
                    .iter()
                    .find(|(surface, _)| *surface == vocabulary)
                    .map(|(_, spelling)| *spelling)
            })
    }
}

/// The schema.org kind a registry that declares no `shape` holds.
const KIND: &str = "Person";

/// One entity as a page named it: what the registry knows, resolved through the
/// registry's slots. A term the registry does not hold is still one of these,
/// with the term itself as all there is to know.
#[derive(Clone, Copy)]
pub struct Resolved<'a> {
    /// The term the page wrote, which is the display name when nothing else is.
    pub term: &'a str,
    entity: Option<&'a Entity>,
    slots: &'a Slots,
    /// The language of the page that named it, which is the language a profile
    /// answers in.
    lang: &'a str,
    kind: &'static str,
}

impl<'a> Resolved<'a> {
    fn new(registry: &'a Registry, term: &'a str, lang: &'a str) -> Self {
        Self {
            term,
            lang,
            entity: registry.get(term),
            slots: registry.slots(),
            kind: registry.shape().map_or(KIND, crate::config::Shape::schema),
        }
    }

    /// A synthesized entity: a name and nothing else.
    pub fn bare(term: &'a str, slots: &'a Slots) -> Self {
        Self {
            term,
            lang: Entity::BASE,
            entity: None,
            slots,
            kind: KIND,
        }
    }

    pub fn kind(&self) -> &'static str {
        self.kind
    }

    /// The name a reader sees: the display slot, else a `name` or `title`
    /// field, else the term itself.
    pub fn display(&self) -> &str {
        self.slot(self.slots.display.as_deref())
            .or_else(|| self.text("name"))
            .or_else(|| self.text("title"))
            .unwrap_or(self.term)
    }

    /// The entity's own canonical URL, off this site.
    pub fn url(&self) -> Option<&str> {
        self.slot(self.slots.url.as_deref())
    }

    pub fn image(&self) -> Option<&str> {
        self.slot(self.slots.image.as_deref())
    }

    pub fn email(&self) -> Option<&str> {
        self.slot(self.slots.email.as_deref())
    }

    /// The other URLs that are also this entity, however the field spells them:
    /// a list of URLs, or a dictionary of platform to URL.
    pub fn same_as(&self) -> Vec<&str> {
        let Some(value) = self
            .slots
            .same_as
            .as_deref()
            .and_then(|key| self.field(key))
        else {
            return Vec::new();
        };
        match value {
            crate::codegen::Value::Str(one) => vec![one.as_str()],
            crate::codegen::Value::Array(many) => many
                .iter()
                .filter_map(crate::codegen::Value::as_str)
                .collect(),
            crate::codegen::Value::Dict(pairs) => pairs
                .iter()
                .filter_map(|(_, value)| value.as_str())
                .collect(),
            _ => Vec::new(),
        }
    }

    pub fn field(&self, key: &str) -> Option<&'a crate::codegen::Value> {
        self.entity?.field(key, self.lang)
    }

    /// Everything the registry holds about it, as a dict.
    pub fn fields(&self) -> crate::codegen::Value {
        crate::codegen::Value::dict(
            self.entity
                .map(|entity| entity.fields(self.lang))
                .unwrap_or_default()
                .into_iter()
                .map(|(key, value)| ((*key).to_owned(), value.clone())),
        )
    }

    /// The string value of the field a slot names.
    fn slot(&self, key: Option<&str>) -> Option<&'a str> {
        self.text(key?)
    }

    /// A field's string value, an empty one treated as absent.
    fn text(&self, key: &str) -> Option<&'a str> {
        self.field(key)
            .and_then(crate::codegen::Value::as_str)
            .filter(|text| !text.is_empty())
    }
}

/// One credited entity, as a surface writes it: the slots resolved to owned
/// strings.
#[derive(Debug, Clone, PartialEq)]
pub struct Attribution {
    pub display: String,
    /// Its own canonical URL, off this site.
    pub url: Option<String>,
    pub image: Option<String>,
    pub email: Option<String>,
    /// The other URLs that are also this entity.
    pub same_as: Vec<String>,
    /// What schema.org calls it.
    pub kind: &'static str,
    /// Everything the registry holds about it, under the names it holds them.
    pub fields: crate::codegen::Value,
}

impl From<&Resolved<'_>> for Attribution {
    fn from(resolved: &Resolved<'_>) -> Self {
        Self {
            display: resolved.display().to_owned(),
            url: resolved.url().map(str::to_owned),
            image: resolved.image().map(str::to_owned),
            email: resolved.email().map(str::to_owned),
            same_as: resolved.same_as().into_iter().map(str::to_owned).collect(),
            kind: resolved.kind(),
            fields: resolved.fields(),
        }
    }
}

/// One credited entity as a template reads it: the slot answers under fixed
/// names, plus the entity's own fields under theirs.
impl From<&Attribution> for crate::codegen::Value {
    fn from(credited: &Attribution) -> Self {
        Self::dict([
            ("name", Self::str(&credited.display)),
            ("url", Self::opt(credited.url.clone())),
            ("image", Self::opt(credited.image.clone())),
            ("email", Self::opt(credited.email.clone())),
            (
                "same-as",
                Self::array(credited.same_as.iter().map(Self::str)),
            ),
            ("fields", credited.fields.clone()),
        ])
    }
}

/// A page's byline as a template reads it: one key per role it credits, a role
/// with nobody in it absent rather than empty.
impl From<&Byline> for crate::codegen::Value {
    fn from(byline: &Byline) -> Self {
        Self::dict(
            byline.roles().map(|(role, credited)| {
                (role.name(), Self::array(credited.iter().map(Self::from)))
            }),
        )
    }
}

/// Who is behind one page, by role, resolved once and then spelled by every
/// surface.
#[derive(Debug, Clone, Default)]
pub struct Byline(Vec<(Credit, Vec<Attribution>)>);

impl Byline {
    /// The byline of `page`: every entity it credits, plus the page's own
    /// `author` field where it credits no author through a taxonomy. The
    /// site-wide floor is *not* applied here; [`Byline::or_site`] adds it for
    /// the surfaces that want it.
    pub fn of(registries: &Registries, config: &Config, page: &Page) -> Self {
        let mut byline = Self::default();
        for reference in registries.references(config, page) {
            let Some(credit) = reference.credit else {
                continue;
            };
            byline.push(
                credit,
                vec![Attribution::from(&Resolved::new(
                    reference.registry,
                    reference.term,
                    &page.lang,
                ))],
            );
        }
        if byline.authors().is_empty()
            && let Some(name) = page.frontmatter.author.as_deref()
        {
            byline.push(Credit::Author, vec![Self::bare(name)]);
        }
        byline
    }

    /// Fill in the site's own `author` for this page's language, where the page
    /// names no author of its own.
    pub fn or_site(mut self, config: &Config, page: &Page) -> Self {
        if self.authors().is_empty()
            && let Some(name) = config.author(&page.lang)
        {
            self.push(Credit::Author, vec![Self::bare(name)]);
        }
        self
    }

    /// An entity that is a name and nothing else.
    fn bare(name: &str) -> Attribution {
        let slots = Slots::default();
        Attribution::from(&Resolved::bare(name, &slots))
    }

    /// The same byline with every picture rewritten by `resolve`, which is how
    /// an entity's image reaches the URL the pipeline serves it from.
    #[must_use]
    pub fn images(mut self, mut resolve: impl FnMut(&str) -> String) -> Self {
        for (_, credited) in &mut self.0 {
            for one in credited.iter_mut() {
                one.image = one.image.as_deref().map(&mut resolve);
            }
        }
        self
    }

    /// Credit `role` to `named`, keeping roles in declaration order.
    pub fn push(&mut self, role: Credit, named: Vec<Attribution>) {
        match self.0.iter_mut().find(|(credit, _)| *credit == role) {
            Some((_, known)) => known.extend(named),
            None => self.0.push((role, named)),
        }
        self.0.sort_by_key(|(credit, _)| *credit);
    }

    /// The entities credited with `role`.
    pub fn get(&self, role: Credit) -> &[Attribution] {
        self.0
            .iter()
            .find(|(credit, _)| *credit == role)
            .map_or(&[], |(_, named)| named.as_slice())
    }

    /// Every role this page names, with its entities, in role order.
    pub fn roles(&self) -> impl Iterator<Item = (Credit, &[Attribution])> {
        self.0
            .iter()
            .map(|(credit, named)| (*credit, named.as_slice()))
    }

    /// Who wrote it: the one role every surface can spell.
    pub fn authors(&self) -> &[Attribution] {
        self.get(Credit::Author)
    }

    /// The credited names joined as one line, for the surfaces that carry a
    /// single string: a PDF info dict, a card byline.
    pub fn line(&self, role: Credit) -> Option<String> {
        let named = self.get(role);
        if named.is_empty() {
            None
        } else {
            Some(
                named
                    .iter()
                    .map(|one| one.display.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Credit, SPELLINGS, Vocabulary};
    use crate::config::Named;

    #[test]
    fn every_role_has_a_row_and_says_something_somewhere() {
        for (name, credit) in Credit::NAMES {
            let row = SPELLINGS
                .iter()
                .find(|(role, _)| role == credit)
                .unwrap_or_else(|| panic!("`{name}` has no row in the spelling table"));
            assert!(
                !row.1.is_empty(),
                "`{name}` is spelled by no vocabulary at all"
            );
            assert!(
                credit.spelling(Vocabulary::JsonLd).is_some(),
                "`{name}` is not spelled by the one vocabulary that can say every role"
            );
        }
    }

    #[test]
    fn atom_spells_only_the_two_roles_it_has() {
        let atom: Vec<&str> = Credit::NAMES
            .iter()
            .filter_map(|(_, credit)| credit.spelling(Vocabulary::Atom))
            .collect();
        assert_eq!(atom, ["author", "contributor"]);
    }
}
