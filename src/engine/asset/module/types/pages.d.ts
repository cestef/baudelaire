/** One catalogue row, the same shape a generated listing hands its template. */
export interface Entry {
  url: string;
  label: string;
  collection: string;
  lang: string;
  /** ISO 8601, `null` on an undated page. */
  date: string | null;
  /** The date as this page's language renders it. */
  display: string | null;
  note: string | null;
  /** The page's one-line summary, from `description` or its `summary` alias. */
  description: string | null;
  /** The page's own social image, `null` when it declared none. */
  image: string | null;
  /** What that image shows; empty marks it decorative. */
  alt: string | null;
  /** Who wrote the page. The page's own only, never the site default. */
  author: string | null;
  /** Taxonomy name to the terms this page carries. */
  taxonomies: Record<string, string[]>;
  /** Whatever else the page's frontmatter declared: keys baudelaire does not name. */
  extra: Record<string, unknown>;
}

const pages: Entry[];
export default pages;
