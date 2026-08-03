/** What a term maps to: a page carrying it. */
export interface Link {
  url: string;
  title: string;
  lang: string;
}

/** Taxonomy name to term to the pages carrying it. */
const taxonomies: Record<string, Record<string, Link[]>>;
export default taxonomies;
