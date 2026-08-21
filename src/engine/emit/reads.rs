//! What a post-build processor reads off the finished site, and the digest of
//! each slice, so one whose inputs are unchanged does not run again.

use std::collections::BTreeMap;

use crate::content::{Data, Page};
use crate::graph::Hash;

use super::{Output, Site};

/// One slice of a built site.
///
/// A processor declares the slices it reads and is skipped when none of them
/// changed, so a slice left out of that list is a file that stops being
/// regenerated. The config is not one: a change there invalidates the whole
/// manifest, this memo with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::engine) enum Reads {
    /// The page set as a listing sees it: identity, permalink, frontmatter,
    /// language and output path. Not a page's own prose, which reaches a
    /// listing through its rendered [`markup`](Reads::Markup).
    Listing,
    /// Every page's rendered markup, exactly as written to `dist`.
    Markup,
    /// What the render pass produced besides the markup: the fragments a
    /// single-file export assembles, the prose a full feed publishes, and the
    /// digests of a page's inline scripts and styles.
    Rendered,
    /// The entity registries a byline resolves through.
    Entities,
    /// Where each page sits among the others.
    Relations,
}

impl Reads {
    fn digest(self, site: &Site) -> Hash {
        match self {
            Self::Listing => Hash::of(&site.pages.iter().map(Listed).collect::<Vec<_>>()),
            Self::Markup => Hash::of(&site.outputs.iter().map(|out| out.html).collect::<Vec<_>>()),
            Self::Rendered => Hash::of(&site.outputs.iter().map(Produced).collect::<Vec<_>>()),
            Self::Entities => Hash::of(site.entities),
            Self::Relations => Hash::of(site.relations),
        }
    }
}

/// The digest of each slice, taken at most once per build and only for a slice
/// some enabled processor declares.
#[derive(Default)]
pub(super) struct Digests(BTreeMap<Reads, Hash>);

impl Digests {
    /// The one fingerprint covering everything `reads` names, or `None` for the
    /// processor that declared none and so runs every build.
    pub(super) fn of(&mut self, site: &Site, reads: Option<&'static [Reads]>) -> Option<Hash> {
        let parts: Vec<Hash> = reads?
            .iter()
            .map(|&slice| *self.0.entry(slice).or_insert_with(|| slice.digest(site)))
            .collect();
        Some(Hash::of(&parts))
    }
}

/// One page as every surface that merely *lists* pages sees it.
struct Listed<'a>(&'a Page);

impl std::hash::Hash for Listed<'_> {
    /// `body` is left out on purpose: a page's prose reaches a listing only
    /// through the markup it rendered to, which is [`Reads::Markup`], and
    /// hashing it here would rebuild every sitemap on every edit.
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let Page {
            id,
            source,
            frontmatter,
            body: _,
            data,
            collection,
            permalink,
            output,
            template,
            lang,
        } = self.0;
        (
            &id.0,
            source,
            frontmatter,
            collection,
            permalink,
            output,
            template,
            lang,
        )
            .hash(state);
        Listing(data).hash(state);
    }
}

/// What a page's [`Data`] tells a listing.
struct Listing<'a>(&'a Data);

impl std::hash::Hash for Listing<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self.0).hash(state);
        match self.0 {
            Data::Export | Data::Empty => {}
            #[cfg(feature = "markdown")]
            Data::Lowered {
                dict,
                sourcemap: _,
                reading,
            } => (dict, reading).hash(state),
            Data::Generated { dict, lists } => (dict, lists).hash(state),
        }
    }
}

/// What the render pass produced for one page besides its markup.
struct Produced<'a>(&'a Output<'a>);

impl std::hash::Hash for Produced<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let Output {
            page,
            html: _,
            fragments,
            syndicated,
            inline,
        } = self.0;
        (&page.permalink, fragments, syndicated, inline).hash(state);
    }
}
