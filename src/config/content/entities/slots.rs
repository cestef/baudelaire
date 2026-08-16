//! `content { entities { <id> { slots } } }`: which field answers each question
//! a renderer asks of an entity.
//!
//! The one indirection that keeps everything downstream from being written
//! about people: a `series` binds `display` to `title` and `image` to `cover`,
//! and the same code renders it.

use crate::config::dispatch::Kind::Text;
use crate::config::dispatch::{Attributed, Attrs};
use crate::config::value::ValueExt;

/// Which field fills each semantic role, by name. Every slot is optional: a
/// registry that fills none still resolves, and the fallbacks in
/// [`crate::content::entities`] answer for what is missing.
#[derive(Debug, Clone, Default, Hash)]
pub struct Slots {
    /// The name a reader sees.
    pub display: Option<String>,
    /// The entity's own canonical URL, off this site.
    pub url: Option<String>,
    /// A picture of it: an avatar, a logo, a cover.
    pub image: Option<String>,
    /// A contact address.
    pub email: Option<String>,
    /// Other URLs that are also this entity, for `sameAs` and the like.
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

/// One `slots display="name" image="avatar"` line.
impl Attributed for Slots {
    const ATTRS: Attrs<Self> = Attrs(&[
        (
            "display",
            Text,
            "The field holding the name a reader sees. Defaults to `name`, then `title`, then the entity's own id.",
            |c, v, t, s| {
                c.display = Some(v.as_str(t, s)?);
                Ok(())
            },
        ),
        (
            "url",
            Text,
            "The field holding the entity's own canonical URL, off this site.",
            |c, v, t, s| {
                c.url = Some(v.as_str(t, s)?);
                Ok(())
            },
        ),
        (
            "image",
            Text,
            "The field holding a picture of it: an avatar, a logo, a cover.",
            |c, v, t, s| {
                c.image = Some(v.as_str(t, s)?);
                Ok(())
            },
        ),
        (
            "email",
            Text,
            "The field holding a contact address.",
            |c, v, t, s| {
                c.email = Some(v.as_str(t, s)?);
                Ok(())
            },
        ),
        (
            "same-as",
            Text,
            "The field holding the other URLs that are also this entity.",
            |c, v, t, s| {
                c.same_as = Some(v.as_str(t, s)?);
                Ok(())
            },
        ),
    ]);
}

#[cfg(test)]
mod tests {
    use super::Slots;
    use crate::config::dispatch::Attributed;

    /// Every slot [`Slots::filled`] reports has to be a key the block accepts,
    /// since that name goes into a diagnostic as advice.
    #[test]
    fn every_reported_slot_is_a_key_the_block_takes() {
        let filled = Slots {
            display: Some("a".into()),
            url: Some("b".into()),
            image: Some("c".into()),
            email: Some("d".into()),
            same_as: Some("e".into()),
        };
        let keys: Vec<&str> = Slots::rows().into_iter().map(|row| row.key).collect();
        let reported: Vec<&str> = filled.filled().into_iter().map(|(slot, _)| slot).collect();
        assert_eq!(reported, keys);
    }
}
