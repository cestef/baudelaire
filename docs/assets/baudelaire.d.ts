// Types for the `baudelaire:*` virtual modules the bundler serves. They are
// generated per build, so nothing on disk declares them and an editor (or
// `tsc --noEmit`) would otherwise read every import below as an unknown module.
// Declarations are read for their types and never bundled.

declare module "baudelaire:feed" {
  /** One recent dated page, newest first, as `baudelaire:feed` serves it. */
  export interface Entry {
    url: string;
    title: string;
    lang: string;
    date: string | null;
  }

  const feed: Entry[];
  export default feed;
}

declare module "baudelaire:config" {
  /** This site's own `client { }` block, whatever it holds. */
  const config: Record<string, unknown>;
  export default config;
}

declare module "baudelaire:search" {
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

  export function mountSearch(options?: SearchOptions): unknown;
}
