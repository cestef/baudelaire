export interface SearchOptions {
  /** The index to fetch. Defaults to the one this page's language emitted. */
  url?: string;
  limit?: number;
  placeholder?: string;
  /** The key that opens the palette, alongside Cmd/Ctrl-K. */
  hotkey?: string;
  /** `false` opts out of the client's own stylesheet. */
  styles?: boolean;
}

/** One hit: a document as the index carries it. */
export interface Hit {
  url: string;
  title?: string;
  tags?: string[];
  /** The page's prose, as much of it as `generate { search { snippet } }` ships. */
  text?: string;
}

export interface Search {
  (query: string, options?: { limit?: number }): Hit[];
  /** Whether the index failed to load, which is why a search finds nothing. */
  failed: boolean;
  /** The base path a hit's `url` is served under. */
  base: string;
  /** How many characters of context a hit shows; `0` shows none. */
  snippet: number;
}

export function createSearch(url?: string): Promise<Search>;
export function mountSearch(options?: SearchOptions): unknown;
