export interface SearchOptions {
  /** The index to fetch. Defaults to the one this build emitted. */
  url?: string;
  limit?: number;
  placeholder?: string;
  /** The key that opens the palette. `/` unless named. */
  hotkey?: string;
  /** `false` opts out of the client's own stylesheet. */
  styles?: boolean;
}

/** One hit: an indexed document, whose fields follow `generate { search { fields } }`. */
export type Hit = Record<string, unknown>;

export interface Search {
  (query: string, options?: { limit?: number }): Hit[];
  /** Whether the index failed to load, which is why a search finds nothing. */
  failed: boolean;
}

export function createSearch(url?: string): Promise<Search>;
export function mountSearch(options?: SearchOptions): unknown;
