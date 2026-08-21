//! `content { entities { <id> { slots } } }`: which field answers each question
//! a renderer asks of an entity.
//!
//! The one indirection that keeps everything downstream from being written
//! about people: a `series` binds `display` to `title` and `image` to `cover`,
//! and the same code renders it.

use dispatch_derive::Table;

use crate::config::dispatch::{Attributed, Attrs};
use crate::config::vocab::attr;

/// Which field fills each semantic role, by name. Every slot is optional: a
/// registry that fills none still resolves, and the fallbacks in
/// [`crate::content::entities`] answer for what is missing.
///
/// One `slots display="name" image="avatar"` line.
#[derive(Debug, Clone, Default, Hash, Table)]
#[table(impl = Attributed, const ATTRS: Attrs<Self> = Attrs, rule = attr)]
pub struct Slots {
    /// The field holding the name a reader sees. Defaults to `name`, then `title`, then the entity's own id.
    #[key(opt text)]
    pub display: Option<String>,

    /// The field holding the entity's own canonical URL, off this site.
    #[key(opt text)]
    pub url: Option<String>,

    /// The field holding a picture of it: an avatar, a logo, a cover.
    #[key(opt text)]
    pub image: Option<String>,

    /// The field holding a contact address.
    #[key(opt text)]
    pub email: Option<String>,

    /// The field holding the other URLs that are also this entity.
    #[key(name = "same-as", opt text)]
    pub same_as: Option<String>,
}

impl Slots {
    /// Every slot this registry fills, as `(slot, field)`. Destructures the
    /// whole struct, so a new slot fails to compile until it is listed here.
    pub fn filled(&self) -> Vec<(&'static str, &str)> {
        let Self {
            display,
            url,
            image,
            email,
            same_as,
        } = self;
        [
            ("display", display),
            ("url", url),
            ("image", image),
            ("email", email),
            ("same-as", same_as),
        ]
        .into_iter()
        .filter_map(|(slot, field)| field.as_deref().map(|field| (slot, field)))
        .collect()
    }

    /// Fill every slot `self` leaves empty from `defaults`, which is how a
    /// registry naming a `shape` inherits that shape's slots.
    pub fn under(&mut self, defaults: &Self) {
        let Self {
            display,
            url,
            image,
            email,
            same_as,
        } = self;
        for (slot, default) in [
            (display, &defaults.display),
            (url, &defaults.url),
            (image, &defaults.image),
            (email, &defaults.email),
            (same_as, &defaults.same_as),
        ] {
            if slot.is_none() {
                slot.clone_from(default);
            }
        }
    }
}
