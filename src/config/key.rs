//! A dotted config path, resolved against the dispatch tables.
//!
//! A segment the tables know continues the path; one they do not is a name the
//! author chose, which is what lets `content.collections.blog.sort` mean `sort`
//! under `content.collections`.

use super::dispatch::Kind;
use super::reference::Reference;

/// A dotted key as it was typed.
pub struct Key<'a>(&'a str);

impl<'a> Key<'a> {
    pub fn new(key: &'a str) -> Self {
        Self(key)
    }

    pub fn segments(&self) -> Vec<&'a str> {
        self.0.split('.').collect()
    }

    /// The table path this names, the author's own names dropped, or `None`
    /// when no table has it.
    pub fn resolved(&self) -> Option<String> {
        let mut known = String::new();
        for segment in self.segments() {
            let below = if known.is_empty() {
                segment.to_owned()
            } else {
                format!("{known}.{segment}")
            };
            if Reference::at(&below).is_some() {
                known = below;
            }
        }
        let last = self.segments().last().copied()?;
        let resolves = known.rsplit('.').next() == Some(last);
        resolves.then_some(known)
    }

    /// The shape of the key this names.
    pub fn kind(&self) -> Option<Kind> {
        let resolved = self.resolved()?;
        Reference::at(&resolved).map(|reference| reference.entries()[0].kind)
    }
}

#[cfg(test)]
mod tests {
    use super::Key;

    #[test]
    fn a_table_key_resolves_to_itself() {
        assert_eq!(
            Key::new("paths.dist").resolved().as_deref(),
            Some("paths.dist")
        );
    }

    #[test]
    fn a_name_the_author_chose_is_stepped_over() {
        assert_eq!(
            Key::new("content.collections.blog.sort")
                .resolved()
                .as_deref(),
            Some("content.collections.sort")
        );
    }

    #[test]
    fn a_path_whose_last_segment_is_no_key_resolves_to_nothing() {
        assert!(Key::new("paths.dsit").resolved().is_none());
        assert!(Key::new("content.collections.blog").resolved().is_none());
    }
}
