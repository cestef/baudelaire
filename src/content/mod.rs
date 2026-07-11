pub mod eval;
pub mod frontmatter;
pub mod listing;
pub mod page;
pub mod pagination;
pub mod permalink;
pub mod slug;
pub mod taxonomy;

pub use frontmatter::{Extract, Frontmatter};
pub use page::{Collection, Page, PageId, discover};
pub use slug::Slug;
pub use pagination::Pagination;
pub use permalink::{Permalink, PermalinkCtx, PermalinkError};
pub use taxonomy::Taxonomy;
