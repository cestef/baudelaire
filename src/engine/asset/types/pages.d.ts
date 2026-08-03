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
  /** Taxonomy name to the terms this page carries. */
  taxonomies: Record<string, string[]>;
  /** Whatever else the page's frontmatter declared. */
  extra: Record<string, unknown>;
}

const pages: Entry[];
export default pages;
